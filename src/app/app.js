// Copyright 2026 Google LLC
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     https://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

// AdSpace Agent — Custom Split-Screen Workspace Client (`/app/`)
//
// Features:
// 1. Google Sign-In (`/auth/me`, `/auth/login`, `/auth/logout`) with sessions
//    bound to the authenticated Google user's identity (`user.id`).
// 2. Seamless in-flight OAuth 2.0 popup consent for tool calls via `/oauth2/events`.
// 3. Left-hand Chat interface with SSE streaming (`/api/run_sse`), file attachments,
//    and user-controlled scrolling (auto-scroll disabled by default).
// 4. Right-hand Interactive `adk-ui` Canvas rendering standalone KPI dashboards,
//    SVG charts, sortable/filterable tables, and two-way forms.

import {
  $,
  bindPopoverTrigger,
  closePopover,
  focusableIn,
  isPopoverOpen,
  openPopover,
  showToast,
  style,
} from "./dom.js";
import {
  formatExactInt,
  formatUsdDetail,
  formatUsdShort,
  initialsOf,
  safeImageSrc,
  variantColors,
} from "./format.js";
import {
  APP_NAME,
  DEFAULT_MODELS,
  els,
  prefs,
  PRESET_MODEL_OPTIONS,
  state,
  STORAGE_KEYS,
} from "./state.js";
import { onStatusChange, setStatus, showError } from "./status.js";
import { appendInlineMarkdown, parseMarkdownBlocksInto } from "./markdown.js";
import { animateCounter, prefersReducedMotion, triggerPulseClass } from "./usage/counter.js";
import { renderChartInner } from "./adk-ui/charts.js";
import { renderTableParts } from "./adk-ui/tables.js";
import { AdSpaceStage } from "./stage.js";

const CHANNEL = "adspace:oauth";
const EVENTS_PATH = "/oauth2/events";
const ACK_PATH = "/oauth2/ack";

const toFiniteRate = (v) => {
  const n = Number(v);
  return Number.isFinite(n) && n >= 0 ? n : null;
};

const ingestServerPricing = (models) => {
  const table = models && typeof models === "object" ? models.pricing : null;
  if (!table || typeof table !== "object") return false;
  const next = {};
  for (const [name, p] of Object.entries(table)) {
    if (!p || typeof p !== "object") continue;
    const input = toFiniteRate(p.input_per_million);
    const cached = toFiniteRate(p.cached_input_per_million);
    const output = toFiniteRate(p.output_per_million);
    if (input === null || cached === null || output === null) continue;
    next[String(name).trim().toLowerCase()] = {
      input,
      inputLong: toFiniteRate(p.input_long_per_million) ?? input,
      cached,
      cachedLong: toFiniteRate(p.cached_input_long_per_million) ?? cached,
      output,
      outputLong: toFiniteRate(p.output_long_per_million) ?? output,
      audioIn: toFiniteRate(p.audio_input_per_million) ?? input,
    };
  }
  state.serverModelPricing = next;
  return true;
};

// ---------------------------------------------------------------------------
// Settings & Theme Management
// ---------------------------------------------------------------------------

const syncModelControl = (selectEl, customEl, presets, value, forceCustom = false) => {
  if (!selectEl || !customEl) return;
  const isPreset = !forceCustom && presets.includes(value);
  if (isPreset) {
    selectEl.value = value;
    customEl.classList.add("hidden");
    customEl.value = value;
  } else {
    selectEl.value = "__custom__";
    customEl.classList.remove("hidden");
    if (document.activeElement !== customEl) {
      customEl.value = value;
    }
  }
};

const MODEL_SELECT_IDS = {
  model: "setting-model-select",
  imageModel: "setting-image-model-select",
  videoModel: "setting-video-model-select",
};

// `PRESET_MODEL_OPTIONS` is the single source of truth for the preset lists:
// the <option>s in studio.html are replaced here so the two cannot drift
// (drift would make `syncModelControl` silently fall back to "Custom…").
const populateModelSelects = () => {
  for (const [key, id] of Object.entries(MODEL_SELECT_IDS)) {
    const selectEl = $(id);
    if (!selectEl) continue;
    const frag = document.createDocumentFragment();
    for (const name of PRESET_MODEL_OPTIONS[key]) {
      const opt = document.createElement("option");
      opt.value = name;
      opt.textContent = name;
      frag.appendChild(opt);
    }
    const custom = document.createElement("option");
    custom.value = "__custom__";
    custom.textContent = "Custom…";
    frag.appendChild(custom);
    selectEl.replaceChildren(frag);
  }
};
populateModelSelects();

const syncAllModelControls = () => {
  syncModelControl(
    $(MODEL_SELECT_IDS.model),
    $("setting-model-custom"),
    PRESET_MODEL_OPTIONS.model,
    prefs.model
  );
  syncModelControl(
    $(MODEL_SELECT_IDS.imageModel),
    $("setting-image-model-custom"),
    PRESET_MODEL_OPTIONS.imageModel,
    prefs.imageModel
  );
  syncModelControl(
    $(MODEL_SELECT_IDS.videoModel),
    $("setting-video-model-custom"),
    PRESET_MODEL_OPTIONS.videoModel,
    prefs.videoModel
  );
};

const pushModelsToServer = async () => {
  if (!state.canManageModels) return;
  try {
    const resp = await fetch("/auth/models", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
        model: prefs.model,
        image_model: prefs.imageModel,
        video_model: prefs.videoModel,
      }),
    });
    if (!resp.ok) {
      const errData = await resp.json().catch(() => ({}));
      if (errData.error) showError(`Could not update model: ${errData.error}`);
      return;
    }
    // The response is the refreshed `/auth/models` payload, which prices the
    // newly active model as well as the presets.
    const data = await resp.json().catch(() => null);
    if (ingestServerPricing(data)) applyPreferences({ rerender: false, syncModels: false });
  } catch {
    /* ignore offline / unauthenticated state */
  }
};

// Settings controls, resolved once (they live in the static settings popover).
const settingEls = {
  themeButtons: Array.from(document.querySelectorAll("#theme-segmented button")),
  canvasModeButtons: Array.from(document.querySelectorAll("#canvas-mode-segmented button")),
  showTools: $("setting-show-tools"),
  showSessionUsage: $("setting-show-session-usage"),
  showMessageUsage: $("setting-show-message-usage"),
  showCost: $("setting-show-cost"),
  autoScroll: $("setting-autoscroll"),
};

// Writes every preference to localStorage. Pure persistence, no DOM work.
const persistPrefs = () => {
  localStorage.setItem(STORAGE_KEYS.theme, prefs.theme);
  localStorage.setItem(STORAGE_KEYS.canvasMode, prefs.canvasMode);
  localStorage.setItem(STORAGE_KEYS.showTools, String(prefs.showTools));
  localStorage.setItem(STORAGE_KEYS.showSessionUsage, String(prefs.showSessionUsage));
  localStorage.setItem(STORAGE_KEYS.showMessageUsage, String(prefs.showMessageUsage));
  localStorage.setItem(STORAGE_KEYS.showCost, String(prefs.showCost));
  localStorage.setItem(STORAGE_KEYS.autoScroll, String(prefs.autoScroll));
  localStorage.setItem(STORAGE_KEYS.model, prefs.model);
  localStorage.setItem(STORAGE_KEYS.imageModel, prefs.imageModel);
  localStorage.setItem(STORAGE_KEYS.videoModel, prefs.videoModel);
  localStorage.setItem(STORAGE_KEYS.pricingTier, prefs.pricingTier || "standard");
  localStorage.setItem(STORAGE_KEYS.modelPricing, JSON.stringify(prefs.modelPricing || {}));
};

const syncSegmented = (buttons, dataKey, value) => {
  for (const btn of buttons) {
    const on = btn.dataset[dataKey] === value;
    btn.classList.toggle("active", on);
    btn.setAttribute("aria-pressed", on ? "true" : "false");
  }
};

// Reflects `prefs` into the settings popover controls and the composer hint.
const syncPrefControls = ({ syncModels = true, syncPricing = true } = {}) => {
  syncSegmented(settingEls.themeButtons, "themeChoice", prefs.theme);
  syncSegmented(settingEls.canvasModeButtons, "canvasMode", prefs.canvasMode);
  if (settingEls.showTools) settingEls.showTools.checked = prefs.showTools;
  if (settingEls.showSessionUsage) settingEls.showSessionUsage.checked = prefs.showSessionUsage;
  if (settingEls.showMessageUsage) settingEls.showMessageUsage.checked = prefs.showMessageUsage;
  if (settingEls.showCost) settingEls.showCost.checked = prefs.showCost;
  if (settingEls.autoScroll) settingEls.autoScroll.checked = prefs.autoScroll;
  if (syncModels) {
    syncAllModelControls();
  }
  if (syncPricing) {
    syncPricingControls();
  }
  els.composerContextHint.textContent = `${prefs.model} · Enter to send`;
};

// Entry point used throughout the file: persist, reflect into the controls,
// apply the theme, then re-render (or at least refresh the usage UI).
const applyPreferences = ({
  rerender = true,
  syncModels = true,
  syncPricing = true,
} = {}) => {
  if (prefs.theme === "light" || prefs.theme === "dark") {
    document.documentElement.setAttribute("data-theme", prefs.theme);
  } else {
    document.documentElement.removeAttribute("data-theme");
  }
  persistPrefs();
  syncPrefControls({ syncModels, syncPricing });
  if (typeof syncCanvasStageTheme === "function") {
    syncCanvasStageTheme();
  }

  if (rerender) {
    renderTranscript();
    renderCanvas();
  } else {
    updateSessionUsageUi();
  }
};

const savedWidth = localStorage.getItem(STORAGE_KEYS.leftWidth);
if (savedWidth) {
  document.documentElement.style.setProperty("--left-pane-width", savedWidth);
}

// ---------------------------------------------------------------------------
// OAuth Popup Management (Sign-In & In-Flight Tool Consent)
// ---------------------------------------------------------------------------

const openPopup = (url, name = "adspace_google_auth") => {
  const w = 520;
  const h = 720;
  const left = Math.max(0, (window.screenX || 0) + ((window.outerWidth || w) - w) / 2);
  const top = Math.max(
    0,
    window.outerHeight ? (window.screenY || 0) + (window.outerHeight - h) / 2 : 80
  );
  const win = window.open(
    url,
    name,
    `popup=yes,width=${w},height=${h},left=${left},top=${top}`
  );
  if (win) {
    state.popup = win;
    try {
      win.focus();
    } catch {
      /* ignore */
    }
  }
  return win;
};

const handleAuthRequired = async (data) => {
  // The event carries an opaque `request_id` (never the signed OAuth state).
  if (!data || !data.auth_url || !data.request_id) return;
  if (!state.currentUser || !state.activeSession) return;
  if (data.session_id && data.session_id !== state.activeSession.id) return;
  if (state.handledAuthRequests.has(data.request_id)) return;
  // Defense in depth: the server is trusted, but never hand `window.open` a
  // `javascript:` (or other non-web) URL — the new window inherits our origin.
  let authUrl;
  try {
    authUrl = new URL(data.auth_url, location.href);
  } catch {
    return;
  }
  if (authUrl.protocol !== "https:" && authUrl.origin !== location.origin) return;
  state.handledAuthRequests.add(data.request_id);

  setStatus("authorizing", "authorizing…");
  const win = openPopup(authUrl.href);
  const opened = !!win;
  try {
    await fetch(ACK_PATH, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ request_id: data.request_id, opened }),
    });
  } catch {
    /* ignore */
  }
};

let lastOAuthCompletion = { status: null, at: 0 };

const handleOAuthCompletion = async (status) => {
  const now = Date.now();
  if (lastOAuthCompletion.status === status && now - lastOAuthCompletion.at < 1500) {
    return;
  }
  lastOAuthCompletion = { status, at: now };

  if (state.popup && !state.popup.closed) {
    try {
      state.popup.close();
    } catch {
      /* ignore */
    }
  }
  state.popup = null;

  if (status === "login_ok") {
    await checkAuthStatus();
    return;
  }
  if (status === "ok") {
    setStatus(state.isRunning ? "running" : "ready", state.isRunning ? "streaming" : "ready");
    if (state.activeSession && !state.isRunning) {
      await loadSession(state.activeSession.id);
    }
  } else if (status === "error") {
    setStatus("ready", "ready");
  }
};

if ("EventSource" in window) {
  const es = new EventSource(EVENTS_PATH);
  es.addEventListener("message", (event) => {
    try {
      const data = JSON.parse(event.data);
      if (data.type === "auth_required") {
        handleAuthRequired(data);
      } else if (data.type === "auth_completed") {
        handleOAuthCompletion(data.status);
      }
    } catch {
      /* ignore */
    }
  });
}

if ("BroadcastChannel" in window) {
  new BroadcastChannel(CHANNEL).addEventListener("message", (event) => {
    if (event.data && event.data.type === CHANNEL) {
      handleOAuthCompletion(event.data.status);
    }
  });
}
window.addEventListener("message", (event) => {
  if (event.origin === location.origin && event.data && event.data.type === CHANNEL) {
    handleOAuthCompletion(event.data.status);
  }
});

// ---------------------------------------------------------------------------
// User Authentication (`/auth/me`, `/auth/login`, `/auth/logout`)
// ---------------------------------------------------------------------------

const setAvatarImage = (imgEl, fallbackEl, user) => {
  const initials = initialsOf(user.name || user.email);
  fallbackEl.textContent = initials;
  if (user.picture) {
    imgEl.src = user.picture;
    imgEl.classList.remove("hidden");
    fallbackEl.classList.add("hidden");
    imgEl.onerror = () => {
      imgEl.classList.add("hidden");
      fallbackEl.classList.remove("hidden");
    };
  } else {
    imgEl.classList.add("hidden");
    fallbackEl.classList.remove("hidden");
  }
};

let authCheckPromise = null;

function syncSettingsAvailability() {
  const hasSession = Boolean(state.currentUser && state.activeSession);
  if (!els.settingsMenu) return;
  els.settingsMenu.classList.toggle("settings-popover--auth-only", !hasSession);
  els.settingsMenu.querySelectorAll("[data-session-only]").forEach((el) => {
    el.classList.toggle("hidden", !hasSession);
    el.querySelectorAll("input, select, button, textarea").forEach((ctrl) => {
      ctrl.disabled = !hasSession;
    });
  });
  // Model selectors stay visible (so users can see the active models) but
  // are disabled for non-admins, who would get a 403 from POST /auth/models.
  els.settingsMenu.querySelectorAll("[data-admin-only]").forEach((el) => {
    el.querySelectorAll("input, select").forEach((ctrl) => {
      ctrl.disabled = !hasSession || !state.canManageModels;
    });
  });
  const adminHint = $("setting-models-admin-hint");
  if (adminHint) adminHint.classList.toggle("hidden", state.canManageModels);
}

const checkAuthStatus = () => {
  if (authCheckPromise) return authCheckPromise;
  authCheckPromise = (async () => {
    try {
      const resp = await fetch("/auth/me");
      if (!resp.ok) {
        showError(`Could not verify authentication status: server returned HTTP ${resp.status}.`);
        return;
      }
      const data = await resp.json();
      if (!data.authenticated || !data.user) {
        state.currentUser = null;
        state.activeSession = null;
        state.canManageModels = true;
        els.authGate.classList.remove("hidden");
        els.workspace.classList.add("hidden");
        syncActiveStage();
        syncSettingsAvailability();
        els.sessionControls.classList.add("hidden");
        els.userMenuAnchor.classList.add("hidden");
        els.authGateHint.textContent = data.oauth_configured
          ? ""
          : "Configure CLIENT_ID and CLIENT_SECRET in .env to enable Google Sign-In.";
        els.authGateHint.classList.toggle("hidden", Boolean(data.oauth_configured));
        return;
      }

      state.currentUser = data.user;
      // `is_admin` gates POST /auth/models; absent (e.g. OAuth unconfigured) means allowed.
      state.canManageModels = data.is_admin !== false;
      els.authGate.classList.add("hidden");
      els.workspace.classList.remove("hidden");
      syncActiveStage();
      syncSettingsAvailability();
      els.sessionControls.classList.remove("hidden");
      els.userMenuAnchor.classList.remove("hidden");

      $("user-name").textContent = state.currentUser.name || state.currentUser.email;
      $("menu-user-name").textContent = state.currentUser.name || "Google User";
      $("menu-user-email").textContent = state.currentUser.email;
      $("menu-user-id").textContent = state.currentUser.id;
      els.composerUserHint.textContent = state.currentUser.email;

      setAvatarImage($("user-avatar-img"), $("user-avatar-fallback"), state.currentUser);
      setAvatarImage($("menu-avatar-img"), $("menu-avatar-fallback"), state.currentUser);

      if (data.models) {
        ingestServerPricing(data.models);
        // The server's global selection is the source of truth on load for
        // everyone. Admins only push changes from the settings UI; a stale
        // localStorage value must never silently flip the models for all users.
        if (data.models.model) prefs.model = data.models.model;
        if (data.models.image_model) prefs.imageModel = data.models.image_model;
        if (data.models.video_model) prefs.videoModel = data.models.video_model;
        applyPreferences({ rerender: false });
      }

      await loadUserSessions();
    } catch (err) {
      showError(`Could not verify authentication status: ${err.message || err}`);
    } finally {
      authCheckPromise = null;
    }
  })();
  return authCheckPromise;
};

els.googleLoginBtn.addEventListener("click", () => {
  const win = openPopup("/auth/login", "adspace_google_login");
  if (!win) {
    window.location.href = "/auth/login";
  }
});

els.logoutBtn.addEventListener("click", async () => {
  els.userMenu.classList.add("hidden");
  try {
    const resp = await fetch("/auth/logout", { method: "POST" });
    if (!resp.ok) throw new Error(`HTTP ${resp.status}`);
    state.activeSession = null;
    state.sessions = [];
    state.canvasSurfaces = [];
  } catch (err) {
    showError(`Could not sign out: ${err.message || err}`);
  } finally {
    await checkAuthStatus();
  }
});

