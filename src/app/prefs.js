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

// ---------------------------------------------------------------------------
// Settings & Theme Management
// ---------------------------------------------------------------------------

import { $ } from "./dom.js";
import { els, prefs, PRESET_MODEL_OPTIONS, state, STORAGE_KEYS } from "./state.js";
import { showError } from "./status.js";
import { ingestServerPricing, syncPricingControls } from "./usage/pricing.js";
import { syncCanvasStageTheme } from "./stage-bridge.js";

export const syncModelControl = (selectEl, customEl, presets, value, forceCustom = false) => {
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

export const MODEL_SELECT_IDS = {
  model: "setting-model-select",
  imageModel: "setting-image-model-select",
  videoModel: "setting-video-model-select",
};

// `PRESET_MODEL_OPTIONS` is the single source of truth for the preset lists:
// the <option>s in studio.html are replaced here so the two cannot drift
// (drift would make `syncModelControl` silently fall back to "Custom…").
export const populateModelSelects = () => {
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

export const syncAllModelControls = () => {
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

export const pushModelsToServer = async () => {
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
export const settingEls = {
  themeButtons: Array.from(document.querySelectorAll("#theme-segmented button")),
  canvasModeButtons: Array.from(document.querySelectorAll("#canvas-mode-segmented button")),
  showTools: $("setting-show-tools"),
  showSessionUsage: $("setting-show-session-usage"),
  showMessageUsage: $("setting-show-message-usage"),
  showCost: $("setting-show-cost"),
  autoScroll: $("setting-autoscroll"),
};

// Writes every preference to localStorage. Pure persistence, no DOM work.
export const persistPrefs = () => {
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

export const syncSegmented = (buttons, dataKey, value) => {
  for (const btn of buttons) {
    const on = btn.dataset[dataKey] === value;
    btn.classList.toggle("active", on);
    btn.setAttribute("aria-pressed", on ? "true" : "false");
  }
};

// Reflects `prefs` into the settings popover controls and the composer hint.
export const syncPrefControls = ({ syncModels = true, syncPricing = true } = {}) => {
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

// Late binding: transcript.js and usage/session-usage.js sit above this module
// (they read `prefs`), so app.js injects the re-render entry points via
// `bindPreferenceRenderers` instead of importing them here.
let renderTranscript = null;
let renderCanvas = null;
let updateSessionUsageUi = null;
export const bindPreferenceRenderers = (fns) => {
  ({ renderTranscript, renderCanvas, updateSessionUsageUi } = fns);
};

// Entry point used throughout the client: persist, reflect into the controls,
// apply the theme, then re-render (or at least refresh the usage UI).
export const applyPreferences = ({
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

export const savedWidth = localStorage.getItem(STORAGE_KEYS.leftWidth);
if (savedWidth) {
  document.documentElement.style.setProperty("--left-pane-width", savedWidth);
}

export function syncSettingsAvailability() {
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
