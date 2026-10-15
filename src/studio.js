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
(() => {
  "use strict";

  const APP_NAME = "adspace_agent";
  const CHANNEL = "adspace:oauth";
  const EVENTS_PATH = "/oauth2/events";
  const ACK_PATH = "/oauth2/ack";
  const SVG_NS = "http://www.w3.org/2000/svg";

  const STORAGE_KEYS = {
    theme: "adspace.studio.theme",
    canvasMode: "adspace.studio.canvasMode",
    showTools: "adspace.studio.showTools",
    showSessionUsage: "adspace.studio.showSessionUsage",
    showMessageUsage: "adspace.studio.showMessageUsage",
    showCost: "adspace.studio.showCost",
    autoScroll: "adspace.studio.autoScroll",
    customerId: "adspace.studio.customerId",
    loginCustomerId: "adspace.studio.loginCustomerId",
    leftWidth: "adspace.studio.leftWidth",
    model: "adspace.studio.model",
    imageModel: "adspace.studio.imageModel",
    videoModel: "adspace.studio.videoModel",
    pricingTier: "adspace.studio.pricingTier",
    modelPricing: "adspace.studio.modelPricing",
  };

  const DEFAULT_MODELS = {
    model: "gemini-3.8-flash",
    imageModel: "gemini-3.1-flash-image",
    videoModel: "veo-3.1-fast-generate-001",
  };

  const PRESET_MODEL_OPTIONS = {
    model: ["gemini-3.8-flash", "gemini-3.5-flash-lite", "gemini-4-argon"],
    imageModel: [
      "gemini-3.1-flash-image",
      "imagen-4.0-generate-001",
      "imagen-4.0-fast-generate-001",
    ],
    videoModel: [
      "veo-3.1-fast-generate-001",
      "veo-3.1-generate-001",
      "veo-3.0-generate-001",
    ],
  };

  const PALETTE = [
    "#4b90ff",
    "#6b7cff",
    "#9b72cb",
    "#d96570",
    "#38bdf8",
    "#34d399",
    "#fbbf24",
    "#f472b6",
  ];

  // Application State
  let currentUser = null;
  let sessions = [];
  let activeSession = null;
  let isRunning = false;
  let abortController = null;
  let attachments = [];
  let canvasSurfaces = []; // [{ id, title, eyebrow, raw, cards }]
  let activeSurfaceId = "all";
  let popup = null;
  const handledStates = new Set();

  const loadStoredModelPricing = () => {
    try {
      const raw = localStorage.getItem(STORAGE_KEYS.modelPricing);
      if (!raw) return {};
      const parsed = JSON.parse(raw);
      return parsed && typeof parsed === "object" ? parsed : {};
    } catch {
      return {};
    }
  };

  // Preferences
  let prefs = {
    theme: localStorage.getItem(STORAGE_KEYS.theme) || "system",
    canvasMode: localStorage.getItem(STORAGE_KEYS.canvasMode) || "stacked",
    showTools: localStorage.getItem(STORAGE_KEYS.showTools) !== "false",
    showSessionUsage: localStorage.getItem(STORAGE_KEYS.showSessionUsage) !== "false",
    showMessageUsage: localStorage.getItem(STORAGE_KEYS.showMessageUsage) !== "false",
    showCost: localStorage.getItem(STORAGE_KEYS.showCost) !== "false",
    autoScroll: localStorage.getItem(STORAGE_KEYS.autoScroll) === "true",
    customerId: localStorage.getItem(STORAGE_KEYS.customerId) || "",
    loginCustomerId: localStorage.getItem(STORAGE_KEYS.loginCustomerId) || "",
    model: localStorage.getItem(STORAGE_KEYS.model) || DEFAULT_MODELS.model,
    imageModel: localStorage.getItem(STORAGE_KEYS.imageModel) || DEFAULT_MODELS.imageModel,
    videoModel: localStorage.getItem(STORAGE_KEYS.videoModel) || DEFAULT_MODELS.videoModel,
    pricingTier: localStorage.getItem(STORAGE_KEYS.pricingTier) || "standard",
    modelPricing: loadStoredModelPricing(),
  };

  // DOM Elements
  const $ = (id) => document.getElementById(id);
  const statusPill = $("status-pill");
  const statusText = $("status-text");
  const sessionControls = $("session-controls");
  const sessionSelect = $("session-select");
  const newSessionBtn = $("new-session-btn");
  const settingsBtn = $("settings-btn");
  const settingsMenu = $("settings-menu");
  const userMenuAnchor = $("user-menu-anchor");
  const userBtn = $("user-btn");
  const userMenu = $("user-menu");
  const logoutBtn = $("logout-btn");
  const deleteSessionBtn = $("delete-session-btn");
  const errorBanner = $("error-banner");
  const errorBannerText = $("error-banner-text");
  const errorBannerClose = $("error-banner-close");
  const authGate = $("auth-gate");
  const authGateHint = $("auth-gate-hint");
  const googleLoginBtn = $("google-login-btn");
  const workspace = $("workspace");
  const chatSessionTitle = $("chat-session-title");
  const sessionUsageBtn = $("session-usage-btn");
  const sessionUsagePopover = $("session-usage-popover");
  const refreshSessionBtn = $("refresh-session-btn");
  const chatTranscript = $("chat-transcript");
  const attachmentChips = $("attachment-chips");
  const fileInput = $("file-input");
  const chatInput = $("chat-input");
  const sendBtn = $("send-btn");
  const composerContextHint = $("composer-context-hint");
  const composerUserHint = $("composer-user-hint");
  const paneResizer = $("pane-resizer");
  const canvasCountBadge = $("canvas-count-badge");
  const canvasTabs = $("canvas-tabs");
  const canvasEmpty = $("canvas-empty");
  const canvasSurfacesEl = $("canvas-surfaces");
  const exportUiBtn = $("export-ui-btn");
  const clearUiBtn = $("clear-ui-btn");

  const style = (el, props) => {
    for (const [k, v] of Object.entries(props)) el.style[k] = v;
  };

  const svgEl = (tag, attrs = {}) => {
    const el = document.createElementNS(SVG_NS, tag);
    for (const [k, v] of Object.entries(attrs)) {
      if (v !== undefined && v !== null) el.setAttribute(k, String(v));
    }
    if (tag === "text") el.setAttribute("stroke", "none");
    return el;
  };

  const showError = (msg) => {
    if (!msg) {
      errorBanner.classList.add("hidden");
      return;
    }
    errorBannerText.textContent = msg;
    errorBanner.classList.remove("hidden");
  };

  const setStatus = (kind, label) => {
    statusPill.className = `status status-${kind}`;
    statusText.textContent = label || kind;
  };

  const initialsOf = (nameOrEmail) => {
    const str = String(nameOrEmail || "U").trim();
    const parts = str.split(/\s+/);
    if (parts.length >= 2) {
      return `${parts[0][0]}${parts[parts.length - 1][0]}`.toUpperCase();
    }
    return str.slice(0, 2).toUpperCase();
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

  const syncAllModelControls = () => {
    syncModelControl(
      $("setting-model-select"),
      $("setting-model-custom"),
      PRESET_MODEL_OPTIONS.model,
      prefs.model
    );
    syncModelControl(
      $("setting-image-model-select"),
      $("setting-image-model-custom"),
      PRESET_MODEL_OPTIONS.imageModel,
      prefs.imageModel
    );
    syncModelControl(
      $("setting-video-model-select"),
      $("setting-video-model-custom"),
      PRESET_MODEL_OPTIONS.videoModel,
      prefs.videoModel
    );
  };

  const pushModelsToServer = async () => {
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
      }
    } catch {
      /* ignore offline / unauthenticated state */
    }
  };

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
    localStorage.setItem(STORAGE_KEYS.theme, prefs.theme);
    localStorage.setItem(STORAGE_KEYS.canvasMode, prefs.canvasMode);
    localStorage.setItem(STORAGE_KEYS.showTools, String(prefs.showTools));
    localStorage.setItem(STORAGE_KEYS.showSessionUsage, String(prefs.showSessionUsage));
    localStorage.setItem(STORAGE_KEYS.showMessageUsage, String(prefs.showMessageUsage));
    localStorage.setItem(STORAGE_KEYS.showCost, String(prefs.showCost));
    localStorage.setItem(STORAGE_KEYS.autoScroll, String(prefs.autoScroll));
    localStorage.setItem(STORAGE_KEYS.customerId, prefs.customerId);
    localStorage.setItem(STORAGE_KEYS.loginCustomerId, prefs.loginCustomerId);
    localStorage.setItem(STORAGE_KEYS.model, prefs.model);
    localStorage.setItem(STORAGE_KEYS.imageModel, prefs.imageModel);
    localStorage.setItem(STORAGE_KEYS.videoModel, prefs.videoModel);
    localStorage.setItem(STORAGE_KEYS.pricingTier, prefs.pricingTier || "standard");
    localStorage.setItem(STORAGE_KEYS.modelPricing, JSON.stringify(prefs.modelPricing || {}));

    document.querySelectorAll("#theme-segmented button").forEach((btn) => {
      btn.classList.toggle("active", btn.dataset.themeChoice === prefs.theme);
    });
    document.querySelectorAll("#canvas-mode-segmented button").forEach((btn) => {
      btn.classList.toggle("active", btn.dataset.canvasMode === prefs.canvasMode);
    });
    $("setting-show-tools").checked = prefs.showTools;
    if ($("setting-show-session-usage")) {
      $("setting-show-session-usage").checked = prefs.showSessionUsage;
    }
    if ($("setting-show-message-usage")) {
      $("setting-show-message-usage").checked = prefs.showMessageUsage;
    }
    if ($("setting-show-cost")) {
      $("setting-show-cost").checked = prefs.showCost;
    }
    $("setting-autoscroll").checked = prefs.autoScroll;
    $("setting-customer-id").value = prefs.customerId;
    $("setting-login-customer-id").value = prefs.loginCustomerId;
    if (syncModels) {
      syncAllModelControls();
    }
    if (syncPricing && typeof syncPricingControls === "function") {
      syncPricingControls();
    }

    const ids = [prefs.model];
    if (prefs.customerId) ids.push(`CID: ${prefs.customerId}`);
    if (prefs.loginCustomerId) ids.push(`MCC: ${prefs.loginCustomerId}`);
    composerContextHint.textContent = `${ids.join(" · ")} · Enter to send`;

    if (rerender) {
      renderTranscript();
      renderCanvas();
    } else if (typeof updateSessionUsageUi === "function") {
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
      popup = win;
      try {
        win.focus();
      } catch {
        /* ignore */
      }
    }
    return win;
  };

  const handleAuthRequired = async (data) => {
    if (!data || !data.auth_url || !data.state) return;
    if (!currentUser || !activeSession) return;
    if (data.session_id && data.session_id !== activeSession.id) return;
    if (handledStates.has(data.state)) return;
    handledStates.add(data.state);

    setStatus("authorizing", "authorizing…");
    const win = openPopup(data.auth_url);
    const opened = !!win;
    try {
      await fetch(ACK_PATH, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ state: data.state, opened }),
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

    if (popup && !popup.closed) {
      try {
        popup.close();
      } catch {
        /* ignore */
      }
    }
    popup = null;

    if (status === "login_ok") {
      await checkAuthStatus();
      return;
    }
    if (status === "ok") {
      setStatus(isRunning ? "running" : "ready", isRunning ? "streaming" : "ready");
      if (activeSession && !isRunning) {
        await loadSession(activeSession.id);
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

  const checkAuthStatus = () => {
    if (authCheckPromise) return authCheckPromise;
    authCheckPromise = (async () => {
      try {
        const resp = await fetch("/auth/me");
        const data = await resp.json();
        if (!data.authenticated || !data.user) {
          currentUser = null;
          authGate.classList.remove("hidden");
          workspace.classList.add("hidden");
          sessionControls.classList.add("hidden");
          userMenuAnchor.classList.add("hidden");
          authGateHint.textContent = data.oauth_configured
            ? "Uses your configured Google OAuth 2.0 Web Client."
            : "Configure CLIENT_ID and CLIENT_SECRET in .env to enable Google Sign-In.";
          return;
        }

        currentUser = data.user;
        authGate.classList.add("hidden");
        workspace.classList.remove("hidden");
        sessionControls.classList.remove("hidden");
        userMenuAnchor.classList.remove("hidden");

        $("user-name").textContent = currentUser.name || currentUser.email;
        $("menu-user-name").textContent = currentUser.name || "Google User";
        $("menu-user-email").textContent = currentUser.email;
        $("menu-user-id").textContent = currentUser.id;
        composerUserHint.textContent = currentUser.email;

        setAvatarImage($("user-avatar-img"), $("user-avatar-fallback"), currentUser);
        setAvatarImage($("menu-avatar-img"), $("menu-avatar-fallback"), currentUser);

        if (data.models) {
          if (!localStorage.getItem(STORAGE_KEYS.model) && data.models.model) {
            prefs.model = data.models.model;
          }
          if (!localStorage.getItem(STORAGE_KEYS.imageModel) && data.models.image_model) {
            prefs.imageModel = data.models.image_model;
          }
          if (!localStorage.getItem(STORAGE_KEYS.videoModel) && data.models.video_model) {
            prefs.videoModel = data.models.video_model;
          }
          applyPreferences({ rerender: false });
          if (
            data.models.model !== prefs.model ||
            data.models.image_model !== prefs.imageModel ||
            data.models.video_model !== prefs.videoModel
          ) {
            await pushModelsToServer();
          }
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

  googleLoginBtn.addEventListener("click", () => {
    const win = openPopup("/auth/login", "adspace_google_login");
    if (!win) {
      window.location.href = "/auth/login";
    }
  });

  logoutBtn.addEventListener("click", async () => {
    userMenu.classList.add("hidden");
    await fetch("/auth/logout", { method: "POST" });
    activeSession = null;
    sessions = [];
    canvasSurfaces = [];
    await checkAuthStatus();
  });

  // ---------------------------------------------------------------------------
  // ADK Session Management (Scoped to `currentUser.id`)
  // ---------------------------------------------------------------------------

  const apiUrl = (path) => `/api${path}`;

  const loadUserSessions = async () => {
    if (!currentUser) return;
    try {
      const resp = await fetch(
        apiUrl(`/apps/${encodeURIComponent(APP_NAME)}/users/${encodeURIComponent(currentUser.id)}/sessions`)
      );
      if (!resp.ok) throw new Error(`HTTP ${resp.status}`);
      const list = await resp.json();
      sessions = (Array.isArray(list) ? list : []).sort(
        (a, b) => (b.lastUpdateTime || 0) - (a.lastUpdateTime || 0)
      );

      if (sessions.length === 0) {
        await createNewSession();
      } else {
        const targetId =
          activeSession && sessions.some((s) => s.id === activeSession.id)
            ? activeSession.id
            : sessions[0].id;
        renderSessionOptions(targetId);
        await loadSession(targetId);
      }
    } catch (err) {
      showError(`Failed to load sessions: ${err.message || err}`);
    }
  };

  const renderSessionOptions = (selectedId) => {
    sessionSelect.replaceChildren();
    sessions.forEach((s, idx) => {
      const opt = document.createElement("option");
      opt.value = s.id;
      const shortId = s.id.slice(0, 8);
      opt.textContent = idx === 0 ? `Latest (${shortId})` : `Session ${shortId}`;
      if (s.id === selectedId) opt.selected = true;
      sessionSelect.appendChild(opt);
    });
  };

  let createSessionPromise = null;

  const createNewSession = () => {
    if (!currentUser) return Promise.resolve();
    if (createSessionPromise) return createSessionPromise;
    createSessionPromise = (async () => {
      const url = apiUrl(
        `/apps/${encodeURIComponent(APP_NAME)}/users/${encodeURIComponent(currentUser.id)}/sessions`
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
        sessions = [created, ...sessions.filter((s) => s.id !== created.id)];
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

  const loadSession = async (sessionId) => {
    if (!currentUser || !sessionId) return;
    const isSessionSwitch = !activeSession || activeSession.id !== sessionId;
    try {
      const resp = await fetch(
        apiUrl(
          `/apps/${encodeURIComponent(APP_NAME)}/users/${encodeURIComponent(
            currentUser.id
          )}/sessions/${encodeURIComponent(sessionId)}`
        )
      );
      if (!resp.ok) throw new Error(`HTTP ${resp.status}`);
      activeSession = await resp.json();
      chatSessionTitle.textContent = `Session ${activeSession.id.slice(0, 8)}`;
      $("menu-session-id").textContent = activeSession.id;
      if (isSessionSwitch) {
        activeSurfaceId = "all";
      }
      rebuildFromSessionEvents(activeSession.events || [], { forceCanvas: isSessionSwitch });
    } catch (err) {
      showError(`Could not load session: ${err.message || err}`);
    }
  };

  sessionSelect.addEventListener("change", () => {
    if (sessionSelect.value) loadSession(sessionSelect.value);
  });

  newSessionBtn.addEventListener("click", () => {
    createNewSession();
  });

  refreshSessionBtn.addEventListener("click", () => {
    if (activeSession) loadSession(activeSession.id);
  });

  deleteSessionBtn.addEventListener("click", async () => {
    userMenu.classList.add("hidden");
    if (!currentUser || !activeSession) return;
    try {
      await fetch(
        apiUrl(
          `/apps/${encodeURIComponent(APP_NAME)}/users/${encodeURIComponent(
            currentUser.id
          )}/sessions/${encodeURIComponent(activeSession.id)}`
        ),
        { method: "DELETE" }
      );
      activeSession = null;
      await loadUserSessions();
    } catch {
      await createNewSession();
    }
  });

  // ---------------------------------------------------------------------------
  // `adk-ui` (`UiResponse`) Interactive Component Renderer (Right-Hand Canvas)
  // ---------------------------------------------------------------------------

  const toNumber = (val) => {
    if (typeof val === "number" && Number.isFinite(val)) return val;
    if (typeof val === "string") {
      const cleaned = val.replace(/[$,%\s,]/g, "");
      const n = Number(cleaned);
      if (cleaned !== "" && Number.isFinite(n)) return n;
    }
    return NaN;
  };

  const formatCompact = (n) => {
    if (!Number.isFinite(n)) return String(n ?? "");
    const abs = Math.abs(n);
    if (abs >= 1e9) return `${(n / 1e9).toFixed(1).replace(/\.0$/, "")}B`;
    if (abs >= 1e6) return `${(n / 1e6).toFixed(1).replace(/\.0$/, "")}M`;
    if (abs >= 1e3) return `${(n / 1e3).toFixed(1).replace(/\.0$/, "")}K`;
    if (Number.isInteger(n)) return String(n);
    return n.toFixed(2).replace(/\.?0+$/, "");
  };

  const formatCellValue = (val) => {
    if (val === null || val === undefined) return "—";
    if (typeof val === "number") {
      return Number.isInteger(val)
        ? val.toLocaleString()
        : val.toLocaleString(undefined, { maximumFractionDigits: 2 });
    }
    if (typeof val === "boolean") return val ? "Yes" : "No";
    if (typeof val === "object") return JSON.stringify(val);
    return String(val);
  };

  const variantColors = (variant) => {
    switch (String(variant || "default").toLowerCase()) {
      case "success":
      case "operational":
      case "ok":
        return {
          bg: "color-mix(in srgb, var(--accent-success) 14%, var(--bg-primary))",
          fg: "var(--accent-success)",
          border: "color-mix(in srgb, var(--accent-success) 42%, var(--border-default))",
        };
      case "warning":
      case "degraded":
        return {
          bg: "color-mix(in srgb, var(--accent-warning) 14%, var(--bg-primary))",
          fg: "var(--accent-warning)",
          border: "color-mix(in srgb, var(--accent-warning) 45%, var(--border-default))",
        };
      case "error":
      case "danger":
      case "down":
      case "outage":
        return {
          bg: "color-mix(in srgb, var(--accent-error) 14%, var(--bg-primary))",
          fg: "var(--accent-error)",
          border: "color-mix(in srgb, var(--accent-error) 42%, var(--border-default))",
        };
      case "info":
      case "primary":
        return {
          bg: "var(--accent-wash)",
          fg: "var(--accent-primary)",
          border: "color-mix(in srgb, var(--accent-primary) 40%, var(--border-default))",
        };
      default:
        return {
          bg: "var(--bg-primary)",
          fg: "var(--text-secondary)",
          border: "var(--border-default)",
        };
    }
  };

  const createStandaloneCard = ({ title, description, eyebrow, headerRight, flushBody = false }) => {
    const card = document.createElement("section");
    card.className = "adk-ui-surface adk-ui-card";
    card.setAttribute("data-ui-scope", "true");
    style(card, {
      width: "100%",
      border: "1px solid var(--border-default)",
      borderRadius: "16px",
      background: "var(--surface-card)",
      boxShadow: "var(--shadow-sm)",
      overflow: "hidden",
      display: "flex",
      flexDirection: "column",
    });

    if (title || description || eyebrow || headerRight) {
      const hdr = document.createElement("header");
      style(hdr, {
        display: "flex",
        alignItems: "center",
        justifyContent: "space-between",
        gap: "10px",
        padding: "12px 16px",
        borderBottom: "1px solid var(--border-default)",
        background: "color-mix(in srgb, var(--bg-primary) 45%, var(--surface-card))",
      });

      const left = document.createElement("div");
      style(left, {
        display: "flex",
        alignItems: "center",
        gap: "8px",
        minWidth: "0",
        flexWrap: "wrap",
      });

      if (eyebrow) {
        const chip = document.createElement("span");
        chip.textContent = eyebrow;
        style(chip, {
          padding: "2px 8px",
          borderRadius: "999px",
          background: "var(--accent-wash)",
          color: "var(--accent-primary)",
          fontSize: "9.5px",
          fontWeight: "700",
          letterSpacing: "0.07em",
          textTransform: "uppercase",
          flexShrink: "0",
        });
        left.appendChild(chip);
      }

      if (title) {
        const strong = document.createElement("strong");
        strong.textContent = title;
        style(strong, {
          fontSize: "13px",
          fontWeight: "650",
          color: "var(--text-primary)",
        });
        left.appendChild(strong);
      }

      if (description) {
        const sub = document.createElement("span");
        sub.textContent = description;
        style(sub, {
          fontSize: "11.5px",
          color: "var(--text-muted)",
        });
        left.appendChild(sub);
      }

      hdr.appendChild(left);
      if (headerRight) hdr.appendChild(headerRight);
      card.appendChild(hdr);
    }

    const body = document.createElement("div");
    style(body, {
      padding: flushBody ? "0" : "14px 16px",
      display: "flex",
      flexDirection: "column",
      gap: "12px",
    });
    card.appendChild(body);

    return { card, body };
  };

  const renderChartInner = (comp) => {
    const wrap = document.createElement("div");
    style(wrap, { display: "flex", flexDirection: "column", gap: "10px" });

    const data = Array.isArray(comp.data) ? comp.data : [];
    const xKey = comp.x_key || (data[0] ? Object.keys(data[0])[0] : "x");
    const yKeys =
      Array.isArray(comp.y_keys) && comp.y_keys.length
        ? comp.y_keys
        : data[0]
          ? Object.keys(data[0]).filter((k) => k !== xKey)
          : [];
    const colors = Array.isArray(comp.colors) && comp.colors.length ? comp.colors : PALETTE;
    const kind = String(comp.kind || "bar").toLowerCase();

    if (!data.length || !yKeys.length) {
      const empty = document.createElement("div");
      empty.textContent = "No chart data available.";
      style(empty, { color: "var(--text-muted)", fontSize: "12px", padding: "8px 0" });
      wrap.appendChild(empty);
      return wrap;
    }

    if (kind === "pie") {
      const yKey = yKeys[0];
      const slices = data
        .map((row, idx) => ({
          label: String(row[xKey] ?? `Item ${idx + 1}`),
          value: Math.max(0, toNumber(row[yKey]) || 0),
          color: colors[idx % colors.length],
        }))
        .filter((s) => s.value > 0);
      const total = slices.reduce((acc, s) => acc + s.value, 0) || 1;

      const rowWrap = document.createElement("div");
      style(rowWrap, {
        display: "flex",
        flexWrap: "wrap",
        alignItems: "center",
        gap: "24px",
        padding: "4px 0",
      });

      const svg = svgEl("svg", { viewBox: "0 0 210 210" });
      style(svg, {
        width: "185px",
        height: "185px",
        flexShrink: "0",
        fill: "none",
        stroke: "none",
      });

      const cx = 105;
      const cy = 105;
      const rOuter = 88;
      const rInner = 52;
      let angle = -Math.PI / 2;

      for (const s of slices) {
        const frac = s.value / total;
        const sweep = frac * Math.PI * 2;
        if (frac >= 0.9999) {
          const c = svgEl("circle", {
            cx,
            cy,
            r: (rOuter + rInner) / 2,
            fill: "none",
            stroke: s.color,
            "stroke-width": rOuter - rInner,
          });
          const t = svgEl("title");
          t.textContent = `${s.label}: ${formatCellValue(s.value)} (100%)`;
          c.appendChild(t);
          svg.appendChild(c);
        } else {
          const end = angle + sweep;
          const large = sweep > Math.PI ? 1 : 0;
          const x1 = cx + rOuter * Math.cos(angle);
          const y1 = cy + rOuter * Math.sin(angle);
          const x2 = cx + rOuter * Math.cos(end);
          const y2 = cy + rOuter * Math.sin(end);
          const x3 = cx + rInner * Math.cos(end);
          const y3 = cy + rInner * Math.sin(end);
          const x4 = cx + rInner * Math.cos(angle);
          const y4 = cy + rInner * Math.sin(angle);
          const d = [
            `M ${x1} ${y1}`,
            `A ${rOuter} ${rOuter} 0 ${large} 1 ${x2} ${y2}`,
            `L ${x3} ${y3}`,
            `A ${rInner} ${rInner} 0 ${large} 0 ${x4} ${y4}`,
            "Z",
          ].join(" ");
          const path = svgEl("path", {
            d,
            fill: s.color,
            stroke: "var(--surface-card)",
            "stroke-width": "2",
          });
          const t = svgEl("title");
          t.textContent = `${s.label}: ${formatCellValue(s.value)} (${(frac * 100).toFixed(1)}%)`;
          path.appendChild(t);
          svg.appendChild(path);
          angle = end;
        }
      }

      const centerTop = svgEl("text", {
        x: cx,
        y: cy - 4,
        "text-anchor": "middle",
        fill: "var(--text-muted)",
        "font-size": "10",
      });
      centerTop.textContent = yKey;
      const centerVal = svgEl("text", {
        x: cx,
        y: cy + 14,
        "text-anchor": "middle",
        fill: "var(--text-primary)",
        "font-size": "15",
        "font-weight": "700",
      });
      centerVal.textContent = formatCompact(total);
      svg.append(centerTop, centerVal);

      const legend = document.createElement("div");
      style(legend, {
        display: "flex",
        flexDirection: "column",
        gap: "6px",
        flex: "1",
        minWidth: "180px",
      });
      for (const s of slices) {
        const item = document.createElement("div");
        style(item, {
          display: "flex",
          alignItems: "center",
          justifyContent: "space-between",
          gap: "10px",
          fontSize: "12px",
        });
        const left = document.createElement("div");
        style(left, { display: "flex", alignItems: "center", gap: "8px", minWidth: "0" });
        const swatch = document.createElement("span");
        style(swatch, {
          width: "9px",
          height: "9px",
          borderRadius: "2.5px",
          background: s.color,
          flexShrink: "0",
        });
        const lbl = document.createElement("span");
        lbl.textContent = s.label;
        style(lbl, {
          color: "var(--text-primary)",
          overflow: "hidden",
          textOverflow: "ellipsis",
          whiteSpace: "nowrap",
        });
        left.append(swatch, lbl);

        const right = document.createElement("span");
        right.textContent = `${formatCompact(s.value)} (${((s.value / total) * 100).toFixed(1)}%)`;
        style(right, {
          color: "var(--text-secondary)",
          fontFamily: "var(--mono)",
          fontSize: "11px",
          flexShrink: "0",
        });
        item.append(left, right);
        legend.appendChild(item);
      }

      rowWrap.append(svg, legend);
      wrap.appendChild(rowWrap);
      return wrap;
    }

    const isSpark = kind === "sparkline";
    const W = 680;
    const H = isSpark ? 84 : 250;
    const padL = isSpark ? 8 : 52;
    const padR = isSpark ? 8 : 16;
    const padT = isSpark ? 8 : 16;
    const padB = isSpark ? 10 : 44;
    const plotW = W - padL - padR;
    const plotH = H - padT - padB;

    let minVal = 0;
    let maxVal = 0;
    for (const row of data) {
      for (const yk of yKeys) {
        const v = toNumber(row[yk]);
        if (Number.isFinite(v)) {
          if (v < minVal) minVal = v;
          if (v > maxVal) maxVal = v;
        }
      }
    }
    if (maxVal === minVal) maxVal = minVal + 1;
    const span = maxVal - minVal;
    const yToPx = (v) => padT + plotH - ((v - minVal) / span) * plotH;

    const svg = svgEl("svg", { viewBox: `0 0 ${W} ${H}` });
    style(svg, {
      width: "100%",
      height: "auto",
      maxHeight: isSpark ? "90px" : "265px",
      overflow: "visible",
      fill: "none",
      stroke: "none",
    });

    if (!isSpark) {
      const steps = 4;
      for (let i = 0; i <= steps; i++) {
        const val = minVal + (span * i) / steps;
        const y = yToPx(val);
        const line = svgEl("line", {
          x1: padL,
          y1: y,
          x2: W - padR,
          y2: y,
          stroke: "var(--border-default)",
          "stroke-width": "1",
        });
        const label = svgEl("text", {
          x: padL - 8,
          y: y + 3.5,
          "text-anchor": "end",
          fill: "var(--text-muted)",
          "font-size": "10",
        });
        label.textContent = formatCompact(val);
        svg.append(line, label);
      }
    }

    const n = data.length;
    if (kind === "bar") {
      const groupStep = plotW / Math.max(1, n);
      const groupW = groupStep * 0.72;
      const barW = Math.max(4, groupW / Math.max(1, yKeys.length) - 2);
      const zeroY = yToPx(Math.max(0, minVal));

      data.forEach((row, i) => {
        const gx = padL + i * groupStep + (groupStep - groupW) / 2;
        yKeys.forEach((yk, sIdx) => {
          const raw = row[yk];
          const v = toNumber(raw);
          if (!Number.isFinite(v)) return;
          const yVal = yToPx(v);
          const top = Math.min(yVal, zeroY);
          const h = Math.max(2, Math.abs(zeroY - yVal));
          const bx = gx + sIdx * (barW + 2);
          const rect = svgEl("rect", {
            x: bx,
            y: top,
            width: barW,
            height: h,
            rx: "3.5",
            fill: colors[sIdx % colors.length],
            stroke: "none",
          });
          const title = svgEl("title");
          title.textContent = `${row[xKey] ?? i} · ${yk}: ${formatCellValue(raw)}`;
          rect.appendChild(title);
          svg.appendChild(rect);

          if (n <= 12 && yKeys.length === 1) {
            const valTxt = svgEl("text", {
              x: bx + barW / 2,
              y: Math.max(10, top - 5),
              "text-anchor": "middle",
              fill: "var(--text-secondary)",
              "font-size": "9.5",
            });
            valTxt.textContent = formatCompact(v);
            svg.appendChild(valTxt);
          }
        });

        const stepEvery = Math.max(1, Math.ceil(n / 10));
        if (i % stepEvery === 0) {
          const rawLbl = String(row[xKey] ?? "");
          const shortLbl = rawLbl.length > 13 ? `${rawLbl.slice(0, 12)}…` : rawLbl;
          const xTxt = svgEl("text", {
            x: padL + (i + 0.5) * groupStep,
            y: H - padB + 17,
            "text-anchor": "middle",
            fill: "var(--text-muted)",
            "font-size": "10",
          });
          xTxt.textContent = shortLbl;
          const t = svgEl("title");
          t.textContent = rawLbl;
          xTxt.appendChild(t);
          svg.appendChild(xTxt);
        }
      });
    } else {
      const xForIdx = (i) => (n === 1 ? padL + plotW / 2 : padL + (i / (n - 1)) * plotW);
      const baseY = yToPx(Math.max(0, minVal));

      yKeys.forEach((yk, sIdx) => {
        const color = colors[sIdx % colors.length];
        const pts = [];
        data.forEach((row, i) => {
          const v = toNumber(row[yk]);
          if (Number.isFinite(v)) {
            pts.push({ x: xForIdx(i), y: yToPx(v), raw: row[yk], label: row[xKey] ?? i });
          }
        });
        if (!pts.length) return;

        if (kind === "area" && pts.length > 1) {
          const areaPts = [
            `${pts[0].x},${baseY}`,
            ...pts.map((p) => `${p.x},${p.y}`),
            `${pts[pts.length - 1].x},${baseY}`,
          ].join(" ");
          svg.appendChild(
            svgEl("polygon", {
              points: areaPts,
              fill: color,
              "fill-opacity": "0.14",
              stroke: "none",
            })
          );
        }

        if (kind !== "scatter" && pts.length > 1) {
          svg.appendChild(
            svgEl("polyline", {
              points: pts.map((p) => `${p.x},${p.y}`).join(" "),
              fill: "none",
              stroke: color,
              "stroke-width": isSpark ? "2" : "2.2",
              "stroke-linecap": "round",
              "stroke-linejoin": "round",
            })
          );
        }

        if (!isSpark) {
          for (const p of pts) {
            const dot = svgEl("circle", {
              cx: p.x,
              cy: p.y,
              r: kind === "scatter" ? "4.5" : "3.2",
              fill: color,
              stroke: "var(--surface-card)",
              "stroke-width": "1.5",
            });
            const t = svgEl("title");
            t.textContent = `${p.label} · ${yk}: ${formatCellValue(p.raw)}`;
            dot.appendChild(t);
            svg.appendChild(dot);
          }
        }
      });

      if (!isSpark) {
        const stepEvery = Math.max(1, Math.ceil(n / 8));
        data.forEach((row, i) => {
          if (i % stepEvery !== 0 && i !== n - 1) return;
          const rawLbl = String(row[xKey] ?? "");
          const shortLbl = rawLbl.length > 13 ? `${rawLbl.slice(0, 12)}…` : rawLbl;
          const xTxt = svgEl("text", {
            x: xForIdx(i),
            y: H - padB + 17,
            "text-anchor": "middle",
            fill: "var(--text-muted)",
            "font-size": "10",
          });
          xTxt.textContent = shortLbl;
          svg.appendChild(xTxt);
        });
      }
    }

    wrap.appendChild(svg);

    if (comp.show_legend !== false && !isSpark && yKeys.length > 0) {
      const legend = document.createElement("div");
      style(legend, {
        display: "flex",
        flexWrap: "wrap",
        gap: "14px",
        justifyContent: "center",
        fontSize: "11px",
        color: "var(--text-secondary)",
      });
      yKeys.forEach((yk, sIdx) => {
        const item = document.createElement("span");
        style(item, { display: "inline-flex", alignItems: "center", gap: "6px" });
        const dot = document.createElement("span");
        style(dot, {
          width: "8px",
          height: "8px",
          borderRadius: "2px",
          background: colors[sIdx % colors.length],
        });
        const txt = document.createElement("span");
        txt.textContent = yk;
        item.append(dot, txt);
        legend.appendChild(item);
      });
      wrap.appendChild(legend);
    }

    return wrap;
  };

  const renderTableParts = (comp) => {
    const wrap = document.createElement("div");
    style(wrap, { display: "flex", flexDirection: "column" });

    const rawRows = Array.isArray(comp.data) ? [...comp.data] : [];
    const columns =
      Array.isArray(comp.columns) && comp.columns.length
        ? comp.columns
        : rawRows[0]
          ? Object.keys(rawRows[0]).map((k) => ({ header: k, accessor_key: k, sortable: true }))
          : [];

    let filterQuery = "";
    let sortKey = null;
    let sortAsc = true;
    let page = 0;
    const pageSize = comp.page_size && comp.page_size > 0 ? Number(comp.page_size) : 0;

    const toolbar = document.createElement("div");
    style(toolbar, { display: "flex", alignItems: "center", gap: "8px" });
    const countChip = document.createElement("span");
    countChip.textContent = `${rawRows.length} rows`;
    style(countChip, {
      fontSize: "10.5px",
      color: "var(--text-muted)",
      fontFamily: "var(--mono)",
    });
    toolbar.appendChild(countChip);

    if (rawRows.length > 4) {
      const search = document.createElement("input");
      search.type = "search";
      search.placeholder = "Filter…";
      style(search, {
        padding: "3px 8px",
        borderRadius: "6px",
        border: "1px solid var(--border-default)",
        background: "var(--bg-primary)",
        color: "var(--text-primary)",
        fontSize: "11px",
        width: "145px",
      });
      search.addEventListener("input", () => {
        filterQuery = search.value.trim().toLowerCase();
        page = 0;
        rebuildBody();
      });
      toolbar.appendChild(search);
    }

    const scroller = document.createElement("div");
    style(scroller, { overflowX: "auto" });

    const table = document.createElement("table");
    style(table, {
      width: "100%",
      borderCollapse: "collapse",
      fontSize: "12px",
      color: "var(--text-primary)",
    });

    const thead = document.createElement("thead");
    const headRow = document.createElement("tr");
    style(headRow, {
      background: "var(--bg-primary)",
      borderBottom: "1px solid var(--border-default)",
    });

    const tbody = document.createElement("tbody");
    const pager = document.createElement("div");

    const rebuildHeader = () => {
      headRow.replaceChildren();
      for (const col of columns) {
        const th = document.createElement("th");
        const canSort = comp.sortable !== false && col.sortable !== false;
        const indicator =
          sortKey === col.accessor_key ? (sortAsc ? " ↑" : " ↓") : canSort ? " ↕" : "";
        th.textContent = `${col.header}${indicator}`;
        style(th, {
          textAlign: "left",
          padding: "8px 14px",
          fontWeight: "650",
          fontSize: "11px",
          color: "var(--text-secondary)",
          whiteSpace: "nowrap",
          cursor: canSort ? "pointer" : "default",
          userSelect: "none",
        });
        if (canSort) {
          th.addEventListener("click", () => {
            if (sortKey === col.accessor_key) sortAsc = !sortAsc;
            else {
              sortKey = col.accessor_key;
              sortAsc = true;
            }
            rebuildHeader();
            rebuildBody();
          });
        }
        headRow.appendChild(th);
      }
    };

    const rebuildBody = () => {
      tbody.replaceChildren();
      let rows = rawRows.filter((r) => {
        if (!filterQuery) return true;
        return columns.some((c) =>
          String(r[c.accessor_key] ?? "")
            .toLowerCase()
            .includes(filterQuery)
        );
      });

      if (sortKey) {
        rows.sort((a, b) => {
          const va = a[sortKey];
          const vb = b[sortKey];
          const na = toNumber(va);
          const nb = toNumber(vb);
          const cmp =
            Number.isFinite(na) && Number.isFinite(nb)
              ? na - nb
              : String(va ?? "").localeCompare(String(vb ?? ""));
          return sortAsc ? cmp : -cmp;
        });
      }

      const totalRows = rows.length;
      if (pageSize > 0) {
        const start = page * pageSize;
        rows = rows.slice(start, start + pageSize);
      }

      rows.forEach((row, idx) => {
        const tr = document.createElement("tr");
        const isLast = idx === rows.length - 1 && !(pageSize > 0 && totalRows > pageSize);
        style(tr, {
          borderBottom: isLast ? "0" : "1px solid var(--border-default)",
          background:
            comp.striped && idx % 2 === 1
              ? "color-mix(in srgb, var(--bg-primary) 55%, var(--surface-card))"
              : "transparent",
        });
        for (const col of columns) {
          const td = document.createElement("td");
          const raw = row[col.accessor_key];
          const text = formatCellValue(raw);
          const isNumeric =
            typeof raw === "number" ||
            (typeof raw === "string" && /^[$+-]?[\d,.]+%?$/.test(raw.trim()));
          td.textContent = text;
          style(td, {
            padding: "8px 14px",
            textAlign: isNumeric ? "right" : "left",
            fontFamily: isNumeric ? "var(--mono)" : "inherit",
            fontSize: isNumeric ? "11.5px" : "12px",
            whiteSpace: isNumeric ? "nowrap" : "normal",
          });
          tr.appendChild(td);
        }
        tbody.appendChild(tr);
      });

      if (pageSize > 0 && totalRows > pageSize) {
        pager.replaceChildren();
        const totalPages = Math.ceil(totalRows / pageSize);
        style(pager, {
          display: "flex",
          justifyContent: "space-between",
          alignItems: "center",
          padding: "7px 14px",
          fontSize: "11px",
          color: "var(--text-muted)",
          background: "var(--bg-primary)",
          borderTop: "1px solid var(--border-default)",
        });
        const info = document.createElement("span");
        info.textContent = `Page ${page + 1} of ${totalPages}`;
        const btnWrap = document.createElement("div");
        style(btnWrap, { display: "flex", gap: "6px" });
        const mkBtn = (lbl, disabled, onClick) => {
          const b = document.createElement("button");
          b.type = "button";
          b.textContent = lbl;
          b.disabled = disabled;
          style(b, {
            padding: "3px 9px",
            borderRadius: "5px",
            border: "1px solid var(--border-default)",
            background: "var(--surface-card)",
            color: "var(--text-secondary)",
            cursor: disabled ? "default" : "pointer",
            opacity: disabled ? "0.45" : "1",
            fontSize: "11px",
          });
          b.addEventListener("click", onClick);
          return b;
        };
        btnWrap.append(
          mkBtn("Prev", page <= 0, () => {
            page--;
            rebuildBody();
          }),
          mkBtn("Next", page >= totalPages - 1, () => {
            page++;
            rebuildBody();
          })
        );
        pager.append(info, btnWrap);
      } else {
        style(pager, { display: "none" });
      }
    };

    rebuildHeader();
    rebuildBody();
    thead.appendChild(headRow);
    table.append(thead, tbody);
    scroller.appendChild(table);
    wrap.append(scroller, pager);
    return { element: wrap, toolbar };
  };

  const renderInnerComponent = (comp, surfaceRoot, inGrid = false) => {
    if (!comp || typeof comp !== "object") return document.createTextNode("");
    const type = String(comp.type || "").toLowerCase();

    switch (type) {
      case "text": {
        const v = String(comp.variant || "body").toLowerCase();
        const raw = String(comp.content ?? "");
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
            fontSize: "19px",
            fontWeight: "750",
            letterSpacing: "-0.01em",
            lineHeight: "1.25",
            color: "var(--text-primary)",
          },
          h2: {
            fontSize: inGrid ? "20px" : "16px",
            fontWeight: "750",
            letterSpacing: "-0.01em",
            lineHeight: "1.25",
            color: "var(--text-primary)",
          },
          h3: {
            fontSize: "13.5px",
            fontWeight: "650",
            lineHeight: "1.35",
            color: "var(--text-primary)",
          },
          h4: {
            fontSize: "12.5px",
            fontWeight: "650",
            lineHeight: "1.4",
            color: "var(--text-primary)",
          },
          caption: { fontSize: "11.5px", color: "var(--text-muted)", lineHeight: "1.4" },
          code: {
            fontFamily: "var(--mono)",
            fontSize: "11.5px",
            background: "var(--bg-primary)",
            border: "1px solid var(--border-default)",
            padding: "1px 5px",
            borderRadius: "4px",
          },
          body: { fontSize: "13px", lineHeight: "1.5", color: "var(--text-primary)" },
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
          padding: "2px 8px",
          borderRadius: "999px",
          fontSize: "10.5px",
          fontWeight: "650",
          background: vc.bg,
          color: vc.fg,
          border: `1px solid ${vc.border}`,
          alignSelf: "flex-start",
        });
        return span;
      }

      case "divider": {
        const hr = document.createElement("hr");
        style(hr, {
          border: "0",
          borderTop: "1px solid var(--border-default)",
          margin: "4px 0",
          width: "100%",
        });
        return hr;
      }

      case "stack": {
        const div = document.createElement("div");
        const horiz = String(comp.direction || "vertical").toLowerCase() === "horizontal";
        const gapPx = Math.max(1, Number(comp.gap ?? 3)) * 3.5;
        style(div, {
          display: "flex",
          flexDirection: horiz ? "row" : "column",
          flexWrap: horiz ? "wrap" : "nowrap",
          alignItems: horiz ? "center" : "stretch",
          gap: `${gapPx}px`,
        });
        for (const child of comp.children || []) {
          div.appendChild(renderInnerComponent(child, surfaceRoot, inGrid));
        }
        return div;
      }

      case "grid": {
        const div = document.createElement("div");
        const cols = Math.max(1, Math.min(6, Number(comp.columns) || 2));
        style(div, {
          display: "grid",
          gridTemplateColumns: `repeat(auto-fit, minmax(${cols >= 3 ? "155px" : "210px"}, 1fr))`,
          gap: "10px",
          alignItems: "stretch",
        });
        for (const child of comp.children || []) {
          div.appendChild(renderInnerComponent(child, surfaceRoot, true));
        }
        return div;
      }

      case "container": {
        const div = document.createElement("div");
        style(div, { display: "flex", flexDirection: "column", gap: "10px" });
        for (const child of comp.children || []) {
          div.appendChild(renderInnerComponent(child, surfaceRoot, inGrid));
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
          gap: "6px",
          padding: "12px 14px",
          borderRadius: "12px",
          background: "var(--bg-primary)",
          border: "1px solid var(--border-default)",
        });

        if (comp.title) {
          const t = document.createElement("div");
          t.textContent = comp.title;
          style(
            t,
            inGrid
              ? {
                  fontSize: "10px",
                  fontWeight: "750",
                  textTransform: "uppercase",
                  letterSpacing: "0.06em",
                  color: "var(--text-muted)",
                }
              : {
                  fontSize: "12.5px",
                  fontWeight: "650",
                  color: "var(--text-primary)",
                }
          );
          tile.appendChild(t);
        }
        if (comp.description) {
          const d = document.createElement("div");
          d.textContent = comp.description;
          style(d, { fontSize: "11.5px", color: "var(--text-muted)" });
          tile.appendChild(d);
        }
        if (Array.isArray(comp.content) && comp.content.length) {
          const body = document.createElement("div");
          style(body, { display: "flex", flexDirection: "column", gap: "6px" });
          for (const child of comp.content) {
            body.appendChild(renderInnerComponent(child, tile, inGrid));
          }
          tile.appendChild(body);
        }
        if (Array.isArray(comp.footer) && comp.footer.length) {
          const ftr = document.createElement("div");
          style(ftr, {
            display: "flex",
            flexWrap: "wrap",
            gap: "6px",
            justifyContent: "flex-end",
            paddingTop: "8px",
            borderTop: "1px solid var(--border-default)",
          });
          for (const child of comp.footer) {
            ftr.appendChild(renderInnerComponent(child, tile, false));
          }
          tile.appendChild(ftr);
        }
        return tile;
      }

      case "tabs": {
        const wrap = document.createElement("div");
        style(wrap, { display: "flex", flexDirection: "column", gap: "10px" });
        const bar = document.createElement("div");
        style(bar, {
          display: "flex",
          gap: "6px",
          borderBottom: "1px solid var(--border-default)",
          paddingBottom: "6px",
        });
        const panel = document.createElement("div");
        const tabs = Array.isArray(comp.tabs) ? comp.tabs : [];
        let activeIdx = 0;

        const selectTab = (idx) => {
          activeIdx = idx;
          Array.from(bar.children).forEach((btn, i) => {
            style(btn, {
              background: i === activeIdx ? "var(--accent-wash)" : "var(--bg-primary)",
              color: i === activeIdx ? "var(--accent-primary)" : "var(--text-secondary)",
              borderColor:
                i === activeIdx ? "var(--accent-primary)" : "var(--border-default)",
            });
          });
          panel.replaceChildren();
          for (const child of tabs[idx]?.content || []) {
            panel.appendChild(renderInnerComponent(child, surfaceRoot, false));
          }
        };

        tabs.forEach((tab, idx) => {
          const b = document.createElement("button");
          b.type = "button";
          b.textContent = tab.label || `Tab ${idx + 1}`;
          style(b, {
            border: "1px solid var(--border-default)",
            borderRadius: "999px",
            padding: "5px 12px",
            fontSize: "11.5px",
            fontWeight: "600",
            cursor: "pointer",
          });
          b.addEventListener("click", () => selectTab(idx));
          bar.appendChild(b);
        });
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
          border: "1px solid var(--border-default)",
          borderRadius: "12px",
          overflow: "hidden",
        });
        const bar = document.createElement("div");
        style(bar, {
          display: "flex",
          justifyContent: "flex-end",
          padding: "7px 12px",
          background: "var(--bg-primary)",
          borderBottom: "1px solid var(--border-default)",
        });
        bar.appendChild(toolbar);
        box.append(bar, element);
        return box;
      }

      case "key_value": {
        const dl = document.createElement("div");
        style(dl, {
          display: "grid",
          gridTemplateColumns: "minmax(130px, auto) 1fr",
          gap: "7px 16px",
          padding: "11px 14px",
          borderRadius: "12px",
          background: "var(--bg-primary)",
          border: "1px solid var(--border-default)",
          fontSize: "12px",
        });
        for (const pair of comp.pairs || []) {
          const k = document.createElement("div");
          k.textContent = pair.key;
          style(k, { color: "var(--text-muted)", fontWeight: "600" });
          const v = document.createElement("div");
          v.textContent = pair.value;
          style(v, { color: "var(--text-primary)", fontWeight: "650" });
          dl.append(k, v);
        }
        return dl;
      }

      case "list": {
        const list = document.createElement(comp.ordered ? "ol" : "ul");
        style(list, {
          margin: "0",
          paddingLeft: "20px",
          display: "flex",
          flexDirection: "column",
          gap: "4px",
          fontSize: "12.5px",
          color: "var(--text-primary)",
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
          padding: "10px 12px",
          borderRadius: "7px",
          background: "var(--bg-primary)",
          border: "1px solid var(--border-default)",
          color: "var(--text-primary)",
          fontSize: "11.5px",
          overflowX: "auto",
          fontFamily: "var(--mono)",
        });
        return pre;
      }

      case "alert":
      case "toast": {
        const box = document.createElement("div");
        const vc = variantColors(comp.variant);
        style(box, {
          padding: "10px 12px",
          borderRadius: "8px",
          background: vc.bg,
          border: `1px solid ${vc.border}`,
          display: "flex",
          flexDirection: "column",
          gap: "3px",
        });
        const title = document.createElement("div");
        title.textContent = comp.title || comp.message || "";
        style(title, { fontWeight: "650", fontSize: "12.5px", color: vc.fg });
        box.appendChild(title);
        if (comp.description) {
          const desc = document.createElement("div");
          desc.textContent = comp.description;
          style(desc, { fontSize: "12px", color: "var(--text-secondary)" });
          box.appendChild(desc);
        }
        return box;
      }

      case "progress": {
        const wrap = document.createElement("div");
        style(wrap, { display: "flex", flexDirection: "column", gap: "6px" });
        const pct = Math.max(0, Math.min(100, Number(comp.value) || 0));
        const top = document.createElement("div");
        style(top, { display: "flex", justifyContent: "space-between", fontSize: "12px" });
        const lbl = document.createElement("span");
        lbl.textContent = comp.label || "Progress";
        style(lbl, { color: "var(--text-primary)", fontWeight: "600" });
        const val = document.createElement("span");
        val.textContent = `${pct}%`;
        style(val, { color: "var(--text-muted)", fontFamily: "var(--mono)" });
        top.append(lbl, val);

        const track = document.createElement("div");
        style(track, {
          height: "7px",
          borderRadius: "999px",
          background: "var(--bg-primary)",
          border: "1px solid var(--border-default)",
          overflow: "hidden",
        });
        const bar = document.createElement("div");
        style(bar, {
          width: `${pct}%`,
          height: "100%",
          borderRadius: "999px",
          background: "var(--accent-primary)",
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
        style(field, { display: "flex", flexDirection: "column", gap: "4px", fontSize: "11.5px" });
        const lbl = document.createElement("span");
        lbl.textContent = `${comp.label || comp.name}${comp.required ? " *" : ""}`;
        style(lbl, { fontWeight: "650", color: "var(--text-secondary)" });
        field.appendChild(lbl);

        let control;
        if (type === "textarea") {
          control = document.createElement("textarea");
          control.rows = Number(comp.rows) || 3;
          control.placeholder = comp.placeholder || "";
          control.value = comp.default_value || "";
        } else if (type === "select" || type === "multi_select") {
          control = document.createElement("select");
          if (type === "multi_select") control.multiple = true;
          for (const opt of comp.options || []) {
            const o = document.createElement("option");
            o.value = opt.value;
            o.textContent = opt.label;
            control.appendChild(o);
          }
        } else if (type === "switch") {
          const row = document.createElement("div");
          style(row, { display: "flex", alignItems: "center", gap: "8px", padding: "2px 0" });
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
          if (comp.default_value !== undefined && comp.default_value !== null) {
            control.value = String(comp.default_value);
          }
        }

        if (type !== "switch") {
          style(control, {
            padding: type === "select" ? "8px 30px 8px 11px" : "8px 11px",
            borderRadius: "10px",
            border: "1px solid var(--border-default)",
            backgroundColor: "var(--bg-primary)",
            color: "var(--text-primary)",
            fontSize: "12.5px",
          });
          field.appendChild(control);
        }

        control.setAttribute("data-ui-field", comp.name || comp.label || "field");
        return field;
      }

      case "button": {
        const btn = document.createElement("button");
        btn.type = "button";
        btn.textContent = comp.label || "Submit";
        btn.disabled = !!comp.disabled;
        const v = String(comp.variant || "primary").toLowerCase();
        const btnStyles =
          v === "primary"
            ? {
                background: "var(--accent-primary)",
                color: "#fff",
                border: "1px solid var(--accent-primary)",
              }
            : v === "danger"
              ? {
                  background: "var(--accent-error)",
                  color: "#fff",
                  border: "1px solid var(--accent-error)",
                }
              : {
                  background: "var(--bg-primary)",
                  color: "var(--text-secondary)",
                  border: "1px solid var(--border-default)",
                };
        style(btn, {
          padding: "7px 16px",
          borderRadius: "999px",
          fontSize: "12px",
          fontWeight: "600",
          cursor: btn.disabled ? "default" : "pointer",
          alignSelf: "flex-start",
          ...btnStyles,
        });

        btn.addEventListener("click", () => {
          const scope = btn.closest('[data-ui-scope="true"]') || surfaceRoot;
          const inputs = scope ? Array.from(scope.querySelectorAll("[data-ui-field]")) : [];
          const actionId = comp.action_id || comp.label || "click";
          let msg;
          if (inputs.length > 0) {
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
          sendMessage(msg);
        });
        return btn;
      }

      default:
        return document.createTextNode("");
    }
  };

  const buildStandaloneCards = (uiResp) => {
    const comps = Array.isArray(uiResp.components) ? uiResp.components : [];
    const cards = [];

    if (
      (comps.length === 2 && comps[0]?.type === "text" && comps[1]?.type === "table") ||
      (comps.length === 1 && comps[0]?.type === "table")
    ) {
      const title = comps.length === 2 ? comps[0].content : comps[0].title || "Data Table";
      const tableComp = comps.length === 2 ? comps[1] : comps[0];
      const { element, toolbar } = renderTableParts(tableComp);
      const { card, body } = createStandaloneCard({
        title,
        eyebrow: "Table",
        headerRight: toolbar,
        flushBody: true,
      });
      body.appendChild(element);
      cards.push(card);
      return { title, eyebrow: "Table", cards };
    }

    if (comps.length === 1 && comps[0]?.type === "chart") {
      const chartComp = comps[0];
      const kind = String(chartComp.kind || "bar").toUpperCase();
      const title = chartComp.title || "Chart";
      const { card, body } = createStandaloneCard({ title, eyebrow: kind });
      body.appendChild(renderChartInner({ ...chartComp, title: null }));
      cards.push(card);
      return { title, eyebrow: kind, cards };
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
        comp.children[0]?.type === "text" &&
        comp.children[1]?.type === "grid"
      ) {
        const secTitle = comp.children[0].content;
        if (!surfaceTitle) surfaceTitle = secTitle;
        const { card, body } = createStandaloneCard({
          title: secTitle,
          description: pendingTitle
            ? `${pendingTitle}${pendingSubtitle ? ` · ${pendingSubtitle}` : ""}`
            : null,
          eyebrow: "KPIs",
        });
        pendingTitle = null;
        pendingSubtitle = null;
        body.appendChild(renderInnerComponent(comp.children[1], card, true));
        cards.push(card);
        continue;
      }

      if (t === "card" || t === "modal") {
        const contentArr = Array.isArray(comp.content) ? comp.content : [];
        const singleTable = contentArr.length === 1 && contentArr[0]?.type === "table";
        const singleChart = contentArr.length === 1 && contentArr[0]?.type === "chart";

        let headerRight = null;
        let tableElement = null;
        if (singleTable) {
          const parts = renderTableParts(contentArr[0]);
          headerRight = parts.toolbar;
          tableElement = parts.element;
        }

        const eyebrow = singleTable
          ? "Table"
          : singleChart
            ? String(contentArr[0].kind || "Chart").toUpperCase()
            : pendingTitle || "Card";

        const cardTitle = comp.title || pendingTitle || "Details";
        if (!surfaceTitle) surfaceTitle = cardTitle;

        const { card, body } = createStandaloneCard({
          title: cardTitle,
          description: comp.description || (comp.title && pendingTitle ? pendingSubtitle : null),
          eyebrow,
          headerRight,
          flushBody: singleTable,
        });
        pendingTitle = null;
        pendingSubtitle = null;

        if (singleTable && tableElement) {
          body.appendChild(tableElement);
        } else if (singleChart) {
          body.appendChild(renderChartInner({ ...contentArr[0], title: null }));
        } else {
          for (const child of contentArr) {
            body.appendChild(renderInnerComponent(child, card, false));
          }
        }

        if (Array.isArray(comp.footer) && comp.footer.length) {
          const ftr = document.createElement("footer");
          style(ftr, {
            display: "flex",
            flexWrap: "wrap",
            gap: "8px",
            justifyContent: "flex-end",
            padding: "10px 14px",
            borderTop: "1px solid var(--border-default)",
            background: "var(--bg-primary)",
          });
          for (const child of comp.footer) {
            ftr.appendChild(renderInnerComponent(child, card, false));
          }
          card.appendChild(ftr);
        }

        cards.push(card);
        continue;
      }

      const fallbackTitle = comp.title || pendingTitle || "Component";
      if (!surfaceTitle) surfaceTitle = fallbackTitle;
      const { card, body } = createStandaloneCard({
        title: fallbackTitle,
        description: pendingSubtitle || null,
        eyebrow: t === "alert" || t === "toast" ? String(comp.variant || "Alert") : "UI",
      });
      pendingTitle = null;
      pendingSubtitle = null;
      body.appendChild(renderInnerComponent(comp, card, false));
      cards.push(card);
    }

    return {
      title: surfaceTitle || "Dashboard",
      eyebrow: cards.length > 1 ? "Dashboard" : "Card",
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

  const appendInlineWithCodeTable = (parent, text, codes) => {
    if (!text) return;
    const tokenRegex =
      /(\u0000CODE(\d+)\u0000|\[([^\]]+)\]\((https?:\/\/[^\s)]+|mailto:[^\s)]+|\/(?!\/)[^\s)]*)\)|\*\*\*([\s\S]+?)\*\*\*|\*\*([\s\S]+?)\*\*|__(?=\S)([\s\S]*?\S)__|~~([\s\S]+?)~~|(?<!\*)\*(?=\S)([\s\S]*?\S)\*(?!\*)|(?<![a-zA-Z0-9_])_(?=\S)([\s\S]*?\S)_(?![a-zA-Z0-9_]))/g;

    let lastIdx = 0;
    let match;
    while ((match = tokenRegex.exec(text)) !== null) {
      if (match.index > lastIdx) {
        parent.appendChild(document.createTextNode(text.slice(lastIdx, match.index)));
      }
      if (match[2] !== undefined) {
        const code = document.createElement("code");
        code.textContent = codes[Number(match[2])] ?? "";
        parent.appendChild(code);
      } else if (match[3] !== undefined && match[4] !== undefined) {
        const a = document.createElement("a");
        appendInlineWithCodeTable(a, match[3], codes);
        a.href = match[4];
        a.target = "_blank";
        a.rel = "noreferrer";
        parent.appendChild(a);
      } else if (match[5] !== undefined) {
        const strong = document.createElement("strong");
        const em = document.createElement("em");
        appendInlineWithCodeTable(em, match[5], codes);
        strong.appendChild(em);
        parent.appendChild(strong);
      } else if (match[6] !== undefined || match[7] !== undefined) {
        const strong = document.createElement("strong");
        appendInlineWithCodeTable(strong, match[6] ?? match[7], codes);
        parent.appendChild(strong);
      } else if (match[8] !== undefined) {
        const del = document.createElement("del");
        appendInlineWithCodeTable(del, match[8], codes);
        parent.appendChild(del);
      } else if (match[9] !== undefined || match[10] !== undefined) {
        const em = document.createElement("em");
        appendInlineWithCodeTable(em, match[9] ?? match[10], codes);
        parent.appendChild(em);
      }
      lastIdx = tokenRegex.lastIndex;
    }
    if (lastIdx < text.length) {
      parent.appendChild(document.createTextNode(text.slice(lastIdx)));
    }
  };

  const appendInlineMarkdown = (parent, rawText) => {
    const str = String(rawText ?? "");
    if (!str) return;
    const codes = [];
    const masked = str.replace(/`([^`\n]+)`/g, (_, codeContent) => {
      const idx = codes.length;
      codes.push(codeContent);
      return `\u0000CODE${idx}\u0000`;
    });
    appendInlineWithCodeTable(parent, masked, codes);
  };

  const isHrLine = (line) => /^\s*([-*_])(?:\s*\1){2,}\s*$/.test(line);
  const isHeadingLine = (line) => /^\s*#{1,6}\s+\S/.test(line);
  const isFenceLine = (line) => /^\s*(`{3,}|~{3,})/.test(line);
  const isBlockquoteLine = (line) => /^\s*>\s?/.test(line);
  const isListLine = (line) => !isHrLine(line) && /^\s*(?:[-*+]|\d+[.)])\s+/.test(line);
  const isTableSepLine = (line) =>
    /^\s*\|?(?:\s*:?-{2,}:?\s*\|)+\s*:?-{2,}:?\s*\|?\s*$/.test(line);
  const isTableStart = (lines, idx) =>
    idx + 1 < lines.length &&
    lines[idx].includes("|") &&
    isTableSepLine(lines[idx + 1]);

  const splitTableCells = (line) => {
    let trimmed = line.trim();
    if (trimmed.startsWith("|")) trimmed = trimmed.slice(1);
    if (trimmed.endsWith("|")) trimmed = trimmed.slice(0, -1);
    return trimmed.split("|").map((c) => c.trim());
  };

  const parseMarkdownBlocksInto = (container, text) => {
    const lines = String(text || "")
      .replace(/\r\n/g, "\n")
      .split("\n");

    const parseList = (startIdx, baseIndent, parentEl) => {
      let i = startIdx;
      const firstMatch = lines[i].match(/^(\s*)([-*+]|\d+[.)])\s+(.*)$/);
      if (!firstMatch) return i + 1;

      const isOrdered = /^\d+[.)]$/.test(firstMatch[2]);
      const listEl = document.createElement(isOrdered ? "ol" : "ul");
      if (isOrdered) {
        const startNum = parseInt(firstMatch[2], 10);
        if (Number.isFinite(startNum) && startNum !== 1) {
          listEl.start = startNum;
        }
      }
      let lastLi = null;

      while (i < lines.length) {
        const line = lines[i];
        if (line.trim() === "") {
          let nextNonEmpty = i + 1;
          while (nextNonEmpty < lines.length && lines[nextNonEmpty].trim() === "") {
            nextNonEmpty++;
          }
          if (nextNonEmpty < lines.length && isListLine(lines[nextNonEmpty])) {
            const nextIndent = lines[nextNonEmpty]
              .match(/^(\s*)/)[1]
              .replace(/\t/g, "  ").length;
            if (nextIndent >= baseIndent) {
              i = nextNonEmpty;
              continue;
            }
          }
          break;
        }

        if (isHrLine(line) || isFenceLine(line) || isHeadingLine(line) || isBlockquoteLine(line)) {
          break;
        }

        const itemMatch = line.match(/^(\s*)([-*+]|\d+[.)])\s+(.*)$/);
        if (itemMatch) {
          const indent = itemMatch[1].replace(/\t/g, "  ").length;
          if (indent < baseIndent) {
            break;
          }
          if (indent > baseIndent) {
            i = parseList(i, indent, lastLi || listEl);
            continue;
          }
          const itemOrdered = /^\d+[.)]$/.test(itemMatch[2]);
          if (itemOrdered !== isOrdered) {
            break;
          }
          const li = document.createElement("li");
          appendInlineMarkdown(li, itemMatch[3]);
          listEl.appendChild(li);
          lastLi = li;
          i++;
          continue;
        }

        const contIndent = line.match(/^(\s*)/)[1].replace(/\t/g, "  ").length;
        if (lastLi && contIndent > baseIndent) {
          lastLi.appendChild(document.createElement("br"));
          appendInlineMarkdown(lastLi, line.trim());
          i++;
          continue;
        }
        break;
      }

      parentEl.appendChild(listEl);
      return i;
    };

    let i = 0;
    while (i < lines.length) {
      const line = lines[i];

      if (line.trim() === "") {
        i++;
        continue;
      }

      // Fenced code block
      const fenceMatch = line.match(/^\s*(`{3,}|~{3,})(.*)$/);
      if (fenceMatch) {
        const fence = fenceMatch[1];
        const codeLines = [];
        i++;
        const closeRe = new RegExp(`^\\s*${fence[0]}{${fence.length},}\\s*$`);
        while (i < lines.length && !closeRe.test(lines[i])) {
          codeLines.push(lines[i]);
          i++;
        }
        if (i < lines.length) i++; // skip closing fence
        const pre = document.createElement("pre");
        const code = document.createElement("code");
        code.textContent = codeLines.join("\n");
        pre.appendChild(code);
        container.appendChild(pre);
        continue;
      }

      // Horizontal rule
      if (isHrLine(line)) {
        container.appendChild(document.createElement("hr"));
        i++;
        continue;
      }

      // ATX Heading (# .. ######)
      const headingMatch = line.match(/^\s*(#{1,6})\s+(.+?)\s*#*$/);
      if (headingMatch) {
        const level = Math.min(4, headingMatch[1].length);
        const h = document.createElement(`h${level}`);
        appendInlineMarkdown(h, headingMatch[2]);
        container.appendChild(h);
        i++;
        continue;
      }

      // Blockquote
      if (isBlockquoteLine(line)) {
        const quoteLines = [];
        while (i < lines.length && isBlockquoteLine(lines[i])) {
          quoteLines.push(lines[i].replace(/^\s*>\s?/, ""));
          i++;
        }
        const bq = document.createElement("blockquote");
        parseMarkdownBlocksInto(bq, quoteLines.join("\n"));
        container.appendChild(bq);
        continue;
      }

      // GFM Table
      if (isTableStart(lines, i)) {
        const headers = splitTableCells(lines[i]);
        i += 2; // skip header + separator
        const table = document.createElement("table");
        const thead = document.createElement("thead");
        const trHead = document.createElement("tr");
        for (const hCell of headers) {
          const th = document.createElement("th");
          appendInlineMarkdown(th, hCell);
          trHead.appendChild(th);
        }
        thead.appendChild(trHead);
        const tbody = document.createElement("tbody");
        while (i < lines.length && lines[i].trim() !== "" && lines[i].includes("|")) {
          const cells = splitTableCells(lines[i]);
          const tr = document.createElement("tr");
          headers.forEach((_, colIdx) => {
            const td = document.createElement("td");
            appendInlineMarkdown(td, cells[colIdx] ?? "");
            tr.appendChild(td);
          });
          tbody.appendChild(tr);
          i++;
        }
        table.append(thead, tbody);
        container.appendChild(table);
        continue;
      }

      // Unordered or Ordered List
      if (isListLine(line)) {
        const baseIndent = line.match(/^(\s*)/)[1].replace(/\t/g, "  ").length;
        i = parseList(i, baseIndent, container);
        continue;
      }

      // Paragraph
      const paraLines = [];
      while (
        i < lines.length &&
        lines[i].trim() !== "" &&
        !isFenceLine(lines[i]) &&
        !isHrLine(lines[i]) &&
        !isHeadingLine(lines[i]) &&
        !isBlockquoteLine(lines[i]) &&
        !isListLine(lines[i]) &&
        !isTableStart(lines, i)
      ) {
        paraLines.push(lines[i]);
        i++;
      }
      if (paraLines.length > 0) {
        const p = document.createElement("p");
        paraLines.forEach((pLine, idx) => {
          if (idx > 0) p.appendChild(document.createElement("br"));
          appendInlineMarkdown(p, pLine);
        });
        container.appendChild(p);
      }
    }
  };

  const renderMarkdownBlock = (text) => {
    const container = document.createElement("div");
    container.className = "message-text";
    parseMarkdownBlocksInto(container, text);
    return container;
  };

  let currentEvents = [];

  // ---------------------------------------------------------------------------
  // OpenTelemetry GenAI Token Usage (`adk_telemetry` / `gen_ai.usage.*`) & Cost
  // ---------------------------------------------------------------------------

  // Official Paid Tier pricing per 1M tokens (USD) from https://ai.google.dev/gemini-api/docs/pricing
  const getDefaultModelPricing = (model, tier = prefs.pricingTier) => {
    const m = String(model || prefs.model || DEFAULT_MODELS.model)
      .trim()
      .toLowerCase();
    let base;
    if (m.includes("argon") || m.includes("gemini-4")) {
      base = {
        input: 2.0,
        inputLong: 2.0,
        cached: 0.1,
        cachedLong: 0.1,
        output: 10.0,
        outputLong: 10.0,
        audioIn: 2.0,
      };
    } else if (m.includes("3.8-flash") || m.includes("3.6-flash")) {
      base = {
        input: 0.75,
        inputLong: 0.75,
        cached: 0.075,
        cachedLong: 0.075,
        output: 3.75,
        outputLong: 3.75,
        audioIn: 0.75,
      };
    } else if (m.includes("3.5-flash-lite")) {
      base = {
        input: 0.3,
        inputLong: 0.3,
        cached: 0.03,
        cachedLong: 0.03,
        output: 2.5,
        outputLong: 2.5,
        audioIn: 0.3,
      };
    } else if (m.includes("3.1-flash-lite")) {
      base = {
        input: 0.25,
        inputLong: 0.25,
        cached: 0.025,
        cachedLong: 0.025,
        output: 1.5,
        outputLong: 1.5,
        audioIn: 0.5,
      };
    } else if (m.includes("3.1-pro") || (m.includes("gemini-3") && m.includes("pro"))) {
      base = {
        input: 2.0,
        inputLong: 4.0,
        cached: 0.2,
        cachedLong: 0.4,
        output: 12.0,
        outputLong: 18.0,
        audioIn: 2.0,
      };
    } else if (m.includes("gemini-3") && m.includes("flash")) {
      base = {
        input: 0.5,
        inputLong: 0.5,
        cached: 0.05,
        cachedLong: 0.05,
        output: 3.0,
        outputLong: 3.0,
        audioIn: 1.0,
      };
    } else if (m.includes("2.5-flash-lite") || m.includes("flash-lite")) {
      base = {
        input: 0.1,
        inputLong: 0.1,
        cached: 0.01,
        cachedLong: 0.01,
        output: 0.4,
        outputLong: 0.4,
        audioIn: 0.3,
      };
    } else if (m.includes("2.0-flash")) {
      base = {
        input: 0.1,
        inputLong: 0.1,
        cached: 0.025,
        cachedLong: 0.025,
        output: 0.4,
        outputLong: 0.4,
        audioIn: 0.7,
      };
    } else if (m.includes("flash")) {
      base = {
        input: 0.3,
        inputLong: 0.3,
        cached: 0.03,
        cachedLong: 0.03,
        output: 2.5,
        outputLong: 2.5,
        audioIn: 1.0,
      };
    } else {
      base = {
        input: 1.25,
        inputLong: 2.5,
        cached: 0.125,
        cachedLong: 0.25,
        output: 10.0,
        outputLong: 15.0,
        audioIn: 1.25,
      };
    }

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

  const formatCompactTokens = (n) => {
    const num = Math.max(0, Math.round(Number(n) || 0));
    if (num >= 1_000_000) return `${(num / 1_000_000).toFixed(2)}M`;
    if (num >= 10_000) return `${(num / 1_000).toFixed(1)}k`;
    if (num >= 1_000) return `${(num / 1_000).toFixed(2)}k`;
    return String(num);
  };

  const formatExactInt = (n) => Math.max(0, Math.round(Number(n) || 0)).toLocaleString("en-US");

  const formatUsdShort = (cost) => {
    const val = Math.max(0, Number(cost) || 0);
    if (val === 0) return "$0.00";
    if (val < 0.0001) return "<$0.0001";
    if (val < 10) return `~$${val.toFixed(4)}`;
    return `~$${val.toFixed(2)}`;
  };

  const formatUsdDetail = (cost) => {
    const val = Math.max(0, Number(cost) || 0);
    if (val === 0) return "$0.0000";
    if (val < 0.0001) return `$${val.toFixed(6)}`;
    if (val < 1) return `$${val.toFixed(4)}`;
    return `$${val.toFixed(2)}`;
  };

  const prefersReducedMotion = () =>
    Boolean(window.matchMedia?.("(prefers-reduced-motion: reduce)")?.matches);

  const triggerPulseClass = (el, className) => {
    if (!el || prefersReducedMotion()) return;
    const now = performance.now();
    if (el._lastPulseAt && now - el._lastPulseAt < 450) return;
    el._lastPulseAt = now;
    el.classList.remove(className);
    void el.offsetWidth;
    el.classList.add(className);
    el.addEventListener(
      "animationend",
      () => {
        el.classList.remove(className);
      },
      { once: true }
    );
  };

  // 22-cell vertical reel: top guard '9', first cycle '0'..'9' (indices 1..10),
  // second cycle '0'..'9' (indices 11..20), bottom guard '0' (index 21).
  // Allows every digit to spin forward and overshoot/bounce without showing blank space.
  const SLOT_REEL_CELLS = [
    "9",
    "0",
    "1",
    "2",
    "3",
    "4",
    "5",
    "6",
    "7",
    "8",
    "9",
    "0",
    "1",
    "2",
    "3",
    "4",
    "5",
    "6",
    "7",
    "8",
    "9",
    "0",
  ];

  const slotReelOffsetPct = (cellIdx) => `-${((cellIdx / SLOT_REEL_CELLS.length) * 100).toFixed(4)}%`;

  const createSlotDigitEl = (initialDigit) => {
    const digitEl = document.createElement("span");
    digitEl.className = "odometer-digit";
    const ribbon = document.createElement("span");
    ribbon.className = "odometer-ribbon";
    for (const ch of SLOT_REEL_CELLS) {
      const cell = document.createElement("span");
      cell.className = "odometer-cell";
      cell.textContent = ch;
      ribbon.appendChild(cell);
    }
    const d = Math.max(0, Math.min(9, Number(initialDigit) || 0));
    ribbon.style.transition = "none";
    ribbon.style.transform = `translateY(${slotReelOffsetPct(1 + d)})`;
    digitEl.appendChild(ribbon);
    digitEl._ribbon = ribbon;
    digitEl._digit = d;
    digitEl._inSecondCycle = false;
    digitEl._resetTimer = null;
    return digitEl;
  };

  const spinSlotDigit = (digitEl, nextDigit, staggerMs = 0, forceSpin = false) => {
    const ribbon = digitEl._ribbon;
    if (!ribbon) return;
    const targetDigit = Math.max(0, Math.min(9, Number(nextDigit) || 0));
    const prevDigit = typeof digitEl._digit === "number" ? digitEl._digit : 0;
    if (!forceSpin && prevDigit === targetDigit) return;

    if (digitEl._resetTimer) {
      clearTimeout(digitEl._resetTimer);
      digitEl._resetTimer = null;
    }

    if (digitEl._inSecondCycle) {
      ribbon.style.transition = "none";
      ribbon.style.transform = `translateY(${slotReelOffsetPct(1 + prevDigit)})`;
      digitEl._inSecondCycle = false;
      void ribbon.offsetHeight;
    }

    digitEl._digit = targetDigit;
    const useSecondCycle = forceSpin || targetDigit <= prevDigit;
    const targetCellIdx = (useSecondCycle ? 11 : 1) + targetDigit;
    const durationMs = 560 + staggerMs;

    // Ease-out with a slot-machine spring overshoot & bounce back into place
    ribbon.style.transition = `transform ${durationMs}ms cubic-bezier(0.22, 1.42, 0.36, 1)`;
    ribbon.style.transform = `translateY(${slotReelOffsetPct(targetCellIdx)})`;

    if (useSecondCycle) {
      digitEl._inSecondCycle = true;
      digitEl._resetTimer = setTimeout(() => {
        digitEl._resetTimer = null;
        if (!digitEl._inSecondCycle) return;
        ribbon.style.transition = "none";
        ribbon.style.transform = `translateY(${slotReelOffsetPct(1 + targetDigit)})`;
        digitEl._inSecondCycle = false;
      }, durationMs + 30);
    }
  };

  const applySlotMachineText = (el, formattedStr, animate) => {
    el.setAttribute("aria-label", formattedStr);
    if (prefersReducedMotion()) {
      el.classList.remove("odometer");
      el._odometerTemplate = null;
      el._odometerDigits = null;
      el.textContent = formattedStr;
      return;
    }

    el.classList.add("odometer");
    const templateSig = formattedStr.replace(/\d/g, "#");
    const chars = Array.from(formattedStr);

    if (el._odometerTemplate === templateSig && el._odometerDigits) {
      let dIdx = 0;
      for (const ch of chars) {
        if (ch >= "0" && ch <= "9") {
          const digitEl = el._odometerDigits[dIdx];
          if (digitEl) {
            if (animate) {
              spinSlotDigit(digitEl, Number(ch), dIdx * 28, false);
            } else {
              const d = Number(ch);
              digitEl._digit = d;
              digitEl._inSecondCycle = false;
              digitEl._ribbon.style.transition = "none";
              digitEl._ribbon.style.transform = `translateY(${slotReelOffsetPct(1 + d)})`;
            }
          }
          dIdx += 1;
        }
      }
      return;
    }

    // Preserve existing digit reels aligned from right-to-left (ones, tens, hundreds...)
    const prevDigits = Array.isArray(el._odometerDigits) ? el._odometerDigits : [];
    const totalNextDigits = chars.filter((c) => c >= "0" && c <= "9").length;
    const reuseOffset = totalNextDigits - prevDigits.length;

    const frag = document.createDocumentFragment();
    const nextDigits = [];
    const spinsToRun = [];
    let dIdx = 0;

    for (const ch of chars) {
      if (ch >= "0" && ch <= "9") {
        const targetDigit = Number(ch);
        const existingIdx = dIdx - reuseOffset;
        let digitEl;
        let isNewReel = false;
        if (existingIdx >= 0 && existingIdx < prevDigits.length) {
          digitEl = prevDigits[existingIdx];
        } else {
          digitEl = createSlotDigitEl(animate ? 0 : targetDigit);
          isNewReel = true;
        }
        nextDigits.push(digitEl);
        frag.appendChild(digitEl);
        if (animate) {
          spinsToRun.push({
            digitEl,
            targetDigit,
            stagger: dIdx * 28,
            forceSpin: isNewReel,
          });
        } else if (!isNewReel) {
          digitEl._digit = targetDigit;
          digitEl._inSecondCycle = false;
          digitEl._ribbon.style.transition = "none";
          digitEl._ribbon.style.transform = `translateY(${slotReelOffsetPct(1 + targetDigit)})`;
        }
        dIdx += 1;
      } else {
        const staticEl = document.createElement("span");
        staticEl.className = "odometer-static";
        staticEl.textContent = ch;
        frag.appendChild(staticEl);
      }
    }

    el.replaceChildren(frag);
    el._odometerTemplate = templateSig;
    el._odometerDigits = nextDigits;

    if (animate && spinsToRun.length > 0) {
      // Flush layout so newly inserted reels animate from 0 to their target digit
      void el.offsetHeight;
      for (const item of spinsToRun) {
        spinSlotDigit(item.digitEl, item.targetDigit, item.stagger, item.forceSpin);
      }
    }
  };

  const animateCounter = (el, nextValue, formatter, options = {}) => {
    if (!el) return;
    const target = Math.max(0, Number(nextValue) || 0);
    const formatSig = options.formatSig ?? "";
    el._counterFormatFn = formatter;
    if (el._counterTarget === target && el._counterFormatSig === formatSig) {
      return;
    }

    const currentVal =
      typeof el._counterValue === "number" && Number.isFinite(el._counterValue)
        ? el._counterValue
        : 0;
    const hadPreviousTarget = typeof el._counterTarget === "number";
    el._counterTarget = target;
    el._counterFormatSig = formatSig;

    if (
      prefersReducedMotion() ||
      target === 0 ||
      Math.abs(target - currentVal) < 1e-9 ||
      (!hadPreviousTarget && !options.pulseOnMount && currentVal === 0 && target === 0)
    ) {
      if (el._slotThrottleTimer) {
        clearTimeout(el._slotThrottleTimer);
        el._slotThrottleTimer = null;
      }
      el._counterValue = target;
      applySlotMachineText(el, formatter(target), false);
      return;
    }

    const commitSlotUpdate = () => {
      el._slotLastUpdateAt = performance.now();
      const endVal = el._counterTarget ?? 0;
      el._counterValue = endVal;
      const fmt = el._counterFormatFn || formatter;
      applySlotMachineText(el, fmt(endVal), true);
    };

    const now = performance.now();
    const minIntervalMs = 150;
    const elapsedSinceLast = now - (el._slotLastUpdateAt || 0);

    if (elapsedSinceLast >= minIntervalMs) {
      if (el._slotThrottleTimer) {
        clearTimeout(el._slotThrottleTimer);
        el._slotThrottleTimer = null;
      }
      commitSlotUpdate();
    } else if (!el._slotThrottleTimer) {
      el._slotThrottleTimer = setTimeout(() => {
        el._slotThrottleTimer = null;
        commitSlotUpdate();
      }, minIntervalMs - elapsedSinceLast);
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

  const extractEventUsage = (ev) => {
    const isUser =
      (ev?.content?.role || (ev?.author === "user" ? "user" : "model")) === "user" ||
      ev?.author === "user";
    if (isUser) return null;

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

  const computeSessionUsage = (events) => {
    const deduped = [];
    const byId = new Map();
    for (const ev of Array.isArray(events) ? events : []) {
      if (!ev || typeof ev !== "object") continue;
      if (ev.id) {
        byId.set(ev.id, ev);
      } else {
        deduped.push(ev);
      }
    }
    const allEvents = [...byId.values(), ...deduped];

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
      const u = extractEventUsage(ev);
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
    const s = computeSessionUsage(currentEvents);
    const activeModel = prefs.model || DEFAULT_MODELS.model;
    const modelsList = s.models.size > 0 ? Array.from(s.models) : [activeModel];
    const primaryModel = modelsList[modelsList.length - 1] || activeModel;
    const rates = getModelPricing(primaryModel);
    const showSession = !!prefs.showSessionUsage;
    const showCost = !!prefs.showCost;

    if ($("session-usage-anchor")) {
      $("session-usage-anchor").classList.toggle("hidden", !showSession);
      if (!showSession && sessionUsagePopover) {
        sessionUsagePopover.classList.add("hidden");
      }
    }

    if (sessionUsageBtn) {
      const prevBtnTokens = sessionUsageBtn._lastSessionTokens;
      if (
        typeof prevBtnTokens === "number" &&
        s.totalTokens > prevBtnTokens &&
        showSession
      ) {
        triggerPulseClass(sessionUsageBtn, "usage-pill-bump");
      }
      sessionUsageBtn._lastSessionTokens = s.totalTokens;
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
    if (sessionUsageBtn) {
      const costSuffix = showCost ? ` · Est. ${formatUsdDetail(s.totalCost)}` : "";
      sessionUsageBtn.title = `Session usage: ${formatExactInt(s.totalTokens)} tokens (${formatExactInt(
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

  const rebuildFromSessionEvents = (events, { forceCanvas = false } = {}) => {
    currentEvents = Array.isArray(events) ? events : [];
    const prevSurfaceIds = canvasSurfaces.map((s) => s.id).join("|");
    const existingById = new Map(canvasSurfaces.map((s) => [s.id, s]));
    const nextSurfaces = [];

    currentEvents.forEach((ev, idx) => {
      const parts = ev?.content?.parts || [];
      let uiIndex = 0;
      parts.forEach((part) => {
        const uiResp = extractUiResponse(part);
        if (uiResp) {
          const surfaceId = `surface-${ev.id || idx}-${uiIndex++}`;
          const prev = existingById.get(surfaceId);
          if (prev) {
            nextSurfaces.push(prev);
          } else {
            const built = buildStandaloneCards(uiResp);
            nextSurfaces.push({
              id: surfaceId,
              title: built.title,
              eyebrow: built.eyebrow,
              raw: uiResp,
              cards: built.cards,
            });
          }
        }
      });
    });
    canvasSurfaces = nextSurfaces;
    const nextSurfaceIds = canvasSurfaces.map((s) => s.id).join("|");

    renderTranscript();
    if (forceCanvas || prevSurfaceIds !== nextSurfaceIds) {
      renderCanvas();
    }
  };

  const focusSurfaceOnCanvas = (surfaceId) => {
    activeSurfaceId = surfaceId;
    prefs.canvasMode = "focused";
    applyPreferences({ rerender: false });
    renderCanvas();
    const target = document.querySelector(`[data-surface-id="${surfaceId}"]`);
    if (target) {
      target.scrollIntoView({ behavior: "smooth", block: "start" });
    }
  };

  const createAvatarElement = (isUser, authorLabel) => {
    const avatar = document.createElement("span");
    avatar.className = `avatar ${isUser ? "avatar-user" : "avatar-agent"}`;
    avatar.title = authorLabel;
    if (isUser) {
      if (currentUser?.picture) {
        const img = document.createElement("img");
        img.src = currentUser.picture;
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
      avatar.innerHTML = `<svg viewBox="0 0 24 24" aria-hidden="true">
        <path d="M12 2.5C12 7.75 16.25 12 21.5 12C16.25 12 12 16.25 12 21.5C12 16.25 7.75 12 2.5 12C7.75 12 12 7.75 12 2.5Z" fill="currentColor" stroke="none"></path>
      </svg>`;
    }
    return avatar;
  };

  const updateMessageArticle = (
    article,
    { isUser, partial, textContent, visibleTools, surfaceRefs, usage }
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
      if (textEl.dataset.renderedText !== textContent) {
        textEl.dataset.renderedText = textContent;
        if (isUser) {
          textEl.textContent = textContent;
        } else {
          textEl.replaceChildren();
          parseMarkdownBlocksInto(textEl, textContent);
        }
      }
    } else if (textEl) {
      textEl.remove();
    }

    // 3. Tool calls container (updated only when tool calls changed)
    const toolsSig = visibleTools.length ? JSON.stringify(visibleTools) : "";
    if (article.dataset.toolsSig !== toolsSig) {
      article.dataset.toolsSig = toolsSig;
      article.querySelectorAll(":scope > details.tool-call").forEach((el) => el.remove());
      const beforeEl =
        article.querySelector(":scope > .canvas-jump-chip") ||
        article.querySelector(":scope > .message-usage");
      for (const tp of visibleTools) {
        const details = document.createElement("details");
        details.className = "tool-call";
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
        if (beforeEl) {
          article.insertBefore(details, beforeEl);
        } else {
          article.appendChild(details);
        }
      }
    }

    // 4. Canvas jump chips (updated only when surface refs changed)
    const surfacesSig = surfaceRefs.map((s) => s.id).join("|");
    if (article.dataset.surfacesSig !== surfacesSig) {
      article.dataset.surfacesSig = surfacesSig;
      article.querySelectorAll(":scope > .canvas-jump-chip").forEach((el) => el.remove());
      const usageFooter = article.querySelector(":scope > .message-usage");
      for (const surf of surfaceRefs) {
        const chip = document.createElement("button");
        chip.type = "button";
        chip.className = "canvas-jump-chip";
        const left = document.createElement("span");
        left.textContent = `✦ ${surf.eyebrow}: `;
        const strong = document.createElement("strong");
        strong.textContent = surf.title;
        left.appendChild(strong);
        const right = document.createElement("span");
        right.textContent = "View on Canvas →";
        chip.append(left, right);
        chip.addEventListener("click", () => focusSurfaceOnCanvas(surf.id));
        if (usageFooter) {
          article.insertBefore(chip, usageFooter);
        } else {
          article.appendChild(chip);
        }
      }
    }

    // 5. Per-turn token telemetry & cost footer (live during streaming and after completion)
    const usageSig =
      !isUser && usage && prefs.showMessageUsage
        ? `${usage.totalTokens}:${usage.inputTokens}:${usage.outputTokens}:${usage.thinkingTokens}:${usage.cacheReadTokens}:${prefs.showCost ? usage.totalCost.toFixed(6) : "nocost"}`
        : "";
    if (article.dataset.usageSig !== usageSig) {
      article.dataset.usageSig = usageSig;
      let usageEl = article.querySelector(":scope > .message-usage");
      if (usageSig && usage) {
        if (!usageEl) {
          usageEl = document.createElement("div");
          usageEl.className = "message-usage";
          usageEl.addEventListener("click", (e) => {
            e.stopPropagation();
            if (sessionUsagePopover && prefs.showSessionUsage) {
              settingsMenu.classList.add("hidden");
              userMenu.classList.add("hidden");
              updateSessionUsageUi();
              sessionUsagePopover.classList.remove("hidden");
            }
          });
          article.appendChild(usageEl);
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
            pulseOnMount: true,
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
          animateCounter(rightSpan, usage.totalCost, formatUsdShort, {
            pulseOnMount: true,
          });
        } else if (rightSpan) {
          rightSpan.remove();
        }
      } else if (usageEl) {
        usageEl.remove();
      }
    }
  };

  const renderTranscript = () => {
    updateSessionUsageUi();
    const visibleItems = [];

    currentEvents.forEach((ev, idx) => {
      const parts = ev?.content?.parts || [];
      const isUser =
        (ev?.content?.role || (ev?.author === "user" ? "user" : "model")) === "user" ||
        ev?.author === "user";

      const textContent = parts
        .map((p) => (typeof p.text === "string" ? p.text : ""))
        .filter(Boolean)
        .join("");

      const toolParts = [];
      const surfaceRefs = [];
      let uiIndex = 0;

      parts.forEach((part) => {
        const callName =
          part?.name || part?.functionCall?.name || part?.function_call?.name || "";
        if (callName && String(callName).startsWith("render_")) {
          return; // Skip raw `render_*` functionCall in chat
        }
        const uiResp = extractUiResponse(part);
        if (uiResp) {
          const surfaceId = `surface-${ev.id || idx}-${uiIndex++}`;
          const found = canvasSurfaces.find((s) => s.id === surfaceId);
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
        ? currentUser?.name || currentUser?.email || "You"
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
        usage: isUser ? null : extractEventUsage(ev),
      });
    });

    if (!visibleItems.length) {
      chatTranscript.replaceChildren();
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
      chatTranscript.appendChild(empty);
      return;
    }

    chatTranscript
      .querySelectorAll(":scope > .transcript-empty-placeholder")
      .forEach((el) => el.remove());

    const existingRows = Array.from(chatTranscript.querySelectorAll(":scope > .message-row"));

    visibleItems.forEach((item, idx) => {
      const roleKey = item.isUser ? "user" : "agent";
      let row = existingRows[idx];

      if (!row || row.dataset.roleKey !== roleKey || row.dataset.authorLabel !== item.authorLabel) {
        const newRow = document.createElement("div");
        newRow.className = `message-row ${item.isUser ? "message-row-user" : "message-row-agent"}`;
        newRow.dataset.roleKey = roleKey;
        newRow.dataset.authorLabel = item.authorLabel;

        const avatar = createAvatarElement(item.isUser, item.authorLabel);
        const article = document.createElement("article");
        article.className = `message ${item.isUser ? "message-user" : "message-agent"}`;
        newRow.append(avatar, article);

        if (row) {
          chatTranscript.replaceChild(newRow, row);
        } else {
          chatTranscript.appendChild(newRow);
        }
        row = newRow;
      }

      const article = row.querySelector(":scope > article.message");
      updateMessageArticle(article, item);
    });

    while (chatTranscript.children.length > visibleItems.length) {
      chatTranscript.lastElementChild.remove();
    }

    if (prefs.autoScroll) {
      chatTranscript.scrollTop = chatTranscript.scrollHeight;
    }
  };

  const renderCanvas = () => {
    canvasCountBadge.textContent = String(canvasSurfaces.length);
    canvasSurfacesEl.replaceChildren();
    canvasTabs.replaceChildren();

    if (canvasSurfaces.length === 0) {
      canvasEmpty.classList.remove("hidden");
      canvasTabs.classList.add("hidden");
      return;
    }

    if (prefs.canvasMode === "stacked") {
      activeSurfaceId = "all";
    } else if (
      activeSurfaceId === "all" ||
      !canvasSurfaces.some((s) => s.id === activeSurfaceId)
    ) {
      activeSurfaceId = canvasSurfaces[canvasSurfaces.length - 1].id;
    }

    canvasEmpty.classList.add("hidden");
    canvasTabs.classList.remove("hidden");

    const allBtn = document.createElement("button");
    allBtn.type = "button";
    allBtn.className = `canvas-tab ${prefs.canvasMode === "stacked" ? "active" : ""}`;
    allBtn.textContent = `All Cards (${canvasSurfaces.length})`;
    allBtn.addEventListener("click", () => {
      activeSurfaceId = "all";
      prefs.canvasMode = "stacked";
      applyPreferences({ rerender: false });
      renderCanvas();
    });
    canvasTabs.appendChild(allBtn);

    canvasSurfaces.forEach((surf) => {
      const tab = document.createElement("button");
      tab.type = "button";
      tab.className = `canvas-tab ${prefs.canvasMode === "focused" && activeSurfaceId === surf.id ? "active" : ""}`;
      tab.textContent = `${surf.eyebrow}: ${surf.title}`;
      tab.addEventListener("click", () => {
        focusSurfaceOnCanvas(surf.id);
      });
      canvasTabs.appendChild(tab);
    });

    const visibleSurfaces =
      prefs.canvasMode === "focused" && activeSurfaceId !== "all"
        ? canvasSurfaces.filter((s) => s.id === activeSurfaceId)
        : canvasSurfaces;

    for (const surf of visibleSurfaces) {
      const group = document.createElement("div");
      group.setAttribute("data-surface-id", surf.id);
      style(group, { display: "flex", flexDirection: "column", gap: "14px" });
      for (const card of surf.cards) {
        group.appendChild(card);
      }
      canvasSurfacesEl.appendChild(group);
    }
  };

  exportUiBtn.addEventListener("click", () => {
    if (!canvasSurfaces.length) return;
    const payload = canvasSurfaces.map((s) => s.raw);
    const blob = new Blob([JSON.stringify(payload, null, 2)], { type: "application/json" });
    const url = URL.createObjectURL(blob);
    const a = document.createElement("a");
    a.href = url;
    a.download = `adspace-ui-${activeSession?.id?.slice(0, 8) || "canvas"}.json`;
    a.click();
    URL.revokeObjectURL(url);
  });

  clearUiBtn.addEventListener("click", () => {
    canvasSurfaces = [];
    renderCanvas();
  });

  // Starter cards on empty canvas
  document.querySelectorAll(".starter-card").forEach((card) => {
    card.addEventListener("click", () => {
      const prompt = card.getAttribute("data-prompt");
      if (prompt) sendMessage(prompt);
    });
  });

  // ---------------------------------------------------------------------------
  // Sending Messages & SSE Streaming (`POST /api/run_sse`)
  // ---------------------------------------------------------------------------

  const renderAttachments = () => {
    attachmentChips.replaceChildren();
    if (!attachments.length) {
      attachmentChips.classList.add("hidden");
      return;
    }
    attachmentChips.classList.remove("hidden");
    attachments.forEach((file, idx) => {
      const chip = document.createElement("button");
      chip.type = "button";
      const nameSpan = document.createElement("span");
      nameSpan.textContent = file.name;
      const closeB = document.createElement("b");
      closeB.textContent = "×";
      chip.append(nameSpan, " ", closeB);
      chip.addEventListener("click", () => {
        attachments.splice(idx, 1);
        renderAttachments();
      });
      attachmentChips.appendChild(chip);
    });
  };

  fileInput.addEventListener("change", () => {
    const files = Array.from(fileInput.files || []);
    attachments.push(...files);
    fileInput.value = "";
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
      buffer += decoder.decode(value, { stream: true }).replace(/\r\n/g, "\n");
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

  const sendMessage = async (overrideText) => {
    if (isRunning) {
      if (abortController) abortController.abort();
      return;
    }
    if (!currentUser || !activeSession) return;

    const rawText =
      typeof overrideText === "string" ? overrideText.trim() : chatInput.value.trim();
    if (!rawText && attachments.length === 0) return;

    let finalMessage = rawText;
    if (
      (prefs.customerId || prefs.loginCustomerId) &&
      !rawText.startsWith("[UI Event:") &&
      !/\b\d{10}\b/.test(rawText)
    ) {
      const ctxParts = [];
      if (prefs.customerId) ctxParts.push(`Customer ID=${prefs.customerId}`);
      if (prefs.loginCustomerId) ctxParts.push(`Login Customer ID=${prefs.loginCustomerId}`);
      finalMessage = `${rawText}\n\n[Default Google Ads Account Context: ${ctxParts.join(", ")}]`;
    }

    const parts = [];
    if (finalMessage) parts.push({ text: finalMessage });

    for (const file of attachments) {
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

    if (typeof overrideText !== "string") {
      chatInput.value = "";
      syncComposerHeight();
    }
    attachments = [];
    renderAttachments();

    // Optimistically append user message
    const userEvent = {
      id: `local-user-${Date.now()}`,
      author: "user",
      content: { role: "user", parts: [{ text: rawText || "(attachment)" }] },
    };
    currentEvents.push(userEvent);
    renderTranscript();

    isRunning = true;
    setStatus("running", "streaming");
    sendBtn.classList.add("stop-button");
    abortController = new AbortController();

    try {
      const resp = await fetch(apiUrl("/run_sse"), {
        method: "POST",
        signal: abortController.signal,
        headers: {
          "Content-Type": "application/json",
          "x-adk-ui-protocol": "a2ui",
        },
        body: JSON.stringify({
          appName: APP_NAME,
          userId: currentUser.id,
          sessionId: activeSession.id,
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
          if (!dataStr || dataStr === "[DONE]") continue;
          try {
            const parsed = JSON.parse(dataStr);
            const ev = parsed.event ?? parsed;
            if (ev && typeof ev === "object") {
              if (ev.error_message || ev.errorMessage) {
                showError(`Agent error: ${ev.error_message || ev.errorMessage}`);
              }
              const existingIdx = currentEvents.findIndex((e) => e.id && e.id === ev.id);
              if (existingIdx !== -1) {
                currentEvents[existingIdx] = mergeStreamedEvent(currentEvents[existingIdx], ev);
              } else {
                currentEvents.push({
                  ...ev,
                  content: ev.content
                    ? { ...ev.content, parts: coalesceParts(ev.content.parts) }
                    : ev.content,
                });
              }
              rebuildFromSessionEvents(currentEvents);
            }
          } catch {
            /* ignore partial JSON */
          }
        }
      }
      await loadSession(activeSession.id);
    } catch (err) {
      if (err.name !== "AbortError") {
        showError(`Agent request failed: ${err.message || err}`);
      }
    } finally {
      isRunning = false;
      abortController = null;
      sendBtn.classList.remove("stop-button");
      setStatus("ready", "ready");
    }
  };

  const syncComposerHeight = () => {
    chatInput.style.height = "36px";
    const next = Math.min(160, Math.max(36, chatInput.scrollHeight));
    chatInput.style.height = `${next}px`;
  };

  chatInput.addEventListener("input", syncComposerHeight);
  sendBtn.addEventListener("click", () => sendMessage());
  chatInput.addEventListener("keydown", (e) => {
    if (e.key === "Enter" && !e.shiftKey) {
      e.preventDefault();
      sendMessage();
    }
  });

  // ---------------------------------------------------------------------------
  // Popovers, Settings Controls & Split-Pane Resizer
  // ---------------------------------------------------------------------------

  settingsBtn.addEventListener("click", (e) => {
    e.stopPropagation();
    userMenu.classList.add("hidden");
    if (sessionUsagePopover) sessionUsagePopover.classList.add("hidden");
    settingsMenu.classList.toggle("hidden");
  });

  userBtn.addEventListener("click", (e) => {
    e.stopPropagation();
    settingsMenu.classList.add("hidden");
    if (sessionUsagePopover) sessionUsagePopover.classList.add("hidden");
    userMenu.classList.toggle("hidden");
  });

  if (sessionUsageBtn && sessionUsagePopover) {
    sessionUsageBtn.addEventListener("click", (e) => {
      e.stopPropagation();
      settingsMenu.classList.add("hidden");
      userMenu.classList.add("hidden");
      updateSessionUsageUi();
      sessionUsagePopover.classList.toggle("hidden");
    });
  }

  document.addEventListener("click", (e) => {
    if (!settingsMenu.contains(e.target) && e.target !== settingsBtn) {
      settingsMenu.classList.add("hidden");
    }
    if (!userMenu.contains(e.target) && e.target !== userBtn) {
      userMenu.classList.add("hidden");
    }
    if (
      sessionUsagePopover &&
      sessionUsageBtn &&
      !sessionUsagePopover.contains(e.target) &&
      !sessionUsageBtn.contains(e.target)
    ) {
      sessionUsagePopover.classList.add("hidden");
    }
  });

  errorBannerClose.addEventListener("click", () => showError(null));

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

  $("setting-customer-id").addEventListener("input", (e) => {
    prefs.customerId = e.target.value.replace(/\D/g, "");
    applyPreferences({ rerender: false });
  });

  $("setting-login-customer-id").addEventListener("input", (e) => {
    prefs.customerId = $("setting-customer-id").value.replace(/\D/g, "");
    prefs.loginCustomerId = e.target.value.replace(/\D/g, "");
    applyPreferences({ rerender: false });
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

  const commitCustomPricingInputs = () => {
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

    applyPreferences({ syncPricing: false });
  };

  ["setting-price-input", "setting-price-cached", "setting-price-output"].forEach((id) => {
    const el = $(id);
    if (!el) return;
    el.addEventListener("input", commitCustomPricingInputs);
    el.addEventListener("change", () => {
      commitCustomPricingInputs();
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

  // Split-Pane Resizer
  let isResizing = false;
  paneResizer.addEventListener("mousedown", (e) => {
    e.preventDefault();
    isResizing = true;
    paneResizer.classList.add("dragging");
  });
  window.addEventListener("mousemove", (e) => {
    if (!isResizing) return;
    const pct = Math.min(68, Math.max(28, (e.clientX / window.innerWidth) * 100));
    const val = `${pct.toFixed(1)}%`;
    document.documentElement.style.setProperty("--left-pane-width", val);
    localStorage.setItem(STORAGE_KEYS.leftWidth, val);
  });
  window.addEventListener("mouseup", () => {
    if (isResizing) {
      isResizing = false;
      paneResizer.classList.remove("dragging");
    }
  });

  // Initialize
  applyPreferences();
  checkAuthStatus();
})();