// ---------------------------------------------------------------------------
// ADK Session Management (Scoped to `currentUser.id`)
// ---------------------------------------------------------------------------

const apiUrl = (path) => `/api${path}`;

const loadUserSessions = async () => {
  if (!state.currentUser) return;
  try {
    const resp = await fetch(
      apiUrl(`/apps/${encodeURIComponent(APP_NAME)}/users/${encodeURIComponent(state.currentUser.id)}/sessions`)
    );
    if (!resp.ok) throw new Error(`HTTP ${resp.status}`);
    const list = await resp.json();
    state.sessions = (Array.isArray(list) ? list : []).sort(
      (a, b) => (b.lastUpdateTime || 0) - (a.lastUpdateTime || 0)
    );

    if (state.sessions.length === 0) {
      await createNewSession();
    } else {
      const targetId =
        state.activeSession && state.sessions.some((s) => s.id === state.activeSession.id)
          ? state.activeSession.id
          : state.sessions[0].id;
      renderSessionOptions(targetId);
      await loadSession(targetId);
    }
  } catch (err) {
    showError(`Failed to load sessions: ${err.message || err}`);
  }
};

const renderSessionOptions = (selectedId) => {
  els.sessionSelect.replaceChildren();
  state.sessions.forEach((s, idx) => {
    const opt = document.createElement("option");
    opt.value = s.id;
    const shortId = s.id.slice(0, 8);
    opt.textContent = idx === 0 ? `Latest (${shortId})` : `Session ${shortId}`;
    if (s.id === selectedId) opt.selected = true;
    els.sessionSelect.appendChild(opt);
  });
};

let createSessionPromise = null;

const createNewSession = () => {
  if (!state.currentUser) return Promise.resolve();
  if (createSessionPromise) return createSessionPromise;
  createSessionPromise = (async () => {
    const url = apiUrl(
      `/apps/${encodeURIComponent(APP_NAME)}/users/${encodeURIComponent(state.currentUser.id)}/sessions`
    );
    try {
      let resp = await fetch(url, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: "{}",
      });
      if (!resp.ok && resp.status >= 500) {
        await new Promise((r) => setTimeout(r, 80));
        resp = await fetch(url, {
          method: "POST",
          headers: { "Content-Type": "application/json" },
          body: "{}",
        });
      }
      if (!resp.ok) throw new Error(`HTTP ${resp.status}`);
      const created = await resp.json();
      state.sessions = [created, ...state.sessions.filter((s) => s.id !== created.id)];
      renderSessionOptions(created.id);
      await loadSession(created.id);
    } catch (err) {
      showError(`Could not create session: ${err.message || err}`);
    } finally {
      createSessionPromise = null;
    }
  })();
  return createSessionPromise;
};

// Monotonic token so overlapping `loadSession` calls (quick <select> changes,
// refresh racing an OAuth completion) cannot apply a stale response.
let loadSeq = 0;

const loadSession = async (sessionId) => {
  if (!state.currentUser || !sessionId) return;
  const seq = ++loadSeq;
  const isSessionSwitch = !state.activeSession || state.activeSession.id !== sessionId;
  // Switching away from a streaming session: stop the run so its SSE loop
  // cannot keep appending the old session's events into the new one.
  if (isSessionSwitch && state.isRunning && state.abortController) state.abortController.abort();
  try {
    const resp = await fetch(
      apiUrl(
        `/apps/${encodeURIComponent(APP_NAME)}/users/${encodeURIComponent(
          state.currentUser.id
        )}/sessions/${encodeURIComponent(sessionId)}`
      )
    );
    if (!resp.ok) throw new Error(`HTTP ${resp.status}`);
    const data = await resp.json();
    if (seq !== loadSeq) return; // stale: a newer load has started
    state.activeSession = data;
    syncSettingsAvailability();
    $("menu-session-id").textContent = state.activeSession.id;
    if (isSessionSwitch) {
      state.activeSurfaceId = "all";
      closeModalOverlay();
    }
    rebuildFromSessionEvents(state.activeSession.events || [], {
      forceCanvas: isSessionSwitch,
      suppressOverlays: true,
    });
  } catch (err) {
    if (seq !== loadSeq) return;
    showError(`Could not load session: ${err.message || err}`);
  }
};

els.sessionSelect.addEventListener("change", () => {
  if (els.sessionSelect.value) loadSession(els.sessionSelect.value);
});

els.newSessionBtn.addEventListener("click", () => {
  createNewSession();
});

els.refreshSessionBtn.addEventListener("click", () => {
  if (state.activeSession) loadSession(state.activeSession.id);
});

els.deleteSessionBtn.addEventListener("click", async () => {
  els.userMenu.classList.add("hidden");
  if (!state.currentUser || !state.activeSession) return;
  try {
    const resp = await fetch(
      apiUrl(
        `/apps/${encodeURIComponent(APP_NAME)}/users/${encodeURIComponent(
          state.currentUser.id
        )}/sessions/${encodeURIComponent(state.activeSession.id)}`
      ),
      { method: "DELETE" }
    );
    if (!resp.ok) throw new Error(`HTTP ${resp.status}`);
  } catch (err) {
    showError(`Could not delete session: ${err.message || err}`);
    return;
  }
  state.activeSession = null;
  syncSettingsAvailability();
  await loadUserSessions();
});

// ---------------------------------------------------------------------------
// `adk-ui` (`UiResponse`) Interactive Component Renderer (Right-Hand Canvas)
// ---------------------------------------------------------------------------

let activeModalCleanup = null;
// Unique id prefix for each rendered `tabs` component (tab ↔ panel wiring).
let uiTabsSeq = 0;

const closeModalOverlay = () => {
  const root = $("ui-modal-root");
  if (activeModalCleanup) {
    activeModalCleanup();
    activeModalCleanup = null;
  }
  if (root) {
    root.classList.add("hidden");
    root.replaceChildren();
  }
};

const showModalOverlay = (spec, { isConfirm = false } = {}) => {
  const root = $("ui-modal-root");
  if (!root || !spec) return;
  closeModalOverlay();

  const closable = spec.closable !== false;
  const rawSize = String(spec.size || (isConfirm ? "small" : "medium")).toLowerCase();
  const size = ["small", "medium", "large", "full"].includes(rawSize) ? rawSize : "medium";

  const backdrop = document.createElement("div");
  backdrop.className = "ui-modal-backdrop";
  if (closable) {
    backdrop.addEventListener("click", closeModalOverlay);
  }

  const dialog = document.createElement("div");
  dialog.className = `ui-modal-dialog ui-modal--${size}`;
  dialog.setAttribute("data-ui-scope", "true");
  dialog.setAttribute("data-ui-modal", "true");

  const hdr = document.createElement("header");
  hdr.className = "ui-modal-header";
  const titleWrap = document.createElement("div");
  titleWrap.className = "ui-modal-title-wrap";

  const hasDangerAction =
    isConfirm &&
    Array.isArray(spec.footer) &&
    spec.footer.some((b) => String(b?.variant || "").toLowerCase() === "danger");

  if (hasDangerAction) {
    const warnChip = document.createElement("span");
    style(warnChip, {
      color: "var(--accent-error)",
      width: "34px",
      height: "34px",
      borderRadius: "50%",
      background: "var(--dm-surface-gray-subtle)",
      display: "inline-flex",
      alignItems: "center",
      justifyContent: "center",
      flexShrink: "0",
    });
    const warnIcon = document.createElement("span");
    warnIcon.className = "google-symbols filled";
    warnIcon.setAttribute("aria-hidden", "true");
    warnIcon.textContent = "warning";
    style(warnIcon, { fontSize: "19px" });
    warnChip.appendChild(warnIcon);
    titleWrap.appendChild(warnChip);
  }

  const titleEl = document.createElement("h3");
  titleEl.className = "ui-modal-title";
  titleEl.textContent = spec.title || (isConfirm ? "Confirm Action" : "Details");
  titleWrap.appendChild(titleEl);
  hdr.appendChild(titleWrap);

  if (closable) {
    const closeBtn = document.createElement("button");
    closeBtn.type = "button";
    closeBtn.className = "ui-modal-close";
    closeBtn.setAttribute("aria-label", "Close dialog");
    const closeSym = document.createElement("span");
    closeSym.className = "google-symbols";
    closeSym.setAttribute("aria-hidden", "true");
    closeSym.textContent = "close";
    style(closeSym, { fontSize: "18px" });
    closeBtn.appendChild(closeSym);
    closeBtn.addEventListener("click", closeModalOverlay);
    hdr.appendChild(closeBtn);
  }

  const body = document.createElement("div");
  body.className = "ui-modal-body";
  const contentArr = Array.isArray(spec.content) ? spec.content : [];
  for (const child of contentArr) {
    body.appendChild(renderInnerComponent(child, dialog, { inModal: true }));
  }

  dialog.append(hdr, body);

  if (Array.isArray(spec.footer) && spec.footer.length) {
    const ftr = document.createElement("footer");
    ftr.className = "ui-modal-footer";
    for (const child of spec.footer) {
      ftr.appendChild(renderInnerComponent(child, dialog, { inModal: true }));
    }
    dialog.appendChild(ftr);
  }

  const onKeyDown = (ev) => {
    if (ev.key === "Escape" && closable) {
      closeModalOverlay();
      return;
    }
    // Keep Tab / Shift+Tab cycling inside the dialog.
    if (ev.key === "Tab") {
      const focusables = focusableIn(dialog);
      if (!focusables.length) {
        ev.preventDefault();
        dialog.focus();
        return;
      }
      const first = focusables[0];
      const last = focusables[focusables.length - 1];
      const active = document.activeElement;
      if (ev.shiftKey && (active === first || !dialog.contains(active))) {
        ev.preventDefault();
        last.focus();
      } else if (!ev.shiftKey && (active === last || !dialog.contains(active))) {
        ev.preventDefault();
        first.focus();
      }
    }
  };
  window.addEventListener("keydown", onKeyDown);

  // Focus management: remember the opener, make the workspace inert, move
  // focus into the dialog, and restore it all in `closeModalOverlay`.
  const previouslyFocused = document.activeElement;
  titleEl.id = "ui-modal-title";
  root.setAttribute("aria-labelledby", "ui-modal-title");
  dialog.tabIndex = -1;
  if (els.workspace) els.workspace.inert = true;
  activeModalCleanup = () => {
    window.removeEventListener("keydown", onKeyDown);
    if (els.workspace) els.workspace.inert = false;
    root.removeAttribute("aria-labelledby");
    if (previouslyFocused && typeof previouslyFocused.focus === "function") {
      try {
        previouslyFocused.focus();
      } catch {
        /* ignore */
      }
    }
  };

  root.replaceChildren(backdrop, dialog);
  root.classList.remove("hidden");
  // Prefer the first form control; fall back to the close button, then the
  // dialog itself, so keyboard users are never left on the inert page.
  const initialFocus =
    focusableIn(body)[0] || dialog.querySelector(".ui-modal-close") || dialog;
  initialFocus.focus();
};

const alertVariantLabel = (variant) => {
  const v = String(variant || "info").toLowerCase();
  if (v === "info") return "Info";
  if (v === "warn" || v === "warning") return "Warning";
  if (v === "error" || v === "danger" || v === "outage") return "Error";
  if (v === "success" || v === "ok") return "Success";
  return v.charAt(0).toUpperCase() + v.slice(1);
};

const buildAlertParts = (alertComp) => {
  const variantLabel = alertVariantLabel(alertComp.variant);
  const titleText = String(alertComp.title || alertComp.message || "Alert");

  const content = document.createElement("div");
  content.className = "dm-alert-content";
  const titleEl = document.createElement("h3");
  titleEl.className = "dm-alert-title";
  titleEl.textContent = titleText;
  content.appendChild(titleEl);

  if (alertComp.description && String(alertComp.description).trim()) {
    const descEl = document.createElement("p");
    descEl.className = "dm-alert-desc";
    descEl.textContent = String(alertComp.description).trim();
    content.appendChild(descEl);
  }

  const statusBtn = document.createElement("span");
  statusBtn.className = "dm-alert-status-btn";
  statusBtn.textContent = variantLabel;

  return { content, statusBtn, variantLabel, titleText };
};

const createAlertCard = (alertComp) => {
  const card = document.createElement("section");
  card.className = "adk-ui-surface adk-ui-card dm-alert-card";
  card.setAttribute("data-ui-scope", "true");
  const { content, statusBtn, titleText } = buildAlertParts(alertComp);
  card.append(content, statusBtn);
  return { card, title: titleText };
};

const createStandaloneCard = ({ title, description, headerRight, flushBody = false }) => {
  const card = document.createElement("section");
  card.className = "adk-ui-surface adk-ui-card";
  card.setAttribute("data-ui-scope", "true");
  style(card, {
    width: "100%",
    border: "1px solid var(--dm-outline)",
    borderRadius: "24px",
    background: "var(--surface-card)",
    boxShadow:
      "0 12px 32px -12px rgba(18, 19, 23, 0.08), 0 2px 8px -2px rgba(18, 19, 23, 0.04)",
    overflow: "hidden",
    display: "flex",
    flexDirection: "column",
  });

  if (title || description || headerRight) {
    const hdr = document.createElement("header");
    style(hdr, {
      display: "flex",
      alignItems: "flex-start",
      justifyContent: "space-between",
      gap: "16px",
      padding: flushBody ? "20px 24px 16px" : "22px 24px 6px",
      borderBottom: flushBody ? "1px solid var(--dm-outline)" : "0",
      background: "transparent",
    });

    const left = document.createElement("div");
    style(left, {
      display: "flex",
      flexDirection: "column",
      alignItems: "flex-start",
      gap: "4px",
      minWidth: "0",
    });

    if (title) {
      const strong = document.createElement("strong");
      strong.textContent = title;
      style(strong, {
        fontSize: "20px",
        fontWeight: "450",
        lineHeight: "1.22",
        letterSpacing: "-0.008em",
        color: "var(--dm-text-primary)",
        textWrap: "pretty",
      });
      left.appendChild(strong);
    }

    if (description) {
      const sub = document.createElement("span");
      sub.textContent = description;
      style(sub, {
        fontSize: "14.5px",
        fontWeight: "400",
        lineHeight: "1.45",
        color: "var(--dm-text-secondary)",
      });
      left.appendChild(sub);
    }

    hdr.appendChild(left);
    if (headerRight) hdr.appendChild(headerRight);
    card.appendChild(hdr);
  }

  const body = document.createElement("div");
  style(body, {
    padding: flushBody ? "0" : "10px 24px 24px",
    display: "flex",
    flexDirection: "column",
    gap: "16px",
  });
  card.appendChild(body);

  return { card, body };
};

// `ctx` is the render context: `inGrid` (KPI tile inside a grid) and
// `inModal` (anywhere inside the modal overlay). It is threaded explicitly
// because children are rendered while their parents are still detached, so
// DOM-based `closest()` sniffing cannot see the modal.
const renderInnerComponent = (comp, surfaceRoot, ctx = {}) => {
  if (!comp || typeof comp !== "object") return document.createTextNode("");
  const { inGrid = false, inModal = false } = ctx || {};
  const type = String(comp.type || "").toLowerCase();

  switch (type) {
    case "text": {
      const v = String(comp.variant || "body").toLowerCase();
      const raw = String(comp.content ?? "");
      const stepMatch = v === "body" && raw.match(/^(✅|⏳|⬜)\s+([\s\S]+)$/);
      if (stepMatch) {
        const row = document.createElement("div");
        const isDone = stepMatch[1] === "✅";
        const isCurrent = stepMatch[1] === "⏳";
        style(row, {
          display: "flex",
          alignItems: "center",
          gap: "10px",
          padding: "10px 14px",
          borderRadius: "14px",
          background: "var(--dm-surface-gray)",
          border: `1px solid ${isCurrent ? "var(--dm-text-primary)" : "var(--dm-outline)"}`,
          fontSize: "14px",
          color: isDone ? "var(--dm-text-secondary)" : "var(--dm-text-primary)",
          fontWeight: isCurrent ? "500" : "400",
        });
        const mark = document.createElement("span");
        mark.textContent = stepMatch[1];
        mark.setAttribute("aria-hidden", "true");
        const txt = document.createElement("span");
        appendInlineMarkdown(txt, stepMatch[2]);
        row.append(mark, txt);
        return row;
      }
      const el = document.createElement(v === "code" ? "code" : "div");
      if (v === "code") {
        el.textContent = raw;
      } else if (v === "body" && raw.includes("\n")) {
        el.className = "message-text";
        parseMarkdownBlocksInto(el, raw);
        style(el, { padding: "0" });
      } else {
        appendInlineMarkdown(el, raw);
      }
      const map = {
        h1: {
          fontSize: "22px",
          fontWeight: "450",
          letterSpacing: "-0.012em",
          lineHeight: "1.2",
          color: "var(--dm-text-primary)",
        },
        h2: {
          fontSize: inGrid ? "28px" : "19px",
          fontWeight: "450",
          letterSpacing: inGrid ? "-0.015em" : "-0.01em",
          lineHeight: inGrid ? "1.12" : "1.22",
          color: "var(--dm-text-primary)",
        },
        h3: {
          fontSize: inModal ? "17px" : "16.5px",
          fontWeight: "450",
          letterSpacing: "-0.006em",
          lineHeight: "1.32",
          color: "var(--dm-text-primary)",
        },
        h4: {
          fontSize: "14.5px",
          fontWeight: "500",
          lineHeight: "1.4",
          color: "var(--dm-text-primary)",
        },
        caption: {
          fontSize: inModal ? "13.5px" : "13px",
          color: "var(--dm-text-secondary)",
          lineHeight: "1.45",
        },
        code: {
          fontFamily: "var(--mono)",
          fontSize: inModal ? "13px" : "12.5px",
          background: "var(--dm-surface-gray-subtle)",
          border: "1px solid var(--dm-outline)",
          padding: "1px 6px",
          borderRadius: "6px",
        },
        body: {
          fontSize: inModal ? "16px" : "14.5px",
          lineHeight: "1.55",
          color: inModal ? "var(--dm-text-secondary)" : "var(--dm-text-primary)",
        },
      };
      style(el, map[v] || map.body);
      return el;
    }

    case "badge": {
      const span = document.createElement("span");
      span.textContent = comp.label ?? "";
      const vc = variantColors(comp.variant);
      style(span, {
        display: "inline-flex",
        alignItems: "center",
        padding: "4px 10px",
        borderRadius: "999px",
        fontSize: "12px",
        fontWeight: "500",
        background: vc.bg,
        color: vc.fg,
        border: "1px solid transparent",
        alignSelf: "flex-start",
      });
      return span;
    }

    case "divider": {
      const hr = document.createElement("hr");
      style(hr, {
        border: "0",
        borderTop: "1px solid var(--dm-outline)",
        margin: "6px 0",
        width: "100%",
      });
      return hr;
    }

    case "stack": {
      const div = document.createElement("div");
      const horiz = String(comp.direction || "vertical").toLowerCase() === "horizontal";
      const gapPx = Math.max(1, Number(comp.gap ?? 3)) * 4;
      style(div, {
        display: "flex",
        flexDirection: horiz ? "row" : "column",
        flexWrap: horiz ? "wrap" : "nowrap",
        alignItems: horiz ? "center" : "stretch",
        gap: `${gapPx}px`,
      });
      for (const child of comp.children || []) {
        div.appendChild(renderInnerComponent(child, surfaceRoot, ctx));
      }
      return div;
    }

    case "grid": {
      const div = document.createElement("div");
      const cols = Math.max(1, Math.min(6, Number(comp.columns) || 2));
      style(div, {
        display: "grid",
        gridTemplateColumns: `repeat(auto-fit, minmax(${cols >= 3 ? "165px" : "220px"}, 1fr))`,
        gap: "14px",
        alignItems: "stretch",
      });
      for (const child of comp.children || []) {
        div.appendChild(renderInnerComponent(child, surfaceRoot, { inGrid: true, inModal }));
      }
      return div;
    }

    case "container": {
      const div = document.createElement("div");
      style(div, { display: "flex", flexDirection: "column", gap: "12px" });
      for (const child of comp.children || []) {
        div.appendChild(renderInnerComponent(child, surfaceRoot, ctx));
      }
      return div;
    }

    case "card":
    case "modal": {
      const tile = document.createElement("div");
      tile.setAttribute("data-ui-scope", "true");
      style(tile, {
        display: "flex",
        flexDirection: "column",
        justifyContent: "space-between",
        gap: "8px",
        padding: inGrid ? "18px 20px" : "20px 22px",
        borderRadius: "20px",
        background: "var(--dm-surface-gray)",
        // KPI tiles inside a grid are borderless; standalone cards keep the outline.
        border: inGrid ? "none" : "1px solid var(--dm-outline)",
      });

      if (comp.title) {
        const t = document.createElement("div");
        t.textContent = comp.title;
        style(
          t,
          inGrid
            ? {
                fontSize: "13.5px",
                fontWeight: "450",
                letterSpacing: "0.005em",
                color: "var(--dm-text-secondary)",
              }
            : {
                fontSize: "16.5px",
                fontWeight: "450",
                letterSpacing: "-0.006em",
                color: "var(--dm-text-primary)",
              }
        );
        tile.appendChild(t);
      }
      if (comp.description) {
        const d = document.createElement("div");
        d.textContent = comp.description;
        style(d, { fontSize: "13.5px", color: "var(--dm-text-secondary)", lineHeight: "1.45" });
        tile.appendChild(d);
      }
      if (Array.isArray(comp.content) && comp.content.length) {
        const body = document.createElement("div");
        style(body, { display: "flex", flexDirection: "column", gap: "8px" });
        for (const child of comp.content) {
          body.appendChild(renderInnerComponent(child, tile, ctx));
        }
        tile.appendChild(body);
      }
      if (Array.isArray(comp.footer) && comp.footer.length) {
        const ftr = document.createElement("div");
        style(ftr, {
          display: "flex",
          flexWrap: "wrap",
          gap: "8px",
          justifyContent: "flex-end",
          paddingTop: "10px",
        });
        for (const child of comp.footer) {
          ftr.appendChild(renderInnerComponent(child, tile, { inModal }));
        }
        tile.appendChild(ftr);
      }
      return tile;
    }

    case "tabs": {
      const wrap = document.createElement("div");
      style(wrap, { display: "flex", flexDirection: "column", gap: "14px" });
      const bar = document.createElement("div");
      bar.setAttribute("role", "tablist");
      if (comp.label) bar.setAttribute("aria-label", String(comp.label));
      style(bar, {
        display: "flex",
        gap: "4px",
        borderBottom: "1px solid var(--dm-outline)",
        paddingBottom: "0",
      });
      const panel = document.createElement("div");
      panel.setAttribute("role", "tabpanel");
      panel.tabIndex = 0;
      const tabs = Array.isArray(comp.tabs) ? comp.tabs : [];
      const tabsId = `ui-tabs-${++uiTabsSeq}`;
      let activeIdx = 0;

      const selectTab = (idx, { focus = false } = {}) => {
        activeIdx = idx;
        Array.from(bar.children).forEach((btn, i) => {
          const selected = i === activeIdx;
          btn.setAttribute("aria-selected", selected ? "true" : "false");
          btn.tabIndex = selected ? 0 : -1;
          style(btn, {
            background: "transparent",
            color: selected ? "var(--dm-text-primary)" : "var(--dm-text-secondary)",
            borderBottom: selected ? "2px solid var(--dm-text-primary)" : "2px solid transparent",
          });
          if (selected && focus) btn.focus();
        });
        panel.setAttribute("aria-labelledby", `${tabsId}-tab-${idx}`);
        panel.replaceChildren();
        for (const child of tabs[idx]?.content || []) {
          panel.appendChild(renderInnerComponent(child, surfaceRoot, { inModal }));
        }
      };

      tabs.forEach((tab, idx) => {
        const b = document.createElement("button");
        b.type = "button";
        b.id = `${tabsId}-tab-${idx}`;
        b.setAttribute("role", "tab");
        b.setAttribute("aria-controls", `${tabsId}-panel`);
        b.textContent = tab.label || `Tab ${idx + 1}`;
        style(b, {
          border: "0",
          borderBottom: "2px solid transparent",
          borderRadius: "0",
          padding: "10px 16px",
          marginBottom: "-1px",
          fontSize: "14px",
          fontWeight: "500",
          cursor: "pointer",
        });
        b.addEventListener("click", () => selectTab(idx));
        // Roving tabindex: arrows move between tabs, Home/End jump to the ends.
        b.addEventListener("keydown", (ev) => {
          const n = tabs.length;
          let next = null;
          if (ev.key === "ArrowRight" || ev.key === "ArrowDown") next = (idx + 1) % n;
          else if (ev.key === "ArrowLeft" || ev.key === "ArrowUp") next = (idx - 1 + n) % n;
          else if (ev.key === "Home") next = 0;
          else if (ev.key === "End") next = n - 1;
          if (next === null) return;
          ev.preventDefault();
          selectTab(next, { focus: true });
        });
        bar.appendChild(b);
      });
      panel.id = `${tabsId}-panel`;
      wrap.append(bar, panel);
      if (tabs.length) selectTab(0);
      return wrap;
    }

    case "chart":
      return renderChartInner(comp);

    case "table": {
      const { element, toolbar } = renderTableParts(comp);
      const box = document.createElement("div");
      style(box, {
        border: "1px solid var(--dm-outline)",
        borderRadius: "18px",
        overflow: "hidden",
      });
      const bar = document.createElement("div");
      style(bar, {
        display: "flex",
        justifyContent: "flex-end",
        padding: "10px 16px",
        background: "transparent",
        borderBottom: "1px solid var(--dm-outline)",
      });
      bar.appendChild(toolbar);
      box.append(bar, element);
      return box;
    }

    case "key_value": {
      const dl = document.createElement("div");
      style(dl, {
        display: "grid",
        gridTemplateColumns: "minmax(140px, auto) 1fr",
        gap: "10px 20px",
        padding: "16px 18px",
        borderRadius: "18px",
        background: "var(--dm-surface-gray)",
        border: "1px solid var(--dm-outline)",
        fontSize: "13.5px",
      });
      for (const pair of comp.pairs || []) {
        const k = document.createElement("div");
        k.textContent = pair.key;
        style(k, { color: "var(--dm-text-secondary)", fontWeight: "450" });
        const v = document.createElement("div");
        v.textContent = pair.value;
        style(v, { color: "var(--dm-text-primary)", fontWeight: "500" });
        dl.append(k, v);
      }
      return dl;
    }

    case "list": {
      const list = document.createElement(comp.ordered ? "ol" : "ul");
      style(list, {
        margin: "0",
        paddingLeft: "22px",
        display: "flex",
        flexDirection: "column",
        gap: "6px",
        fontSize: "14.5px",
        lineHeight: "1.55",
        color: "var(--dm-text-primary)",
      });
      for (const item of comp.items || []) {
        const li = document.createElement("li");
        li.textContent = item;
        list.appendChild(li);
      }
      return list;
    }

    case "code_block": {
      const pre = document.createElement("pre");
      pre.textContent = comp.code ?? "";
      style(pre, {
        margin: "0",
        padding: "14px 16px",
        borderRadius: "14px",
        background: "var(--dm-surface-gray)",
        border: "1px solid var(--dm-outline)",
        color: "var(--dm-text-primary)",
        fontSize: "12.5px",
        overflowX: "auto",
        fontFamily: "var(--mono)",
      });
      return pre;
    }

    case "alert":
    case "toast": {
      const box = document.createElement("div");
      box.className = "dm-alert-inline";
      const { content, statusBtn } = buildAlertParts(comp);
      box.append(content, statusBtn);
      return box;
    }

    case "progress": {
      const wrap = document.createElement("div");
      style(wrap, { display: "flex", flexDirection: "column", gap: "8px" });
      const pct = Math.max(0, Math.min(100, Number(comp.value) || 0));
      const top = document.createElement("div");
      style(top, { display: "flex", justifyContent: "space-between", fontSize: "13.5px" });
      const lbl = document.createElement("span");
      lbl.textContent = comp.label || "Progress";
      style(lbl, { color: "var(--dm-text-primary)", fontWeight: "450" });
      const val = document.createElement("span");
      val.textContent = `${pct}%`;
      style(val, { color: "var(--dm-text-secondary)", fontFamily: "var(--mono)" });
      top.append(lbl, val);

      const track = document.createElement("div");
      style(track, {
        height: "8px",
        borderRadius: "999px",
        background: "var(--dm-surface-gray)",
        overflow: "hidden",
      });
      const bar = document.createElement("div");
      style(bar, {
        width: `${pct}%`,
        height: "100%",
        borderRadius: "999px",
        background: "var(--dm-text-primary)",
      });
      track.appendChild(bar);
      wrap.append(top, track);
      return wrap;
    }

    case "text_input":
    case "number_input":
    case "date_input":
    case "textarea":
    case "select":
    case "multi_select":
    case "slider":
    case "switch": {
      const field = document.createElement("label");
      style(field, { display: "flex", flexDirection: "column", gap: "6px", fontSize: "13.5px" });
      const lbl = document.createElement("span");
      lbl.textContent = `${comp.label || comp.name}${comp.required ? " *" : ""}`;
      style(lbl, { fontWeight: "450", color: "var(--dm-text-secondary)" });

      let sliderValEl = null;
      if (type === "slider") {
        const lblRow = document.createElement("div");
        style(lblRow, { display: "flex", justifyContent: "space-between", alignItems: "center" });
        sliderValEl = document.createElement("code");
        sliderValEl.textContent = String(comp.default_value ?? comp.min ?? 0);
        style(sliderValEl, {
          fontFamily: "var(--mono)",
          fontSize: "12.5px",
          color: "var(--dm-text-primary)",
        });
        lblRow.append(lbl, sliderValEl);
        field.appendChild(lblRow);
      } else {
        field.appendChild(lbl);
      }

      let control;
      if (type === "textarea") {
        control = document.createElement("textarea");
        control.rows = Number(comp.rows) || 3;
        control.placeholder = comp.placeholder || "";
        control.value = comp.default_value || "";
      } else if (type === "select" || type === "multi_select") {
        control = document.createElement("select");
        if (type === "multi_select") control.multiple = true;
        const hasDefault =
          comp.default_value !== undefined &&
          comp.default_value !== null &&
          String(comp.default_value) !== "";
        if (type === "select" && (comp.placeholder || (comp.required && !hasDefault))) {
          const ph = document.createElement("option");
          ph.value = "";
          ph.textContent = comp.placeholder || "Select an option…";
          ph.disabled = true;
          ph.selected = !hasDefault;
          control.appendChild(ph);
        }
        for (const opt of comp.options || []) {
          const o = document.createElement("option");
          if (opt && typeof opt === "object") {
            o.value = String(opt.value ?? opt.label ?? "");
            o.textContent = String(opt.label ?? opt.value ?? "");
          } else {
            // Plain-string options: the string is both value and label.
            o.value = String(opt ?? "");
            o.textContent = o.value;
          }
          control.appendChild(o);
        }
        if (hasDefault) {
          if (control.multiple) {
            const defaults = (
              Array.isArray(comp.default_value) ? comp.default_value : [comp.default_value]
            ).map((d) => String(d));
            for (const o of control.options) o.selected = defaults.includes(o.value);
          } else {
            control.value = String(comp.default_value);
          }
        }
      } else if (type === "switch") {
        const row = document.createElement("div");
        style(row, { display: "flex", alignItems: "center", gap: "10px", padding: "4px 0" });
        control = document.createElement("input");
        control.type = "checkbox";
        control.checked = !!comp.default_checked;
        row.append(control, lbl);
        field.replaceChildren(row);
      } else {
        control = document.createElement("input");
        control.type =
          type === "number_input"
            ? "number"
            : type === "date_input"
              ? "date"
              : type === "slider"
                ? "range"
                : comp.input_type || "text";
        if (comp.placeholder) control.placeholder = comp.placeholder;
        if (comp.min !== undefined && comp.min !== null) control.min = String(comp.min);
        if (comp.max !== undefined && comp.max !== null) control.max = String(comp.max);
        if (comp.step !== undefined && comp.step !== null) control.step = String(comp.step);
        if (comp.pattern) control.pattern = String(comp.pattern);
        if (comp.default_value !== undefined && comp.default_value !== null) {
          control.value = String(comp.default_value);
        }
        if (type === "slider" && sliderValEl) {
          sliderValEl.textContent = control.value;
          control.addEventListener("input", () => {
            sliderValEl.textContent = control.value;
          });
        }
        if (type !== "slider") {
          control.addEventListener("keydown", (ev) => {
            if (ev.key === "Enter") {
              ev.preventDefault();
              const scope = control.closest('[data-ui-scope="true"]') || surfaceRoot;
              const submitBtn = scope?.querySelector('button[data-ui-primary="true"]');
              if (submitBtn && !submitBtn.disabled) submitBtn.click();
            }
          });
        }
      }

      if (comp.required) {
        control.required = true;
        control.setAttribute("aria-required", "true");
      }
      control.setAttribute("data-ui-label", comp.label || comp.name || "Field");

      if (type !== "switch") {
        style(control, {
          padding:
            type === "select"
              ? "10px 34px 10px 14px"
              : type === "slider"
                ? "4px 0"
                : "10px 14px",
          borderRadius: "12px",
          border: type === "slider" ? "0" : "1px solid var(--dm-outline)",
          backgroundColor: type === "slider" ? "transparent" : "var(--dm-surface-gray)",
          color: "var(--dm-text-primary)",
          fontSize: "14px",
        });
        field.appendChild(control);
      }

      const errEl = document.createElement("span");
      errEl.className = "ui-field-error hidden";
      style(errEl, {
        fontSize: "12px",
        fontWeight: "450",
        color: "var(--accent-error)",
      });
      field.appendChild(errEl);

      const clearFieldError = () => {
        if (type !== "switch" && type !== "slider") {
          control.style.borderColor = "var(--dm-outline)";
        }
        control.removeAttribute("aria-invalid");
        errEl.textContent = "";
        errEl.classList.add("hidden");
      };
      control.addEventListener("input", clearFieldError);
      control.addEventListener("change", clearFieldError);

      control.setAttribute("data-ui-field", comp.name || comp.label || "field");
      return field;
    }

    case "button": {
      const btn = document.createElement("button");
      btn.type = "button";
      btn.textContent = comp.label || "Submit";
      btn.disabled = !!comp.disabled;
      const v = String(comp.variant || "primary").toLowerCase();
      if (v === "primary" || v === "danger") {
        btn.setAttribute("data-ui-primary", "true");
      }
      const btnStyles =
        v === "primary"
          ? {
              background: "var(--primary-btn-bg)",
              color: "var(--primary-btn-fg)",
              border: "1px solid transparent",
            }
          : v === "danger"
            ? {
                background: "var(--accent-error)",
                color: "#fff",
                border: "1px solid var(--accent-error)",
              }
            : v === "outline"
              ? {
                  background: "transparent",
                  color: "var(--dm-text-primary)",
                  border: "1px solid var(--dm-outline)",
                  boxShadow: "0 0 0 1px var(--dm-outline)",
                }
              : v === "ghost"
                ? {
                    background: "transparent",
                    color: "var(--dm-text-secondary)",
                    border: "1px solid transparent",
                  }
                : {
                    background: inModal ? "var(--dm-surface-gray-subtle)" : "var(--dm-surface-gray)",
                    color: "var(--dm-text-primary)",
                    border: "1px solid var(--dm-outline)",
                  };
      style(btn, {
        padding: "10px 22px",
        borderRadius: "999px",
        fontSize: inModal ? "14.5px" : "14px",
        fontWeight: "500",
        letterSpacing: "-0.004em",
        cursor: btn.disabled ? "default" : "pointer",
        opacity: btn.disabled ? "0.45" : "1",
        alignSelf: "flex-start",
        transition: "opacity 0.15s ease, transform 0.12s ease",
        ...btnStyles,
      });

      btn.addEventListener("click", () => {
        // Programmatic sends must never double as the composer's stop toggle:
        // while a run is streaming, refuse instead of aborting it and dropping
        // the form payload.
        if (state.isRunning) {
          showToast({ variant: "warning", message: "Wait for the agent to finish." });
          return;
        }
        const scope = btn.closest('[data-ui-scope="true"]') || surfaceRoot;
        const inputs = scope ? Array.from(scope.querySelectorAll("[data-ui-field]")) : [];
        const actionId = comp.action_id || comp.label || "click";
        const isCancelAction =
          v === "secondary" ||
          v === "ghost" ||
          /^(cancel|dismiss|close|skip|reset)\b/i.test(actionId) ||
          /^(cancel|dismiss|close|skip|reset)\b/i.test(comp.label || "");

        if (inputs.length > 0 && !isCancelAction) {
          let firstInvalid = null;
          for (const inp of inputs) {
            const fieldLabel = inp.getAttribute("data-ui-label") || inp.getAttribute("data-ui-field") || "Field";
            const errEl = inp.closest("label")?.querySelector(".ui-field-error");
            let errMsg = "";

            if (inp.required) {
              const isEmpty =
                inp.type === "checkbox"
                  ? !inp.checked
                  : inp.multiple
                    ? inp.selectedOptions.length === 0
                    : String(inp.value ?? "").trim() === "";
              if (isEmpty) {
                errMsg = `${fieldLabel} is required.`;
              }
            }
            if (!errMsg && typeof inp.checkValidity === "function" && !inp.checkValidity()) {
              errMsg = inp.validationMessage || `${fieldLabel} is invalid.`;
            }

            if (errMsg) {
              if (inp.type !== "checkbox" && inp.type !== "range") {
                inp.style.borderColor = "var(--accent-error)";
              }
              inp.setAttribute("aria-invalid", "true");
              if (errEl) {
                errEl.textContent = errMsg;
                errEl.classList.remove("hidden");
              }
              if (!firstInvalid) firstInvalid = inp;
            }
          }

          if (firstInvalid) {
            firstInvalid.focus();
            if (typeof firstInvalid.reportValidity === "function") {
              firstInvalid.reportValidity();
            }
            return;
          }
        }

        let msg;
        if (inputs.length > 0 && !isCancelAction) {
          const lines = inputs.map((inp) => {
            const name = inp.getAttribute("data-ui-field");
            const val =
              inp.type === "checkbox"
                ? String(inp.checked)
                : inp.multiple
                  ? Array.from(inp.selectedOptions)
                      .map((o) => o.value)
                      .join(", ")
                  : inp.value;
            return `  ${name}: ${val}`;
          });
          msg = `[UI Event: Form submitted]\nAction: ${actionId}\nData:\n${lines.join("\n")}`;
        } else {
          msg = `[UI Event: Button clicked]\nAction: ${actionId}`;
        }
        const orig = btn.textContent;
        btn.textContent = `${orig} ✓`;
        setTimeout(() => {
          btn.textContent = orig;
        }, 1800);
        if (btn.closest('[data-ui-modal="true"]')) {
          closeModalOverlay();
        }
        sendMessage(msg);
      });
      return btn;
    }

    case "icon": {
      const span = document.createElement("span");
      span.className = "google-symbols";
      span.setAttribute("aria-hidden", "true");
      const rawName = String(comp.name || "info").toLowerCase().replace(/-/g, "_");
      const iconMap = {
        check: "check_circle",
        alert_triangle: "warning",
        alert_circle: "error",
        x: "close",
      };
      span.textContent = iconMap[rawName] || rawName;
      style(span, {
        fontSize: `${Math.max(14, Number(comp.size) || 18)}px`,
        color: "var(--accent-primary)",
      });
      return span;
    }

    case "image": {
      const img = document.createElement("img");
      img.src = safeImageSrc(comp.src);
      img.alt = comp.alt || "";
      style(img, {
        maxWidth: "100%",
        height: "auto",
        borderRadius: "10px",
        border: "1px solid var(--border-default)",
      });
      return img;
    }

    case "spinner": {
      const wrap = document.createElement("div");
      style(wrap, { display: "inline-flex", alignItems: "center", gap: "8px" });
      const ring = document.createElement("span");
      const sz = comp.size === "small" ? "14px" : comp.size === "large" ? "24px" : "18px";
      style(ring, {
        width: sz,
        height: sz,
        borderRadius: "50%",
        border: "2px solid var(--border-strong)",
        borderTopColor: "var(--accent-primary)",
        display: "inline-block",
        animation: "ui-spin 0.8s linear infinite",
      });
      wrap.appendChild(ring);
      if (comp.label) {
        const lbl = document.createElement("span");
        lbl.textContent = comp.label;
        style(lbl, { fontSize: "12px", color: "var(--text-secondary)" });
        wrap.appendChild(lbl);
      }
      return wrap;
    }

    case "skeleton": {
      const sk = document.createElement("div");
      style(sk, {
        width: comp.width || "100%",
        height: comp.height || "16px",
        borderRadius: comp.circle ? "50%" : "8px",
        background: "var(--bg-primary)",
        border: "1px solid var(--border-default)",
        animation: "ui-shimmer 1.4s ease-in-out infinite",
      });
      return sk;
    }

    default:
      return document.createTextNode("");
  }
};

// Lower-cased component type, matching how `renderInnerComponent` dispatches.
const compType = (c) => String(c?.type || "").toLowerCase();

const isConfirmCard = (comp, toolName) => {
  if (toolName === "render_confirm") return true;
  if (String(comp?.type || "").toLowerCase() !== "card") return false;
  const footer = Array.isArray(comp.footer) ? comp.footer : [];
  const content = Array.isArray(comp.content) ? comp.content : [];
  if (footer.length !== 2 || !content.length) return false;
  const allButtons = footer.every((b) => String(b?.type || "").toLowerCase() === "button");
  const allText = content.every((c) => String(c?.type || "").toLowerCase() === "text");
  const hasCancel = footer.some(
    (b) =>
      String(b?.variant || "").toLowerCase() === "ghost" ||
      /^(cancel|dismiss|no)\b/i.test(String(b?.action_id || "")) ||
      /^(cancel|dismiss|no)\b/i.test(String(b?.label || ""))
  );
  return allButtons && allText && hasCancel;
};

const classifyUiResponse = (part, uiResp) => {
  const toolName =
    part?.functionResponse?.name || part?.function_response?.name || part?.name || "";
  const comps = Array.isArray(uiResp?.components) ? uiResp.components : [];
  const toasts = comps.filter((c) => String(c?.type || "").toLowerCase() === "toast");
  const rest = comps.filter((c) => String(c?.type || "").toLowerCase() !== "toast");

  if (toolName === "render_toast" || (toasts.length > 0 && rest.length === 0)) {
    return { kind: "toast", toolName, toasts };
  }
  if (
    toolName === "render_modal" ||
    (rest.length === 1 && String(rest[0]?.type || "").toLowerCase() === "modal")
  ) {
    return { kind: "modal", toolName, modal: rest[0], toasts };
  }
  if (rest.length === 1 && isConfirmCard(rest[0], toolName)) {
    return { kind: "confirm", toolName, confirmCard: rest[0], toasts };
  }
  const cleanResp = rest.length === comps.length ? uiResp : { ...uiResp, components: rest };
  return { kind: "surface", toolName, uiResp: cleanResp, toasts };
};

const buildStandaloneCards = (uiResp, toolName = "") => {
  const comps = Array.isArray(uiResp.components) ? uiResp.components : [];
  const cards = [];

  if (
    (comps.length === 2 && compType(comps[0]) === "text" && compType(comps[1]) === "table") ||
    (comps.length === 1 && compType(comps[0]) === "table")
  ) {
    const title = comps.length === 2 ? comps[0].content : comps[0].title || "Data Table";
    const tableComp = comps.length === 2 ? comps[1] : comps[0];
    const { element, toolbar } = renderTableParts(tableComp);
    const { card, body } = createStandaloneCard({
      title,
      headerRight: toolbar,
      flushBody: true,
    });
    body.appendChild(element);
    cards.push(card);
    return { title, cards };
  }

  if (comps.length === 1 && compType(comps[0]) === "chart") {
    const chartComp = comps[0];
    const title = chartComp.title || "Chart";
    const { card, body } = createStandaloneCard({ title });
    body.appendChild(renderChartInner(chartComp));
    cards.push(card);
    return { title, cards };
  }

  if (comps.length === 1 && String(comps[0]?.type || "").toLowerCase() === "alert") {
    const { card, title } = createAlertCard(comps[0]);
    cards.push(card);
    return { title, cards };
  }

  let surfaceTitle = null;
  let pendingTitle = null;
  let pendingSubtitle = null;

  for (let i = 0; i < comps.length; i++) {
    const comp = comps[i];
    if (!comp || typeof comp !== "object") continue;
    const t = String(comp.type || "").toLowerCase();

    if (t === "text" && (comp.variant === "h1" || comp.variant === "h2") && !pendingTitle) {
      pendingTitle = comp.content;
      if (!surfaceTitle) surfaceTitle = comp.content;
      continue;
    }
    if (t === "text" && comp.variant === "caption" && pendingTitle && !pendingSubtitle) {
      pendingSubtitle = comp.content;
      continue;
    }

    if (
      t === "stack" &&
      Array.isArray(comp.children) &&
      comp.children.length === 2 &&
      compType(comp.children[0]) === "text" &&
      compType(comp.children[1]) === "grid"
    ) {
      const secTitle = comp.children[0].content;
      if (!surfaceTitle) surfaceTitle = secTitle;
      const { card, body } = createStandaloneCard({
        title: secTitle,
        description: pendingTitle
          ? `${pendingTitle}${pendingSubtitle ? ` · ${pendingSubtitle}` : ""}`
          : null,
      });
      pendingTitle = null;
      pendingSubtitle = null;
      body.appendChild(renderInnerComponent(comp.children[1], card, { inGrid: true }));
      cards.push(card);
      continue;
    }

    if (t === "card" || t === "modal") {
      let contentArr = Array.isArray(comp.content) ? [...comp.content] : [];
      const singleTable = contentArr.length === 1 && compType(contentArr[0]) === "table";
      const singleChart = contentArr.length === 1 && compType(contentArr[0]) === "chart";
      const isProgress =
        toolName === "render_progress" ||
        (!comp.title && contentArr.some((c) => String(c?.type || "").toLowerCase() === "progress"));

      let liftedTitle = null;
      let liftedDesc = null;
      if (isProgress && !comp.title && compType(contentArr[0]) === "text") {
        liftedTitle = contentArr.shift().content;
        if (compType(contentArr[0]) === "text" && String(contentArr[0].variant || "").toLowerCase() === "caption") {
          liftedDesc = contentArr.shift().content;
        }
      }

      let headerRight = null;
      let tableElement = null;
      if (singleTable) {
        const parts = renderTableParts(contentArr[0]);
        headerRight = parts.toolbar;
        tableElement = parts.element;
      }

      const cardTitle = comp.title || liftedTitle || pendingTitle || "Details";
      if (!surfaceTitle) surfaceTitle = cardTitle;

      const { card, body } = createStandaloneCard({
        title: cardTitle,
        description:
          comp.description ||
          liftedDesc ||
          (comp.title && pendingTitle ? pendingSubtitle : null),
        headerRight,
        flushBody: singleTable,
      });
      pendingTitle = null;
      pendingSubtitle = null;

      if (singleTable && tableElement) {
        body.appendChild(tableElement);
      } else if (singleChart) {
        body.appendChild(renderChartInner(contentArr[0]));
      } else {
        for (const child of contentArr) {
          body.appendChild(renderInnerComponent(child, card));
        }
      }

      if (Array.isArray(comp.footer) && comp.footer.length) {
        const ftr = document.createElement("footer");
        style(ftr, {
          display: "flex",
          flexWrap: "wrap",
          gap: "10px",
          justifyContent: "flex-end",
          padding: "4px 24px 22px",
          background: "transparent",
        });
        for (const child of comp.footer) {
          ftr.appendChild(renderInnerComponent(child, card));
        }
        card.appendChild(ftr);
      }

      cards.push(card);
      continue;
    }

    if (t === "alert") {
      const { card, title } = createAlertCard(comp);
      if (!surfaceTitle) surfaceTitle = title;
      pendingTitle = null;
      pendingSubtitle = null;
      cards.push(card);
      continue;
    }

    const fallbackTitle = comp.title || pendingTitle || "Component";
    if (!surfaceTitle) surfaceTitle = fallbackTitle;
    const { card, body } = createStandaloneCard({
      title: fallbackTitle,
      description: pendingSubtitle || null,
    });
    pendingTitle = null;
    pendingSubtitle = null;
    body.appendChild(renderInnerComponent(comp, card));
    cards.push(card);
  }

  return {
    title: surfaceTitle || "Dashboard",
    cards,
  };
};

const extractUiResponse = (obj) => {
  if (!obj || typeof obj !== "object") return null;
  if (
    Array.isArray(obj.components) &&
    obj.components.length > 0 &&
    typeof obj.components[0]?.type === "string"
  ) {
    return obj;
  }
  const candidates = [
    obj.functionResponse?.response,
    obj.function_response?.response,
    obj.response,
    obj.result,
  ];
  for (const c of candidates) {
    if (
      c &&
      typeof c === "object" &&
      Array.isArray(c.components) &&
      c.components.length > 0 &&
      typeof c.components[0]?.type === "string"
    ) {
      return c;
    }
  }
  return null;
};

// ---------------------------------------------------------------------------
// Transcript & Canvas Rendering
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// OpenTelemetry GenAI Token Usage (`adk_telemetry` / `gen_ai.usage.*`) & Cost
// ---------------------------------------------------------------------------

// Base rates for a model. Prefers the table served by the backend (see
// `ingestServerPricing`); the heuristics below are a fallback that mirrors
// `config::model_pricing` in src/config.rs rule-for-rule (keep both in sync).
// Official Paid Tier pricing per 1M tokens (USD) from https://ai.google.dev/gemini-api/docs/pricing
const getFallbackModelPricing = (m) => {
  if (m.includes("gemini-4") || m.includes("argon")) {
    // Gemini 4 Argon: $2.00 in, $0.10 cached, $10.00 out
    return {
      input: 2.0,
      inputLong: 2.0,
      cached: 0.1,
      cachedLong: 0.1,
      output: 10.0,
      outputLong: 10.0,
      audioIn: 2.0,
    };
  }
  if (m.includes("gemini-3.5-flash-lite")) {
    // Gemini 3.5 Flash-Lite: $0.30 in, $0.03 cached, $2.50 out
    return {
      input: 0.3,
      inputLong: 0.3,
      cached: 0.03,
      cachedLong: 0.03,
      output: 2.5,
      outputLong: 2.5,
      audioIn: 0.3,
    };
  }
  if (m.includes("gemini-3.1-flash-lite")) {
    // Gemini 3.1 Flash-Lite: $0.25 in ($0.50 audio), $0.025 cached, $1.50 out
    return {
      input: 0.25,
      inputLong: 0.25,
      cached: 0.025,
      cachedLong: 0.025,
      output: 1.5,
      outputLong: 1.5,
      audioIn: 0.5,
    };
  }
  if (m.includes("flash-lite") || m.includes("gemini-2.0-flash")) {
    // Gemini 2.5 Flash-Lite / 2.0 Flash: $0.10 in ($0.30 audio), $0.01 cached, $0.40 out
    return {
      input: 0.1,
      inputLong: 0.1,
      cached: 0.01,
      cachedLong: 0.01,
      output: 0.4,
      outputLong: 0.4,
      audioIn: 0.3,
    };
  }
  if ((m.includes("gemini-3.8") || m.includes("gemini-3.6")) && m.includes("flash")) {
    // Gemini 3.8 Flash & 3.6 Flash: $0.75 in, $0.075 cached, $3.75 out
    return {
      input: 0.75,
      inputLong: 0.75,
      cached: 0.075,
      cachedLong: 0.075,
      output: 3.75,
      outputLong: 3.75,
      audioIn: 0.75,
    };
  }
  if (m.includes("gemini-3") && m.includes("flash")) {
    // Gemini 3 Flash Preview: $0.50 in ($1.00 audio), $0.05 cached, $3.00 out
    return {
      input: 0.5,
      inputLong: 0.5,
      cached: 0.05,
      cachedLong: 0.05,
      output: 3.0,
      outputLong: 3.0,
      audioIn: 1.0,
    };
  }
  if (m.includes("flash")) {
    // Gemini 2.5 Flash: $0.30 in ($1.00 audio), $0.03 cached, $2.50 out
    return {
      input: 0.3,
      inputLong: 0.3,
      cached: 0.03,
      cachedLong: 0.03,
      output: 2.5,
      outputLong: 2.5,
      audioIn: 1.0,
    };
  }
  if (m.includes("gemini-3")) {
    // Gemini 3.1 Pro Preview: $2.00/$4.00 in, $0.20/$0.40 cached, $12.00/$18.00 out
    return {
      input: 2.0,
      inputLong: 4.0,
      cached: 0.2,
      cachedLong: 0.4,
      output: 12.0,
      outputLong: 18.0,
      audioIn: 2.0,
    };
  }
  // Gemini 2.5 Pro: $1.25/$2.50 in, $0.125/$0.25 cached, $10.00/$15.00 out
  return {
    input: 1.25,
    inputLong: 2.5,
    cached: 0.125,
    cachedLong: 0.25,
    output: 10.0,
    outputLong: 15.0,
    audioIn: 1.25,
  };
};

const getDefaultModelPricing = (model, tier = prefs.pricingTier) => {
  const m = String(model || prefs.model || DEFAULT_MODELS.model)
    .trim()
    .toLowerCase();
  const base = state.serverModelPricing[m] || getFallbackModelPricing(m);

  const mult = tier === "priority" ? 1.8 : tier === "batch" ? 0.5 : 1.0;
  const scale = (v) => Number((v * mult).toFixed(6));
  return {
    input: scale(base.input),
    inputLong: scale(base.inputLong),
    cached: scale(base.cached),
    cachedLong: scale(base.cachedLong),
    output: scale(base.output),
    outputLong: scale(base.outputLong),
    audioIn: scale(base.audioIn),
  };
};

const getModelPricingKey = (model, tier = prefs.pricingTier) => {
  const m = String(model || prefs.model || DEFAULT_MODELS.model)
    .trim()
    .toLowerCase();
  const t = String(tier || "standard").toLowerCase();
  return `${t}:${m}`;
};

const getModelPricing = (model) => {
  const tier = prefs.pricingTier || "standard";
  const defaults = getDefaultModelPricing(model, tier);
  const key = getModelPricingKey(model, tier);
  const custom =
    prefs.modelPricing && typeof prefs.modelPricing === "object"
      ? prefs.modelPricing[key]
      : null;
  if (!custom || typeof custom !== "object") {
    return { ...defaults, isCustom: false, tier };
  }

  const input =
    typeof custom.input === "number" && Number.isFinite(custom.input) && custom.input >= 0
      ? custom.input
      : defaults.input;
  const cached =
    typeof custom.cached === "number" && Number.isFinite(custom.cached) && custom.cached >= 0
      ? custom.cached
      : defaults.cached;
  const output =
    typeof custom.output === "number" && Number.isFinite(custom.output) && custom.output >= 0
      ? custom.output
      : defaults.output;

  const inLongRatio = defaults.input > 0 ? defaults.inputLong / defaults.input : 1;
  const cachedLongRatio = defaults.cached > 0 ? defaults.cachedLong / defaults.cached : 1;
  const outLongRatio = defaults.output > 0 ? defaults.outputLong / defaults.output : 1;
  const audioRatio = defaults.input > 0 ? defaults.audioIn / defaults.input : 1;

  return {
    input,
    inputLong: Number((input * inLongRatio).toFixed(6)),
    cached,
    cachedLong: Number((cached * cachedLongRatio).toFixed(6)),
    output,
    outputLong: Number((output * outLongRatio).toFixed(6)),
    audioIn: Number((input * audioRatio).toFixed(6)),
    isCustom: true,
    tier,
  };
};

const syncPricingControls = () => {
  const activeModel = prefs.model || DEFAULT_MODELS.model;
  const rates = getModelPricing(activeModel);
  if ($("setting-pricing-model-label")) {
    $("setting-pricing-model-label").textContent = activeModel;
  }
  if ($("setting-pricing-tier")) {
    $("setting-pricing-tier").value = prefs.pricingTier || "standard";
  }
  const inEl = $("setting-price-input");
  if (inEl && document.activeElement !== inEl) {
    inEl.value = String(rates.input);
  }
  const cachedEl = $("setting-price-cached");
  if (cachedEl && document.activeElement !== cachedEl) {
    cachedEl.value = String(rates.cached);
  }
  const outEl = $("setting-price-output");
  if (outEl && document.activeElement !== outEl) {
    outEl.value = String(rates.output);
  }
};

const estimateStreamedPartsTokens = (parts) => {
  let textChars = 0;
  let thinkingChars = 0;
  for (const p of Array.isArray(parts) ? parts : []) {
    if (!p || typeof p !== "object") continue;
    if (typeof p.thinking === "string") {
      thinkingChars += p.thinking.length;
    } else if (typeof p.text === "string") {
      textChars += p.text.length;
    }
    const fc = p.functionCall || p.function_call;
    if (fc && typeof fc === "object") {
      textChars += (fc.name || "").length + JSON.stringify(fc.args || {}).length;
    }
  }
  const thinkingTokens = Math.ceil(thinkingChars / 4);
  const textTokens = Math.ceil(textChars / 4);
  return {
    thinkingTokens,
    outputTokens: textTokens + thinkingTokens,
  };
};

const isUserEvent = (ev) => ev?.content?.role === "user" || ev?.author === "user";

const extractEventUsage = (ev) => {
  if (isUserEvent(ev)) return null;

  const u = ev?.usage_metadata || ev?.usageMetadata;
  const streamedEst = ev?.partial
    ? estimateStreamedPartsTokens(ev?.content?.parts)
    : { thinkingTokens: 0, outputTokens: 0 };

  if (!u || typeof u !== "object") {
    if (!ev?.partial || streamedEst.outputTokens === 0) return null;
    const model = prefs.model || DEFAULT_MODELS.model;
    const p = getModelPricing(model);
    const outputCost = (streamedEst.outputTokens / 1e6) * p.output;
    return {
      model,
      inputTokens: 0,
      uncachedInputTokens: 0,
      outputTokens: streamedEst.outputTokens,
      totalTokens: streamedEst.outputTokens,
      cacheReadTokens: 0,
      cacheCreationTokens: 0,
      thinkingTokens: streamedEst.thinkingTokens,
      audioInputTokens: 0,
      audioOutputTokens: 0,
      inputCost: 0,
      cacheReadCost: 0,
      audioCost: 0,
      outputCost,
      totalCost: outputCost,
    };
  }

  const prov = u.provider_usage && typeof u.provider_usage === "object" ? u.provider_usage : {};
  const model =
    prov["gen_ai.request.model"] || prov.model || prefs.model || DEFAULT_MODELS.model;

  const inputTokens = Math.max(
    0,
    Number(
      u.prompt_token_count ?? prov["gen_ai.usage.input_tokens"] ?? prov.promptTokenCount ?? 0
    ) || 0
  );
  const rawOutputTokens = Math.max(
    0,
    Number(
      u.candidates_token_count ??
        prov["gen_ai.usage.output_tokens"] ??
        prov.candidatesTokenCount ??
        0
    ) || 0
  );
  const outputTokens = ev?.partial
    ? Math.max(rawOutputTokens, streamedEst.outputTokens)
    : rawOutputTokens;
  const rawTotalTokens = Math.max(
    0,
    Number(
      u.total_token_count ??
        prov["gen_ai.usage.total_tokens"] ??
        prov.totalTokenCount ??
        inputTokens + outputTokens
    ) || 0
  );
  const totalTokens = Math.max(rawTotalTokens, inputTokens + outputTokens);
  const cacheReadTokens = Math.min(
    inputTokens,
    Math.max(
      0,
      Number(
        u.cache_read_input_token_count ??
          prov["gen_ai.usage.cache_read_tokens"] ??
          prov.cachedContentTokenCount ??
          0
      ) || 0
    )
  );
  const cacheCreationTokens = Math.max(
    0,
    Number(
      u.cache_creation_input_token_count ?? prov["gen_ai.usage.cache_creation_tokens"] ?? 0
    ) || 0
  );
  const rawThinkingTokens = Math.max(
    0,
    Number(
      u.thinking_token_count ??
        prov["gen_ai.usage.thinking_tokens"] ??
        prov.thoughtsTokenCount ??
        0
    ) || 0
  );
  const thinkingTokens = ev?.partial
    ? Math.max(rawThinkingTokens, streamedEst.thinkingTokens)
    : rawThinkingTokens;
  const remainingInput = Math.max(0, inputTokens - cacheReadTokens);
  const audioInputTokens = Math.min(
    remainingInput,
    Math.max(
      0,
      Number(u.audio_input_token_count ?? prov["gen_ai.usage.audio_input_tokens"] ?? 0) || 0
    )
  );
  const audioOutputTokens = Math.max(
    0,
    Number(u.audio_output_token_count ?? prov["gen_ai.usage.audio_output_tokens"] ?? 0) || 0
  );
  const uncachedInputTokens = Math.max(0, remainingInput - audioInputTokens);

  const p = getModelPricing(model);
  const isLong = inputTokens > 200000;
  const inputRate = isLong ? p.inputLong : p.input;
  const cachedRate = isLong ? p.cachedLong : p.cached;
  const outputRate = isLong ? p.outputLong : p.output;

  const inputCost = (uncachedInputTokens / 1e6) * inputRate;
  const cacheReadCost = (cacheReadTokens / 1e6) * cachedRate;
  const audioCost = (audioInputTokens / 1e6) * p.audioIn;
  const outputCost = (outputTokens / 1e6) * outputRate;
  const computedTotalCost = inputCost + cacheReadCost + audioCost + outputCost;
  const totalCost =
    computedTotalCost > 0 || inputTokens + outputTokens > 0
      ? computedTotalCost
      : typeof u.cost === "number" && Number.isFinite(u.cost)
        ? u.cost
        : 0;

  return {
    model,
    inputTokens,
    uncachedInputTokens,
    outputTokens,
    totalTokens,
    cacheReadTokens,
    cacheCreationTokens,
    thinkingTokens,
    audioInputTokens,
    audioOutputTokens,
    inputCost,
    cacheReadCost,
    audioCost,
    outputCost,
    totalCost,
  };
};

// Usage of a finished (non-partial) event only changes when the pricing
// inputs change, so cache it per event object and drop the whole cache when
// the model / tier / custom rates / server table move. Partial events are
// re-estimated every time (their parts keep growing).
let usageCache = new WeakMap();
let usageCacheSig = "";
let usageCacheServerPricing = null;

const syncUsageCache = () => {
  const sig = `${prefs.model}|${prefs.pricingTier || "standard"}|${JSON.stringify(
    prefs.modelPricing || {}
  )}`;
  if (sig !== usageCacheSig || state.serverModelPricing !== usageCacheServerPricing) {
    usageCache = new WeakMap();
    usageCacheSig = sig;
    usageCacheServerPricing = state.serverModelPricing;
  }
};

const cachedEventUsage = (ev) => {
  if (!ev || typeof ev !== "object" || ev.partial) return extractEventUsage(ev);
  if (usageCache.has(ev)) return usageCache.get(ev);
  const u = extractEventUsage(ev);
  usageCache.set(ev, u);
  return u;
};

const computeSessionUsage = (events) => {
  syncUsageCache();
  // Dedupe by id in place (last occurrence wins) without reordering events.
  const list = Array.isArray(events) ? events : [];
  const lastIdxById = new Map();
  list.forEach((ev, idx) => {
    if (ev && typeof ev === "object" && ev.id) lastIdxById.set(ev.id, idx);
  });
  const allEvents = list.filter(
    (ev, idx) => ev && typeof ev === "object" && (!ev.id || lastIdxById.get(ev.id) === idx)
  );

  const summary = {
    llmCalls: 0,
    inputTokens: 0,
    outputTokens: 0,
    totalTokens: 0,
    cacheReadTokens: 0,
    cacheCreationTokens: 0,
    thinkingTokens: 0,
    audioInputTokens: 0,
    audioOutputTokens: 0,
    inputCost: 0,
    cacheReadCost: 0,
    audioCost: 0,
    outputCost: 0,
    totalCost: 0,
    models: new Set(),
  };

  for (const ev of allEvents) {
    const u = cachedEventUsage(ev);
    if (!u) continue;
    summary.llmCalls += 1;
    summary.inputTokens += u.inputTokens;
    summary.outputTokens += u.outputTokens;
    summary.totalTokens += u.totalTokens;
    summary.cacheReadTokens += u.cacheReadTokens;
    summary.cacheCreationTokens += u.cacheCreationTokens;
    summary.thinkingTokens += u.thinkingTokens;
    summary.audioInputTokens += u.audioInputTokens;
    summary.audioOutputTokens += u.audioOutputTokens;
    summary.inputCost += u.inputCost;
    summary.cacheReadCost += u.cacheReadCost;
    summary.audioCost += u.audioCost;
    summary.outputCost += u.outputCost;
    summary.totalCost += u.totalCost;
    if (u.model) summary.models.add(u.model);
  }

  return summary;
};

const updateSessionUsageUi = () => {
  const s = computeSessionUsage(state.currentEvents);
  const activeModel = prefs.model || DEFAULT_MODELS.model;
  const modelsList = s.models.size > 0 ? Array.from(s.models) : [activeModel];
  const primaryModel = modelsList[modelsList.length - 1] || activeModel;
  const rates = getModelPricing(primaryModel);
  const showSession = !!prefs.showSessionUsage;
  const showCost = !!prefs.showCost;

  if ($("session-usage-anchor")) {
    $("session-usage-anchor").classList.toggle("hidden", !showSession);
    if (!showSession && els.sessionUsagePopover) {
      els.sessionUsagePopover.classList.add("hidden");
    }
  }

  if (els.sessionUsageBtn) {
    const prevBtnTokens = els.sessionUsageBtn._lastSessionTokens;
    if (
      typeof prevBtnTokens === "number" &&
      s.totalTokens > prevBtnTokens &&
      showSession
    ) {
      triggerPulseClass(els.sessionUsageBtn, "usage-pill-bump");
    }
    els.sessionUsageBtn._lastSessionTokens = s.totalTokens;
  }

  animateCounter(
    $("session-usage-tokens"),
    s.totalTokens,
    (v) => `${formatExactInt(v)} tok`
  );
  if ($("session-usage-sep")) {
    $("session-usage-sep").classList.toggle("hidden", !showCost);
  }
  if ($("session-usage-cost")) {
    $("session-usage-cost").classList.toggle("hidden", !showCost);
    animateCounter($("session-usage-cost"), s.totalCost, formatUsdShort);
  }
  if (els.sessionUsageBtn) {
    const costSuffix = showCost ? ` · Est. ${formatUsdDetail(s.totalCost)}` : "";
    els.sessionUsageBtn.title = `Session usage: ${formatExactInt(s.totalTokens)} tokens (${formatExactInt(
      s.inputTokens
    )} input, ${formatExactInt(s.outputTokens)} output)${costSuffix} across ${s.llmCalls} LLM call${
      s.llmCalls === 1 ? "" : "s"
    }`;
  }

  animateCounter($("usage-total-tokens"), s.totalTokens, formatExactInt);
  animateCounter(
    $("usage-llm-calls"),
    s.llmCalls,
    (v) => {
      const n = Math.round(v);
      return `${n} LLM call${n === 1 ? "" : "s"}`;
    }
  );
  if ($("usage-kpi-cost-card")) {
    $("usage-kpi-cost-card").classList.toggle("hidden", !showCost);
  }
  animateCounter($("usage-total-cost"), s.totalCost, formatUsdDetail);
  if ($("usage-active-model")) {
    $("usage-active-model").textContent = modelsList.join(", ");
  }

  animateCounter($("usage-input-tokens"), s.inputTokens, formatExactInt);
  if ($("usage-input-cost")) {
    $("usage-input-cost").classList.toggle("hidden", !showCost);
    animateCounter($("usage-input-cost"), s.inputCost, formatUsdDetail);
  }
  animateCounter($("usage-cache-read-tokens"), s.cacheReadTokens, formatExactInt);
  if ($("usage-cache-read-cost")) {
    $("usage-cache-read-cost").classList.toggle("hidden", !showCost);
    animateCounter($("usage-cache-read-cost"), s.cacheReadCost, formatUsdDetail);
  }

  const cacheCreateRow = $("usage-row-cache-creation");
  if (cacheCreateRow) {
    cacheCreateRow.classList.toggle("hidden", s.cacheCreationTokens === 0);
    animateCounter(
      $("usage-cache-creation-tokens"),
      s.cacheCreationTokens,
      formatExactInt
    );
  }

  animateCounter($("usage-output-tokens"), s.outputTokens, formatExactInt);
  if ($("usage-output-cost")) {
    $("usage-output-cost").classList.toggle("hidden", !showCost);
    animateCounter($("usage-output-cost"), s.outputCost, formatUsdDetail);
  }
  animateCounter($("usage-thinking-tokens"), s.thinkingTokens, formatExactInt);

  const audioRow = $("usage-row-audio");
  if (audioRow) {
    const hasAudio = s.audioInputTokens > 0 || s.audioOutputTokens > 0;
    audioRow.classList.toggle("hidden", !hasAudio);
    const totalAudio = s.audioInputTokens + s.audioOutputTokens;
    animateCounter(
      $("usage-audio-tokens"),
      totalAudio,
      (v) => {
        const r = totalAudio > 0 ? v / totalAudio : 1;
        return `${formatExactInt(s.audioInputTokens * r)} / ${formatExactInt(
          s.audioOutputTokens * r
        )}`;
      },
      { formatSig: `${s.audioInputTokens}:${s.audioOutputTokens}` }
    );
    if ($("usage-audio-cost")) {
      $("usage-audio-cost").classList.toggle("hidden", !showCost);
      animateCounter($("usage-audio-cost"), s.audioCost, formatUsdDetail);
    }
  }

  if ($("usage-rate-note")) {
    const tierLabel = rates.isCustom
      ? "Custom"
      : rates.tier === "priority"
        ? "Paid Priority"
        : rates.tier === "batch"
          ? "Paid Batch/Flex"
          : "Paid Standard";
    $("usage-rate-note").textContent = showCost
      ? `${tierLabel} (${primaryModel}): $${rates.input}/1M in · $${rates.cached}/1M cached · $${rates.output}/1M out`
      : `Model (${primaryModel}) · OpenTelemetry gen_ai.usage.* counters`;
  }

  if ($("menu-session-tokens-row")) {
    $("menu-session-tokens-row").classList.toggle("hidden", !showSession);
  }
  animateCounter(
    $("menu-session-tokens"),
    s.totalTokens,
    (v) => `${formatExactInt(v)} tok`
  );
  if ($("menu-session-cost-row")) {
    $("menu-session-cost-row").classList.toggle("hidden", !showSession || !showCost);
  }
  animateCounter($("menu-session-cost"), s.totalCost, formatUsdDetail);
};

const isPlainTextPart = (part) =>
  part &&
  typeof part.text === "string" &&
  !part.thinking &&
  !part.functionCall &&
  !part.function_call &&
  !part.functionResponse &&
  !part.function_response &&
  !part.inlineData &&
  !part.inline_data;

const coalesceParts = (parts) => {
  const out = [];
  for (const part of Array.isArray(parts) ? parts : []) {
    if (!part || typeof part !== "object") continue;
    if (isPlainTextPart(part)) {
      const last = out[out.length - 1];
      if (last && isPlainTextPart(last)) {
        last.text += part.text;
        continue;
      }
    } else if (typeof part.thinking === "string") {
      const last = out[out.length - 1];
      if (last && typeof last.thinking === "string" && !isPlainTextPart(last)) {
        last.thinking += part.thinking;
        continue;
      }
    }
    out.push({ ...part });
  }
  return out;
};

const extractConcatenatedText = (parts) =>
  (Array.isArray(parts) ? parts : [])
    .map((p) => (typeof p?.text === "string" ? p.text : ""))
    .join("");

const mergeStreamedEvent = (prev, incoming) => {
  const prevParts = Array.isArray(prev?.content?.parts) ? prev.content.parts : [];
  const nextParts = Array.isArray(incoming?.content?.parts) ? incoming.content.parts : [];
  const mergedUsage =
    incoming?.usage_metadata ??
    incoming?.usageMetadata ??
    prev?.usage_metadata ??
    prev?.usageMetadata ??
    null;

  if (incoming.partial) {
    return {
      ...prev,
      ...incoming,
      usage_metadata: mergedUsage,
      content: {
        ...(prev?.content || {}),
        ...(incoming?.content || {}),
        parts: coalesceParts([...prevParts, ...nextParts]),
      },
    };
  }

  const contentComplete = incoming?.provider_metadata?.content_complete === true;
  const prevText = extractConcatenatedText(prevParts);
  const nextText = extractConcatenatedText(nextParts);

  let finalParts;
  if (nextParts.length === 0 && prevParts.length > 0) {
    finalParts = coalesceParts(prevParts);
  } else if (
    prev?.partial &&
    !contentComplete &&
    prevText &&
    (!nextText || !nextText.startsWith(prevText))
  ) {
    finalParts = coalesceParts([...prevParts, ...nextParts]);
  } else {
    finalParts = coalesceParts(nextParts);
  }

  return {
    ...prev,
    ...incoming,
    usage_metadata: mergedUsage,
    partial: false,
    content: {
      ...(prev?.content || {}),
      ...(incoming?.content || {}),
      parts: finalParts,
    },
  };
};

const shownOverlayIds = new Set();
let overlayTriggersById = new Map();

const rebuildFromSessionEvents = (
  events,
  { forceCanvas = false, suppressOverlays = false } = {}
) => {
  state.currentEvents = Array.isArray(events) ? events : [];
  const prevSurfaceIds = state.canvasSurfaces.map((s) => s.id).join("|");
  const existingById = new Map(state.canvasSurfaces.map((s) => [s.id, s]));
  const nextSurfaces = [];
  const nextOverlays = new Map();

  state.currentEvents.forEach((ev, idx) => {
    const parts = ev?.content?.parts || [];
    let uiIndex = 0;
    parts.forEach((part) => {
      const uiResp = extractUiResponse(part);
      if (!uiResp) return;
      const slotIndex = uiIndex++;
      const surfaceId = `surface-${ev.id || idx}-${slotIndex}`;
      const overlayKey = `${state.activeSession?.id || "anon"}:${ev.id || idx}:${slotIndex}`;
      const classified = classifyUiResponse(part, uiResp);

      if (classified.toasts.length > 0) {
        const toastKey = `toast:${overlayKey}`;
        if (suppressOverlays) {
          shownOverlayIds.add(toastKey);
        } else if (!ev.partial && !shownOverlayIds.has(toastKey)) {
          shownOverlayIds.add(toastKey);
          classified.toasts.forEach(showToast);
        }
      }

      if (classified.kind === "toast") {
        return;
      }

      if (classified.kind === "modal") {
        const modalKey = `modal:${overlayKey}`;
        nextOverlays.set(surfaceId, {
          id: surfaceId,
          title: classified.modal?.title || "Dialog",
          cta: "Open Modal",
          icon: "dialogs",
          onOpen: () => showModalOverlay(classified.modal),
        });
        if (suppressOverlays) {
          shownOverlayIds.add(modalKey);
        } else if (!ev.partial && !shownOverlayIds.has(modalKey)) {
          shownOverlayIds.add(modalKey);
          showModalOverlay(classified.modal);
        }
        return;
      }

      if (classified.kind === "confirm") {
        const confirmKey = `confirm:${overlayKey}`;
        nextOverlays.set(surfaceId, {
          id: surfaceId,
          title: classified.confirmCard?.title || "Confirm Action",
          cta: "Review",
          icon: "help",
          onOpen: () => showModalOverlay(classified.confirmCard, { isConfirm: true }),
        });
        if (suppressOverlays) {
          shownOverlayIds.add(confirmKey);
        } else if (!ev.partial && !shownOverlayIds.has(confirmKey)) {
          shownOverlayIds.add(confirmKey);
          showModalOverlay(classified.confirmCard, { isConfirm: true });
        }
        return;
      }

      const prev = existingById.get(surfaceId);
      if (prev) {
        nextSurfaces.push(prev);
      } else {
        const built = buildStandaloneCards(classified.uiResp, classified.toolName);
        if (built.cards.length > 0) {
          nextSurfaces.push({
            id: surfaceId,
            title: built.title,
            raw: classified.uiResp,
            cards: built.cards,
          });
        }
      }
    });
  });
  state.canvasSurfaces = nextSurfaces;
  overlayTriggersById = nextOverlays;
  const nextSurfaceIds = state.canvasSurfaces.map((s) => s.id).join("|");

  renderTranscript();
  if (forceCanvas || prevSurfaceIds !== nextSurfaceIds) {
    renderCanvas();
  }
};

const focusSurfaceOnCanvas = (surfaceId) => {
  state.activeSurfaceId = surfaceId;
  prefs.canvasMode = "focused";
  applyPreferences({ rerender: false });
  renderCanvas();
  const target = document.querySelector(`[data-surface-id="${CSS.escape(surfaceId)}"]`);
  if (target) {
    target.scrollIntoView({ behavior: "smooth", block: "start" });
  }
};

const createAvatarElement = (isUser, authorLabel) => {
  const avatar = document.createElement("span");
  avatar.className = `avatar ${isUser ? "avatar-user" : "avatar-agent"}`;
  avatar.title = authorLabel;
  if (isUser) {
    if (state.currentUser?.picture) {
      const img = document.createElement("img");
      img.src = state.currentUser.picture;
      img.alt = authorLabel;
      img.referrerPolicy = "no-referrer";
      img.onerror = () => {
        avatar.textContent = initialsOf(authorLabel);
      };
      avatar.appendChild(img);
    } else {
      avatar.textContent = initialsOf(authorLabel);
    }
  } else {
    avatar.innerHTML = `<span class="google-symbols filled" aria-hidden="true">robot</span>`;
  }
  return avatar;
};

const parseUiEventMessage = (text) => {
  if (typeof text !== "string" || !text.startsWith("[UI Event:")) return null;
  const match = text.match(/^\[UI Event:\s*([^\]]+)\]\s*(?:\r?\n|$)([\s\S]*)$/);
  if (!match) return null;
  const eventType = match[1].trim();
  const rest = match[2] || "";
  let action = "";
  const fields = [];
  let inData = false;
  for (const rawLine of rest.split(/\r?\n/)) {
    if (!rawLine.trim()) continue;
    if (!inData && rawLine.startsWith("Action:")) {
      action = rawLine.slice("Action:".length).trim();
      continue;
    }
    if (!inData && rawLine.trim() === "Data:") {
      inData = true;
      continue;
    }
    const kv = rawLine.match(/^\s*([^:]+):\s*(.*)$/);
    if (kv) {
      fields.push({ key: kv[1].trim(), value: kv[2].trim() });
    }
  }
  return { eventType, action, fields };
};

const renderUiEventInto = (container, parsed) => {
  container.replaceChildren();
  const wrap = document.createElement("div");
  wrap.className = "ui-event-card";

  const header = document.createElement("div");
  header.className = "ui-event-header";

  const badge = document.createElement("span");
  badge.className = "ui-event-badge";
  const icon = document.createElement("span");
  icon.className = "google-symbols";
  icon.setAttribute("aria-hidden", "true");
  icon.textContent = /form/i.test(parsed.eventType) ? "fact_check" : "ads_click";
  const title = document.createElement("span");
  title.className = "ui-event-title";
  title.textContent = parsed.eventType;
  badge.append(icon, title);
  header.appendChild(badge);

  if (parsed.action) {
    const actionCode = document.createElement("code");
    actionCode.className = "ui-event-action";
    actionCode.textContent = parsed.action;
    header.appendChild(actionCode);
  }
  wrap.appendChild(header);

  if (parsed.fields.length > 0) {
    const grid = document.createElement("div");
    grid.className = "ui-event-fields";
    for (const f of parsed.fields) {
      const row = document.createElement("div");
      row.className = "ui-event-field";
      const keyEl = document.createElement("span");
      keyEl.className = "ui-event-key";
      keyEl.textContent = f.key;
      const valEl = document.createElement("code");
      valEl.className = "ui-event-val";
      valEl.textContent = f.value || "—";
      row.append(keyEl, valEl);
      grid.appendChild(row);
    }
    wrap.appendChild(grid);
  }

  container.appendChild(wrap);
};

const updateUsageFooter = (parentEl, { isUser, usage }) => {
  const usageSig =
    !isUser && usage && prefs.showMessageUsage
      ? `${usage.totalTokens}:${usage.inputTokens}:${usage.outputTokens}:${usage.thinkingTokens}:${usage.cacheReadTokens}:${prefs.showCost ? usage.totalCost.toFixed(6) : "nocost"}`
      : "";
  if (parentEl.dataset.usageSig === usageSig) return;
  parentEl.dataset.usageSig = usageSig;

  let usageEl = parentEl.querySelector(":scope > .message-usage");
  if (usageSig && usage) {
    if (!usageEl) {
      usageEl = document.createElement("button");
      usageEl.type = "button";
      usageEl.className = "message-usage";
      usageEl.setAttribute("aria-haspopup", "dialog");
      if (els.sessionUsagePopover?.id) usageEl.setAttribute("aria-controls", els.sessionUsagePopover.id);
      usageEl.addEventListener("click", (e) => {
        e.stopPropagation();
        if (els.sessionUsagePopover && prefs.showSessionUsage) {
          closePopover(els.settingsBtn, els.settingsMenu);
          closePopover(els.userBtn, els.userMenu);
          updateSessionUsageUi();
          openPopover(els.sessionUsageBtn, els.sessionUsagePopover);
        }
      });
      parentEl.appendChild(usageEl);
    }
    let leftSpan = usageEl.querySelector(":scope > .message-usage-tokens");
    if (!leftSpan) {
      leftSpan = document.createElement("span");
      leftSpan.className = "message-usage-tokens";
      usageEl.prepend(leftSpan);
    }
    animateCounter(
      leftSpan,
      usage.totalTokens,
      (v) => {
        const r = usage.totalTokens > 0 ? v / usage.totalTokens : 1;
        const detailParts = [
          `${formatExactInt(usage.inputTokens * r)} in`,
          `${formatExactInt(usage.outputTokens * r)} out`,
        ];
        if (usage.thinkingTokens > 0) {
          detailParts.push(`${formatExactInt(usage.thinkingTokens * r)} thinking`);
        }
        if (usage.cacheReadTokens > 0) {
          detailParts.push(`${formatExactInt(usage.cacheReadTokens * r)} cached`);
        }
        return `${formatExactInt(v)} tok (${detailParts.join(" · ")})`;
      },
      {
        formatSig: `${usage.inputTokens}:${usage.outputTokens}:${usage.thinkingTokens}:${usage.cacheReadTokens}`,
      }
    );
    usageEl.title = `gen_ai.request.model=${usage.model} · gen_ai.usage.input_tokens=${usage.inputTokens} · gen_ai.usage.output_tokens=${usage.outputTokens} · gen_ai.usage.total_tokens=${usage.totalTokens}`;
    let rightSpan = usageEl.querySelector(":scope > .message-usage-cost");
    if (prefs.showCost) {
      if (!rightSpan) {
        rightSpan = document.createElement("span");
        rightSpan.className = "message-usage-cost";
        usageEl.appendChild(rightSpan);
      }
      animateCounter(rightSpan, usage.totalCost, formatUsdShort);
    } else if (rightSpan) {
      rightSpan.remove();
    }
  } else if (usageEl) {
    usageEl.remove();
  }
};

const updateMessageArticle = (
  article,
  { isUser, partial, textContent, visibleTools, usage }
) => {
  // 1. Streaming indicator
  let st = article.querySelector(":scope > .streaming-tag");
  if (partial) {
    if (!st) {
      st = document.createElement("div");
      st.className = "streaming-tag";
      st.textContent = "streaming…";
      article.prepend(st);
    }
  } else if (st) {
    st.remove();
  }

  // 2. Message text container (updated in place only when text changed)
  let textEl = article.querySelector(":scope > .message-text");
  if (textContent) {
    if (!textEl) {
      textEl = document.createElement("div");
      textEl.className = "message-text";
      if (st && st.nextSibling) {
        article.insertBefore(textEl, st.nextSibling);
      } else if (st) {
        article.appendChild(textEl);
      } else {
        article.prepend(textEl);
      }
    }
    if (textEl._renderedText !== textContent) {
      textEl._renderedText = textContent;
      const parsedUiEvent = isUser ? parseUiEventMessage(textContent) : null;
      if (parsedUiEvent) {
        article.classList.add("message-ui-event");
        textEl.classList.add("message-text-ui-event");
        renderUiEventInto(textEl, parsedUiEvent);
      } else {
        article.classList.remove("message-ui-event");
        textEl.classList.remove("message-text-ui-event");
        if (isUser) {
          textEl.textContent = textContent;
        } else {
          textEl.replaceChildren();
          parseMarkdownBlocksInto(textEl, textContent);
        }
      }
    }
  } else {
    article.classList.remove("message-ui-event");
    if (textEl) textEl.remove();
  }

  // Clear any legacy usage footer inside bubble
  const legacyUsage = article.querySelector(":scope > .message-usage");
  if (legacyUsage) legacyUsage.remove();
};

// Serialised tool parts are compared on every render; loaded events keep
// the same part objects, so memoise the JSON per object.
const toolPartSigCache = new WeakMap();
const toolPartSig = (tp) => {
  if (!tp || typeof tp !== "object") return String(tp);
  let sig = toolPartSigCache.get(tp);
  if (sig === undefined) {
    sig = JSON.stringify(tp);
    toolPartSigCache.set(tp, sig);
  }
  return sig;
};

const updateMessageCol = (col, item) => {
  const hasBubble = item.partial || Boolean(item.textContent);
  let article = col.querySelector(":scope > article.message");

  if (hasBubble) {
    if (!article) {
      article = document.createElement("article");
      article.className = `message ${item.isUser ? "message-user" : "message-agent"}`;
      col.prepend(article);
    }
    updateMessageArticle(article, item);
  } else if (article) {
    article.remove();
  }

  // Tool calls live outside the chat bubble as soft-gray pills
  const toolSigs = item.visibleTools.map(toolPartSig);
  const toolsSig = toolSigs.join("\u0001");
  if (col.dataset.toolsSig !== toolsSig) {
    col.dataset.toolsSig = toolsSig;
    let toolGroup = col.querySelector(":scope > .chat-tool-calls");
    if (item.visibleTools.length === 0) {
      if (toolGroup) toolGroup.remove();
    } else {
      if (!toolGroup) {
        toolGroup = document.createElement("div");
        toolGroup.className = "chat-tool-calls";
        const beforeEl =
          col.querySelector(":scope > .chat-canvas-buttons") ||
          col.querySelector(":scope > .message-usage");
        if (beforeEl) {
          col.insertBefore(toolGroup, beforeEl);
        } else {
          col.appendChild(toolGroup);
        }
      }
      // Reconcile by position: keep an unchanged <details> (so its open
      // state survives) and only build the ones that are new or changed.
      const existing = Array.from(toolGroup.children);
      item.visibleTools.forEach((tp, i) => {
        const sig = toolSigs[i];
        const prevEl = existing[i];
        if (prevEl && prevEl._toolSig === sig) return;
        const details = document.createElement("details");
        details.className = "tool-call";
        details._toolSig = sig;
        const summary = document.createElement("summary");
        const isResp = "functionResponse" in tp || "function_response" in tp;
        const name =
          tp.name ||
          tp.functionCall?.name ||
          tp.function_call?.name ||
          tp.functionResponse?.name ||
          tp.function_response?.name ||
          "tool";
        summary.textContent = isResp ? `Result · ${name}` : `Tool · ${name}`;
        const pre = document.createElement("pre");
        pre.textContent = JSON.stringify(tp, null, 2);
        details.append(summary, pre);
        if (prevEl) {
          toolGroup.replaceChild(details, prevEl);
        } else {
          toolGroup.appendChild(details);
        }
      });
      while (toolGroup.children.length > item.visibleTools.length) {
        toolGroup.lastElementChild.remove();
      }
    }
  }

  // Canvas jump buttons live outside the chat bubble (styled like .cover__buttons)
  const surfacesSig = item.surfaceRefs.map((s) => s.id).join("|");
  if (col.dataset.surfacesSig !== surfacesSig) {
    col.dataset.surfacesSig = surfacesSig;
    let btnGroup = col.querySelector(":scope > .chat-canvas-buttons");
    if (item.surfaceRefs.length === 0) {
      if (btnGroup) btnGroup.remove();
    } else {
      if (!btnGroup) {
        btnGroup = document.createElement("div");
        btnGroup.className = "chat-canvas-buttons";
        const standaloneUsage = col.querySelector(":scope > .message-usage");
        if (standaloneUsage) {
          col.insertBefore(btnGroup, standaloneUsage);
        } else {
          col.appendChild(btnGroup);
        }
      }
      btnGroup.replaceChildren();
      for (const surf of item.surfaceRefs) {
        const btn = document.createElement("button");
        btn.type = "button";
        btn.className = "button button--protected canvas-jump-btn";

        const icon = document.createElement("span");
        icon.className = "google-symbols filled canvas-jump-icon";
        icon.setAttribute("aria-hidden", "true");
        icon.textContent = surf.icon || "spark";

        const label = document.createElement("span");
        label.className = "button__label canvas-jump-label";
        const strong = document.createElement("strong");
        strong.textContent = surf.title;
        label.appendChild(strong);

        const cta = document.createElement("span");
        cta.className = "canvas-jump-cta";
        cta.textContent = surf.cta || "View on Canvas →";

        btn.append(icon, label, cta);
        btn.addEventListener("click", () => {
          if (typeof surf.onOpen === "function") {
            surf.onOpen();
          } else {
            focusSurfaceOnCanvas(surf.id);
          }
        });
        btnGroup.appendChild(btn);
      }
    }
  }

  // Per-turn token telemetry & cost printed cleanly below the message/pills
  updateUsageFooter(col, { isUser: item.isUser, usage: item.usage });
};

const renderTranscript = () => {
  updateSessionUsageUi();
  // Decide whether to follow the bottom *before* mutating the DOM: only
  // stick when the user is already near the bottom (or the session changed),
  // so scrolling up to read an earlier message is not fought by streaming.
  const sessionKey = state.activeSession?.id || "";
  const isNewSessionView = els.chatTranscript._renderedSessionId !== sessionKey;
  els.chatTranscript._renderedSessionId = sessionKey;
  const nearBottom =
    els.chatTranscript.scrollHeight - els.chatTranscript.scrollTop - els.chatTranscript.clientHeight < 40;
  const stickToBottom = prefs.autoScroll && (nearBottom || isNewSessionView);
  const visibleItems = [];

  state.currentEvents.forEach((ev, idx) => {
    const parts = ev?.content?.parts || [];
    const hasToolOrUi = parts.some(
      (p) =>
        p &&
        ("functionCall" in p ||
          "function_call" in p ||
          "functionResponse" in p ||
          "function_response" in p)
    );
    const isUser = !hasToolOrUi && isUserEvent(ev);

    const textContent = parts
      .map((p) => (typeof p?.text === "string" ? p.text : ""))
      .filter(Boolean)
      .join("");

    const toolParts = [];
    const surfaceRefs = [];
    let uiIndex = 0;

    parts.forEach((part) => {
      if (!part || typeof part !== "object") return;
      const callName =
        part?.name || part?.functionCall?.name || part?.function_call?.name || "";
      if (callName && String(callName).startsWith("render_")) {
        return; // Skip raw `render_*` functionCall in chat
      }
      const uiResp = extractUiResponse(part);
      if (uiResp) {
        const surfaceId = `surface-${ev.id || idx}-${uiIndex++}`;
        const found =
          state.canvasSurfaces.find((s) => s.id === surfaceId) || overlayTriggersById.get(surfaceId);
        if (found) surfaceRefs.push(found);
        return;
      }
      if ("name" in part || "functionResponse" in part || "function_response" in part) {
        toolParts.push(part);
      }
    });

    const visibleTools = prefs.showTools ? toolParts : [];
    if (!textContent && visibleTools.length === 0 && surfaceRefs.length === 0) {
      return;
    }

    const authorLabel = isUser
      ? state.currentUser?.name || state.currentUser?.email || "You"
      : ev.author === APP_NAME
        ? "AdSpace Agent"
        : ev.author || "Agent";

    visibleItems.push({
      isUser,
      authorLabel,
      partial: !!ev.partial,
      textContent,
      visibleTools,
      surfaceRefs,
      usage: isUser ? null : cachedEventUsage(ev),
    });
  });

  if (!visibleItems.length) {
    els.chatTranscript.replaceChildren();
    const empty = document.createElement("div");
    empty.className = "transcript-empty-placeholder";
    style(empty, {
      margin: "auto",
      textAlign: "center",
      color: "var(--text-muted)",
      padding: "32px 18px",
      fontSize: "12.5px",
    });
    empty.textContent =
      "Ask a question on the left or pick a starter card on the right to begin.";
    els.chatTranscript.appendChild(empty);
    return;
  }

  els.chatTranscript
    .querySelectorAll(":scope > .transcript-empty-placeholder")
    .forEach((el) => el.remove());

  // On a session switch, drop the old rows instead of reusing them so the
  // per-message counters do not animate from the previous session's values.
  if (isNewSessionView) els.chatTranscript.replaceChildren();

  const existingRows = Array.from(els.chatTranscript.querySelectorAll(":scope > .message-row"));

  visibleItems.forEach((item, idx) => {
    const roleKey = item.isUser ? "user" : "agent";
    let row = existingRows[idx];

    if (!row || row.dataset.roleKey !== roleKey || row.dataset.authorLabel !== item.authorLabel) {
      const newRow = document.createElement("div");
      newRow.className = `message-row ${item.isUser ? "message-row-user" : "message-row-agent"}`;
      newRow.dataset.roleKey = roleKey;
      newRow.dataset.authorLabel = item.authorLabel;

      const avatar = createAvatarElement(item.isUser, item.authorLabel);
      const col = document.createElement("div");
      col.className = "message-col";
      newRow.append(avatar, col);

      if (row) {
        els.chatTranscript.replaceChild(newRow, row);
      } else {
        els.chatTranscript.appendChild(newRow);
      }
      row = newRow;
    }

    const col = row.querySelector(":scope > .message-col");
    updateMessageCol(col, item);
  });

  while (els.chatTranscript.children.length > visibleItems.length) {
    els.chatTranscript.lastElementChild.remove();
  }

  if (stickToBottom) {
    els.chatTranscript.scrollTop = els.chatTranscript.scrollHeight;
  }
};

const canvasTabIdFor = (surfaceId) => `canvas-tab-${surfaceId}`;
const canvasPanelIdFor = (surfaceId) => `canvas-panel-${surfaceId}`;

const renderCanvas = () => {
  // Rebuilding the tablist destroys the focused tab; remember whether focus
  // was inside so it can be restored on the newly active tab afterwards.
  const hadTabFocus = els.canvasTabs.contains(document.activeElement);
  els.canvasSurfacesEl.replaceChildren();
  els.canvasTabs.replaceChildren();

  if (state.canvasSurfaces.length === 0) {
    els.canvasEmpty.classList.remove("hidden");
    els.canvasTabs.classList.add("hidden");
    return;
  }

  if (prefs.canvasMode === "stacked") {
    state.activeSurfaceId = "all";
  } else if (
    state.activeSurfaceId === "all" ||
    !state.canvasSurfaces.some((s) => s.id === state.activeSurfaceId)
  ) {
    state.activeSurfaceId = state.canvasSurfaces[state.canvasSurfaces.length - 1].id;
  }

  els.canvasEmpty.classList.add("hidden");
  els.canvasTabs.classList.remove("hidden");

  const makeTab = (id, label, isActive, controlsId) => {
    const tab = document.createElement("button");
    tab.type = "button";
    tab.id = canvasTabIdFor(id);
    tab.className = `canvas-tab ${isActive ? "active" : ""}`;
    tab.setAttribute("role", "tab");
    tab.setAttribute("aria-selected", String(isActive));
    tab.setAttribute("aria-controls", controlsId);
    // Roving tabindex: only the selected tab is in the Tab order.
    tab.tabIndex = isActive ? 0 : -1;
    tab.textContent = label;
    return tab;
  };

  const allActive = prefs.canvasMode === "stacked";
  const allBtn = makeTab(
    "all",
    `All Cards (${state.canvasSurfaces.length})`,
    allActive,
    els.canvasSurfacesEl.id || "canvas-surfaces"
  );
  allBtn.addEventListener("click", () => {
    state.activeSurfaceId = "all";
    prefs.canvasMode = "stacked";
    applyPreferences({ rerender: false });
    renderCanvas();
  });
  els.canvasTabs.appendChild(allBtn);

  state.canvasSurfaces.forEach((surf) => {
    const isActive = prefs.canvasMode === "focused" && state.activeSurfaceId === surf.id;
    const tab = makeTab(surf.id, surf.title, isActive, canvasPanelIdFor(surf.id));
    tab.addEventListener("click", () => {
      focusSurfaceOnCanvas(surf.id);
    });
    els.canvasTabs.appendChild(tab);
  });

  const visibleSurfaces =
    prefs.canvasMode === "focused" && state.activeSurfaceId !== "all"
      ? state.canvasSurfaces.filter((s) => s.id === state.activeSurfaceId)
      : state.canvasSurfaces;

  for (const surf of visibleSurfaces) {
    const group = document.createElement("div");
    group.setAttribute("data-surface-id", surf.id);
    group.id = canvasPanelIdFor(surf.id);
    group.setAttribute("role", "tabpanel");
    group.setAttribute("aria-labelledby", canvasTabIdFor(allActive ? "all" : surf.id));
    style(group, { display: "flex", flexDirection: "column", gap: "14px" });
    for (const card of surf.cards) {
      group.appendChild(card);
    }
    els.canvasSurfacesEl.appendChild(group);
  }

  if (hadTabFocus) {
    const selected = els.canvasTabs.querySelector('[role="tab"][aria-selected="true"]');
    if (selected) selected.focus();
  }
};

// Arrow / Home / End move between canvas tabs and activate the target
// (activation re-renders, which re-focuses the selected tab above).
els.canvasTabs.addEventListener("keydown", (e) => {
  const tabs = Array.from(els.canvasTabs.querySelectorAll('[role="tab"]'));
  if (!tabs.length) return;
  const idx = tabs.indexOf(document.activeElement);
  let next;
  if (e.key === "ArrowRight") next = idx < 0 ? 0 : (idx + 1) % tabs.length;
  else if (e.key === "ArrowLeft") next = idx < 0 ? tabs.length - 1 : (idx - 1 + tabs.length) % tabs.length;
  else if (e.key === "Home") next = 0;
  else if (e.key === "End") next = tabs.length - 1;
  else return;
  e.preventDefault();
  tabs[next].focus();
  tabs[next].click();
});

els.exportUiBtn.addEventListener("click", () => {
  if (!state.canvasSurfaces.length) return;
  const payload = state.canvasSurfaces.map((s) => s.raw);
  const blob = new Blob([JSON.stringify(payload, null, 2)], { type: "application/json" });
  const url = URL.createObjectURL(blob);
  const a = document.createElement("a");
  a.href = url;
  a.download = `adspace-ui-${state.activeSession?.id?.slice(0, 8) || "canvas"}.json`;
  a.click();
  // Revoke after the click has been dispatched; revoking synchronously can
  // cancel the download in some browsers.
  setTimeout(() => URL.revokeObjectURL(url), 0);
});

els.clearUiBtn.addEventListener("click", () => {
  state.canvasSurfaces = [];
  renderCanvas();
});

// Starter cards on empty canvas
document.querySelectorAll(".starter-card").forEach((card) => {
  card.addEventListener("click", () => {
    const prompt = card.getAttribute("data-prompt");
    if (prompt && !state.isRunning) sendMessage(prompt);
  });
});

// ---------------------------------------------------------------------------
// Sending Messages & SSE Streaming (`POST /api/run_sse`)
// ---------------------------------------------------------------------------

const renderAttachments = () => {
  els.attachmentChips.replaceChildren();
  if (!state.attachments.length) {
    els.attachmentChips.classList.add("hidden");
    return;
  }
  els.attachmentChips.classList.remove("hidden");
  state.attachments.forEach((file, idx) => {
    const chip = document.createElement("button");
    chip.type = "button";
    const nameSpan = document.createElement("span");
    nameSpan.textContent = file.name;
    const closeB = document.createElement("b");
    closeB.textContent = "×";
    chip.append(nameSpan, " ", closeB);
    chip.addEventListener("click", () => {
      state.attachments.splice(idx, 1);
      renderAttachments();
    });
    els.attachmentChips.appendChild(chip);
  });
};

els.fileInput.addEventListener("change", () => {
  const files = Array.from(els.fileInput.files || []);
  state.attachments.push(...files);
  els.fileInput.value = "";
  renderAttachments();
});

const extractSseChunkData = (chunk) => {
  const dataLines = chunk
    .split("\n")
    .filter((l) => l.startsWith("data:"))
    .map((l) => l.slice(5).trimStart());
  return dataLines.length ? dataLines.join("\n") : null;
};

const parseSseStream = async function* (body) {
  const reader = body.getReader();
  const decoder = new TextDecoder();
  let buffer = "";
  while (true) {
    const { value, done } = await reader.read();
    if (done) {
      buffer += decoder.decode();
      break;
    }
    // Normalise CRLF on the accumulated buffer (not just this chunk) so a
    // `\r\n` split across two TCP reads still forms an event boundary.
    buffer = (buffer + decoder.decode(value, { stream: true })).replace(/\r\n/g, "\n");
    let boundary;
    while ((boundary = buffer.indexOf("\n\n")) !== -1) {
      const chunk = buffer.slice(0, boundary);
      buffer = buffer.slice(boundary + 2);
      const data = extractSseChunkData(chunk);
      if (data) yield data;
    }
  }
  const trailing = extractSseChunkData(buffer.replace(/\r\n/g, "\n"));
  if (trailing) yield trailing;
};

// Aborts the in-flight run, if any. Only the send button toggles between
// send and stop; Enter, starter cards and adk-ui forms never stop a run.
const stopRun = () => {
  if (state.isRunning && state.abortController) state.abortController.abort();
};

// SSE chunks arrive far faster than frames; coalesce the transcript/canvas
// rebuild to at most once per animation frame while streaming. The stream
// end (and error paths) flush or cancel the pending frame explicitly.
let streamRenderRaf = 0;
const flushStreamRender = () => {
  if (streamRenderRaf) {
    cancelAnimationFrame(streamRenderRaf);
    streamRenderRaf = 0;
  }
  rebuildFromSessionEvents(state.currentEvents);
};
const scheduleStreamRender = () => {
  if (streamRenderRaf) return;
  streamRenderRaf = requestAnimationFrame(() => {
    streamRenderRaf = 0;
    rebuildFromSessionEvents(state.currentEvents);
  });
};
const cancelStreamRender = () => {
  if (streamRenderRaf) {
    cancelAnimationFrame(streamRenderRaf);
    streamRenderRaf = 0;
  }
};

// Polite screen-reader announcement via the visually hidden `#sr-announcer`
// live region (null-guarded: the element lives in studio.html). Clearing
// first makes a repeated identical message re-announce.
let announceTimer = null;
const announce = (text) => {
  const el = $("sr-announcer");
  if (!el) return;
  if (announceTimer) clearTimeout(announceTimer);
  el.textContent = "";
  announceTimer = setTimeout(() => {
    announceTimer = null;
    el.textContent = text;
  }, 50);
};

const sendMessage = async (overrideText) => {
  if (state.isRunning) return;
  if (!state.currentUser || !state.activeSession) return;

  const rawText =
    typeof overrideText === "string" ? overrideText.trim() : els.chatInput.value.trim();
  if (!rawText && state.attachments.length === 0) return;

  // Pin the session this run streams for. A session switch mid-stream must
  // not render this run's chunks into (or reload) the newly active session.
  const runSession = state.activeSession;
  const pendingFiles = state.attachments;

  if (typeof overrideText !== "string") {
    els.chatInput.value = "";
    syncComposerHeight();
  }
  state.attachments = [];
  renderAttachments();

  // Flip to the running state synchronously, before any `await`, so a second
  // Enter/click while attachments are still being encoded cannot start a
  // concurrent request.
  state.isRunning = true;
  setStatus("running", "streaming");
  els.sendBtn.classList.add("stop-button");
  if ($("send-icon")) {
    $("send-icon").textContent = "stop";
    $("send-icon").classList.add("filled");
  }
  state.abortController = new AbortController();
  let failed = false;

  try {
    const parts = [];
    if (rawText) parts.push({ text: rawText });

    for (const file of pendingFiles) {
      const bytes = new Uint8Array(await file.arrayBuffer());
      let binary = "";
      for (let i = 0; i < bytes.length; i += 32768) {
        binary += String.fromCharCode(...bytes.subarray(i, i + 32768));
      }
      parts.push({
        inlineData: {
          displayName: file.name,
          data: btoa(binary),
          mimeType: file.type || "application/octet-stream",
        },
      });
    }

    // Encoding large attachments can take a while; if the user switched
    // sessions (or hit stop) meanwhile, bail before the optimistic bubble
    // lands in the wrong session's transcript.
    if (state.abortController.signal.aborted || state.activeSession?.id !== runSession.id) {
      throw new DOMException("Run superseded before send", "AbortError");
    }

    // Optimistically append user message
    const userEvent = {
      id: `local-user-${Date.now()}`,
      author: "user",
      content: { role: "user", parts: [{ text: rawText || "(attachment)" }] },
    };
    state.currentEvents.push(userEvent);
    renderTranscript();
    // The user's own message always scrolls into view.
    if (prefs.autoScroll) els.chatTranscript.scrollTop = els.chatTranscript.scrollHeight;

    const resp = await fetch(apiUrl("/run_sse"), {
      method: "POST",
      signal: state.abortController.signal,
      headers: {
        "Content-Type": "application/json",
        "x-adk-ui-protocol": "a2ui",
      },
      body: JSON.stringify({
        appName: APP_NAME,
        userId: state.currentUser.id,
        sessionId: runSession.id,
        streaming: true,
        newMessage: { role: "user", parts },
      }),
    });

    if (!resp.ok) {
      const errText = await resp.text();
      throw new Error(errText || `HTTP ${resp.status}`);
    }

    if (resp.body) {
      for await (const dataStr of parseSseStream(resp.body)) {
        if (state.abortController.signal.aborted || state.activeSession?.id !== runSession.id) {
          // The user switched sessions; stop streaming into the wrong one.
          state.abortController.abort();
          break;
        }
        if (!dataStr || dataStr === "[DONE]") continue;
        // Only the parse is allowed to fail quietly (a frame may be split
        // mid-JSON); renderer exceptions must surface, not be swallowed.
        let parsed;
        try {
          parsed = JSON.parse(dataStr);
        } catch {
          continue;
        }
        const ev = parsed.event ?? parsed;
        if (ev && typeof ev === "object") {
          if (ev.error_message || ev.errorMessage) {
            showError(`Agent error: ${ev.error_message || ev.errorMessage}`);
          }
          const existingIdx = state.currentEvents.findIndex((e) => e.id && e.id === ev.id);
          if (existingIdx !== -1) {
            state.currentEvents[existingIdx] = mergeStreamedEvent(state.currentEvents[existingIdx], ev);
          } else {
            state.currentEvents.push({
              ...ev,
              content: ev.content
                ? { ...ev.content, parts: coalesceParts(ev.content.parts) }
                : ev.content,
            });
          }
          scheduleStreamRender();
        }
      }
      // Flush whatever the last frame did not get to before reloading.
      flushStreamRender();
    }
    // Reload only if nobody switched away meanwhile. `loadSession` aborts the
    // controller on an explicit switch before `activeSession` changes, so the
    // signal is checked too; otherwise this reload would supersede the
    // user's pending switch.
    if (!state.abortController.signal.aborted && state.activeSession?.id === runSession.id) {
      // Single polite announcement per completed turn (the transcript
      // itself is no longer a live region).
      announce("Agent replied");
      await loadSession(runSession.id);
    }
  } catch (err) {
    if (err.name !== "AbortError") {
      failed = true;
      showError(`Agent request failed: ${err.message || err}`);
    }
  } finally {
    // Paint whatever the last frame did not get to (e.g. after stopRun(),
    // which skips the post-loop flush); drop it if the user switched away.
    if (state.activeSession?.id === runSession.id) {
      flushStreamRender();
    } else {
      cancelStreamRender();
    }
    state.isRunning = false;
    state.abortController = null;
    els.sendBtn.classList.remove("stop-button");
    if ($("send-icon")) {
      $("send-icon").textContent = "arrow_upward";
      $("send-icon").classList.remove("filled");
    }
    // `failed` sticks until the next send flips the pill back to running.
    setStatus(failed ? "failed" : "ready", failed ? "failed" : "ready");
  }
};

const syncComposerHeight = () => {
  els.chatInput.style.height = "36px";
  const next = Math.min(160, Math.max(36, els.chatInput.scrollHeight));
  els.chatInput.style.height = `${next}px`;
};

els.chatInput.addEventListener("input", syncComposerHeight);
els.sendBtn.addEventListener("click", () => (state.isRunning ? stopRun() : sendMessage()));
els.chatInput.addEventListener("keydown", (e) => {
  // Ignore the Enter that confirms an IME composition (CJK etc.); `keyCode
  // 229` is the legacy signal some browsers still send for it.
  if (e.isComposing || e.keyCode === 229) return;
  if (e.key === "Enter" && !e.shiftKey) {
    e.preventDefault();
    if (!state.isRunning) sendMessage();
  }
});

// ---------------------------------------------------------------------------
// Popovers, Settings Controls & Split-Pane Resizer
// ---------------------------------------------------------------------------

bindPopoverTrigger(els.settingsBtn, els.settingsMenu);
bindPopoverTrigger(els.userBtn, els.userMenu);
bindPopoverTrigger(els.sessionUsageBtn, els.sessionUsagePopover);

els.settingsBtn.addEventListener("click", (e) => {
  e.stopPropagation();
  closePopover(els.userBtn, els.userMenu);
  closePopover(els.sessionUsageBtn, els.sessionUsagePopover);
  if (isPopoverOpen(els.settingsMenu)) {
    closePopover(els.settingsBtn, els.settingsMenu);
  } else {
    openPopover(els.settingsBtn, els.settingsMenu);
  }
});

els.userBtn.addEventListener("click", (e) => {
  e.stopPropagation();
  closePopover(els.settingsBtn, els.settingsMenu);
  closePopover(els.sessionUsageBtn, els.sessionUsagePopover);
  if (isPopoverOpen(els.userMenu)) {
    closePopover(els.userBtn, els.userMenu);
  } else {
    openPopover(els.userBtn, els.userMenu);
  }
});

if (els.sessionUsageBtn && els.sessionUsagePopover) {
  els.sessionUsageBtn.addEventListener("click", (e) => {
    e.stopPropagation();
    closePopover(els.settingsBtn, els.settingsMenu);
    closePopover(els.userBtn, els.userMenu);
    updateSessionUsageUi();
    if (isPopoverOpen(els.sessionUsagePopover)) {
      closePopover(els.sessionUsageBtn, els.sessionUsagePopover);
    } else {
      openPopover(els.sessionUsageBtn, els.sessionUsagePopover);
    }
  });
}

document.addEventListener("click", (e) => {
  if (!els.settingsMenu.contains(e.target) && e.target !== els.settingsBtn) {
    closePopover(els.settingsBtn, els.settingsMenu);
  }
  if (!els.userMenu.contains(e.target) && e.target !== els.userBtn) {
    closePopover(els.userBtn, els.userMenu);
  }
  if (
    els.sessionUsagePopover &&
    els.sessionUsageBtn &&
    !els.sessionUsagePopover.contains(e.target) &&
    !els.sessionUsageBtn.contains(e.target)
  ) {
    closePopover(els.sessionUsageBtn, els.sessionUsagePopover);
  }
});

document.addEventListener("keydown", (e) => {
  if (e.key !== "Escape") return;
  const pairs = [
    [els.settingsBtn, els.settingsMenu],
    [els.userBtn, els.userMenu],
    [els.sessionUsageBtn, els.sessionUsagePopover],
  ];
  for (const [trigger, popover] of pairs) {
    if (isPopoverOpen(popover)) {
      closePopover(trigger, popover);
      e.preventDefault();
    }
  }
});

els.errorBannerClose.addEventListener("click", () => showError(null));

document.querySelectorAll("#theme-segmented button").forEach((btn) => {
  btn.addEventListener("click", () => {
    prefs.theme = btn.dataset.themeChoice;
    applyPreferences();
  });
});

document.querySelectorAll("#canvas-mode-segmented button").forEach((btn) => {
  btn.addEventListener("click", () => {
    prefs.canvasMode = btn.dataset.canvasMode;
    applyPreferences();
  });
});

$("setting-show-tools").addEventListener("change", (e) => {
  prefs.showTools = e.target.checked;
  applyPreferences();
});

if ($("setting-show-session-usage")) {
  $("setting-show-session-usage").addEventListener("change", (e) => {
    prefs.showSessionUsage = e.target.checked;
    applyPreferences({ rerender: false });
  });
}

if ($("setting-show-message-usage")) {
  $("setting-show-message-usage").addEventListener("change", (e) => {
    prefs.showMessageUsage = e.target.checked;
    applyPreferences();
  });
}

if ($("setting-show-cost")) {
  $("setting-show-cost").addEventListener("change", (e) => {
    prefs.showCost = e.target.checked;
    applyPreferences();
  });
}

$("setting-autoscroll").addEventListener("change", (e) => {
  prefs.autoScroll = e.target.checked;
  applyPreferences();
});

const bindModelPicker = (prefKey, selectId, customId, defaultVal) => {
  const selectEl = $(selectId);
  const customEl = $(customId);
  if (!selectEl || !customEl) return;

  selectEl.addEventListener("change", () => {
    if (selectEl.value === "__custom__") {
      customEl.classList.remove("hidden");
      customEl.value = prefs[prefKey] || defaultVal;
      customEl.focus();
      customEl.select();
    } else {
      customEl.classList.add("hidden");
      prefs[prefKey] = selectEl.value;
      applyPreferences({ rerender: prefKey === "model" });
      pushModelsToServer();
    }
  });

  const commitCustom = () => {
    const cleaned = customEl.value.trim();
    if (!cleaned) {
      customEl.value = prefs[prefKey] || defaultVal;
      return;
    }
    if (cleaned !== prefs[prefKey]) {
      prefs[prefKey] = cleaned;
      applyPreferences({ rerender: prefKey === "model", syncModels: false });
      pushModelsToServer();
    }
  };

  customEl.addEventListener("change", commitCustom);
  customEl.addEventListener("blur", commitCustom);
  customEl.addEventListener("keydown", (e) => {
    if (e.key === "Enter") {
      e.preventDefault();
      commitCustom();
      customEl.blur();
    }
  });
};

bindModelPicker("model", "setting-model-select", "setting-model-custom", DEFAULT_MODELS.model);
bindModelPicker(
  "imageModel",
  "setting-image-model-select",
  "setting-image-model-custom",
  DEFAULT_MODELS.imageModel
);
bindModelPicker(
  "videoModel",
  "setting-video-model-select",
  "setting-video-model-custom",
  DEFAULT_MODELS.videoModel
);

// Model Pricing Controls (Paid Tier Standard / Priority / Batch & Custom Overrides)
if ($("setting-pricing-tier")) {
  $("setting-pricing-tier").addEventListener("change", (e) => {
    prefs.pricingTier = e.target.value || "standard";
    applyPreferences();
  });
}

const commitCustomPricingInputs = ({ rerender = true } = {}) => {
  const activeModel = prefs.model || DEFAULT_MODELS.model;
  const tier = prefs.pricingTier || "standard";
  const defaults = getDefaultModelPricing(activeModel, tier);
  const key = getModelPricingKey(activeModel, tier);

  const parseRate = (el, fallback) => {
    if (!el) return fallback;
    const num = parseFloat(el.value);
    return Number.isFinite(num) && num >= 0 ? num : fallback;
  };

  const nextInput = parseRate($("setting-price-input"), defaults.input);
  const nextCached = parseRate($("setting-price-cached"), defaults.cached);
  const nextOutput = parseRate($("setting-price-output"), defaults.output);

  if (!prefs.modelPricing || typeof prefs.modelPricing !== "object") {
    prefs.modelPricing = {};
  }

  const isDefault =
    Math.abs(nextInput - defaults.input) < 1e-9 &&
    Math.abs(nextCached - defaults.cached) < 1e-9 &&
    Math.abs(nextOutput - defaults.output) < 1e-9;

  if (isDefault) {
    delete prefs.modelPricing[key];
  } else {
    prefs.modelPricing[key] = {
      input: nextInput,
      cached: nextCached,
      output: nextOutput,
    };
  }

  applyPreferences({ rerender, syncPricing: false });
};

["setting-price-input", "setting-price-cached", "setting-price-output"].forEach((id) => {
  const el = $(id);
  if (!el) return;
  // Keystrokes only persist + refresh the session totals; the full
  // transcript/canvas rerender waits for `change` (blur / Enter).
  el.addEventListener("input", () => commitCustomPricingInputs({ rerender: false }));
  el.addEventListener("change", () => {
    commitCustomPricingInputs({ rerender: true });
    syncPricingControls();
  });
});

if ($("setting-pricing-reset")) {
  $("setting-pricing-reset").addEventListener("click", () => {
    const activeModel = prefs.model || DEFAULT_MODELS.model;
    const tier = prefs.pricingTier || "standard";
    if (prefs.modelPricing && typeof prefs.modelPricing === "object") {
      delete prefs.modelPricing[getModelPricingKey(activeModel, tier)];
    }
    applyPreferences();
  });
}

// Split-Pane Resizer (pointer + keyboard; persisted once per gesture)
const LEFT_PANE_MIN_PCT = 28;
const LEFT_PANE_MAX_PCT = 68;
const clampLeftPanePct = (pct) =>
  Math.min(LEFT_PANE_MAX_PCT, Math.max(LEFT_PANE_MIN_PCT, pct));
const readLeftPanePct = () => {
  const raw = getComputedStyle(document.documentElement).getPropertyValue("--left-pane-width");
  const n = parseFloat(raw);
  return Number.isFinite(n) ? clampLeftPanePct(n) : 44;
};
let leftPanePct = readLeftPanePct();
const currentLeftPanePct = () => leftPanePct;
const setLeftPanePct = (pct, { persist = false } = {}) => {
  const clamped = clampLeftPanePct(pct);
  leftPanePct = clamped;
  const val = `${clamped.toFixed(1)}%`;
  document.documentElement.style.setProperty("--left-pane-width", val);
  if (els.paneResizer) els.paneResizer.setAttribute("aria-valuenow", String(Math.round(clamped)));
  if (persist) localStorage.setItem(STORAGE_KEYS.leftWidth, val);
  return clamped;
};
if (els.paneResizer) {
  els.paneResizer.setAttribute("aria-valuemin", String(LEFT_PANE_MIN_PCT));
  els.paneResizer.setAttribute("aria-valuemax", String(LEFT_PANE_MAX_PCT));
  els.paneResizer.setAttribute("aria-valuenow", String(Math.round(currentLeftPanePct())));

  let resizePointerId = null;
  els.paneResizer.addEventListener("pointerdown", (e) => {
    if (e.button !== 0 && e.pointerType === "mouse") return;
    e.preventDefault();
    resizePointerId = e.pointerId;
    els.paneResizer.classList.add("dragging");
    try {
      els.paneResizer.setPointerCapture(e.pointerId);
    } catch {
      /* capture is best-effort (e.g. pointer already released) */
    }
  });
  els.paneResizer.addEventListener("pointermove", (e) => {
    if (resizePointerId === null || e.pointerId !== resizePointerId) return;
    setLeftPanePct((e.clientX / window.innerWidth) * 100);
  });
  const endResize = (e) => {
    if (resizePointerId === null || e.pointerId !== resizePointerId) return;
    resizePointerId = null;
    els.paneResizer.classList.remove("dragging");
    try {
      els.paneResizer.releasePointerCapture(e.pointerId);
    } catch {
      /* already released */
    }
    setLeftPanePct(currentLeftPanePct(), { persist: true });
  };
  els.paneResizer.addEventListener("pointerup", endResize);
  els.paneResizer.addEventListener("pointercancel", endResize);

  els.paneResizer.addEventListener("keydown", (e) => {
    let delta;
    if (e.key === "ArrowLeft") delta = -(e.shiftKey ? 10 : 2);
    else if (e.key === "ArrowRight") delta = e.shiftKey ? 10 : 2;
    else if (e.key === "Home") delta = LEFT_PANE_MIN_PCT - currentLeftPanePct();
    else if (e.key === "End") delta = LEFT_PANE_MAX_PCT - currentLeftPanePct();
    else return;
    e.preventDefault();
    setLeftPanePct(currentLeftPanePct() + delta, { persist: true });
  });
}

// ---------------------------------------------------------------------------
// Interactive Canvas Stage (Argon Dotted Grid WebGL Stage)
// ---------------------------------------------------------------------------
let authStageInstance = null;
let canvasStageInstance = null;
let stageAbort = null;

const STAGE_CONFIG = {
  dark_mode_preset: "argon-dark",
  light_mode_preset: "argon-light",
  grid_step: 24,
  dot_radius: 1.4,
  hover_reach: 155,
  hover_push: 7.5,
  hover_swirl: 2.2,
  hover_scale: 1.35,
};

const getEffectiveColorMode = () => {
  if (prefs.theme === "dark") return "dark";
  if (prefs.theme === "light") return "light";
  return window.matchMedia?.("(prefers-color-scheme: dark)")?.matches ? "dark" : "light";
};

function syncCanvasStageTheme() {
  const colorMode = getEffectiveColorMode();
  if (authStageInstance && typeof authStageInstance.update === "function") {
    authStageInstance.update({ colorMode });
  }
  if (canvasStageInstance && typeof canvasStageInstance.update === "function") {
    canvasStageInstance.update({ colorMode });
  }
}

// Wave on the canvas stage while the agent is busy: blue while streaming,
// amber while waiting for Google authorization (matching the status pill).
// Driven off the status pill so every path that flips it (send, stop,
// errors, popup consent, reconnect after login) keeps the stage in sync;
// re-applied when the stage is (re)mounted since mounting may finish after
// a run started.
const STAGE_ACTIVITY_BY_STATUS = { running: "streaming", authorizing: "authorizing" };
const currentStatusKind = () => {
  const match = /\bstatus-([a-z]+)/.exec(els.statusPill.className || "");
  return match ? match[1] : "ready";
};
function syncStageStreaming(kind = currentStatusKind()) {
  const activity = STAGE_ACTIVITY_BY_STATUS[kind] || "idle";
  if (canvasStageInstance && typeof canvasStageInstance.update === "function") {
    if (canvasStageInstance.activity !== activity) {
      canvasStageInstance.update({ activity });
    }
  }
}
onStatusChange(syncStageStreaming);

function syncActiveStage() {
  const authVisible = els.authGate && !els.authGate.classList.contains("hidden");
  const workspaceVisible = els.workspace && !els.workspace.classList.contains("hidden");
  const allowMotion = !prefersReducedMotion();

  if (authStageInstance) {
    if (authVisible && allowMotion && typeof authStageInstance.resume === "function") {
      authStageInstance.resume();
    } else if (typeof authStageInstance.pause === "function") {
      authStageInstance.pause();
    }
  }
  if (canvasStageInstance) {
    if (workspaceVisible && allowMotion && typeof canvasStageInstance.resume === "function") {
      canvasStageInstance.resume();
    } else if (typeof canvasStageInstance.pause === "function") {
      canvasStageInstance.pause();
    }
  }
}

const mountStageOn = async (stageEl, signal) => {
  const stageMod = AdSpaceStage;
  if (!stageEl || !stageMod || typeof stageMod.mount !== "function") return null;
  try {
    return await stageMod.mount({
      root: stageEl,
      colorMode: getEffectiveColorMode(),
      signal,
      config: STAGE_CONFIG,
    });
  } catch {
    /* fallback silently to CSS background if WebGL2 is unavailable */
    return null;
  }
};

const initCanvasStage = async () => {
  if (stageAbort) stageAbort.abort();
  const ctrl = new AbortController();
  stageAbort = ctrl;

  const [authInst, canvasInst] = await Promise.all([
    mountStageOn($("auth-stage"), ctrl.signal),
    mountStageOn($("canvas-stage"), ctrl.signal),
  ]);
  if (stageAbort !== ctrl) return;
  authStageInstance = authInst;
  canvasStageInstance = canvasInst;
  syncActiveStage();
  syncStageStreaming();
};

if (window.matchMedia) {
  const darkMq = window.matchMedia("(prefers-color-scheme: dark)");
  if (typeof darkMq.addEventListener === "function") {
    darkMq.addEventListener("change", () => {
      if (prefs.theme === "system") syncCanvasStageTheme();
    });
  }
  // Pause/resume the stages live when the OS reduced-motion setting flips;
  // `syncActiveStage` re-reads `prefersReducedMotion()`.
  const motionMq = window.matchMedia("(prefers-reduced-motion: reduce)");
  if (typeof motionMq.addEventListener === "function") {
    motionMq.addEventListener("change", () => syncActiveStage());
  }
}

// Initialize
applyPreferences();
syncSettingsAvailability();
initCanvasStage();
checkAuthStatus();
