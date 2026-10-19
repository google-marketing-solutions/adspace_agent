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
// Popovers, Settings Controls & Split-Pane Resizer
// ---------------------------------------------------------------------------

import { $, bindPopoverTrigger, closePopover, isPopoverOpen, openPopover } from "./dom.js";
import { DEFAULT_MODELS, els, prefs, STORAGE_KEYS } from "./state.js";
import { showError } from "./status.js";
import {
  getDefaultModelPricing,
  getModelPricingKey,
  syncPricingControls,
} from "./usage/pricing.js";
import { applyPreferences, pushModelsToServer } from "./prefs.js";
import { updateSessionUsageUi } from "./usage/session-usage.js";

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

export const bindModelPicker = (prefKey, selectId, customId, defaultVal) => {
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

export const commitCustomPricingInputs = ({ rerender = true } = {}) => {
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
export const LEFT_PANE_MIN_PCT = 28;
export const LEFT_PANE_MAX_PCT = 68;
export const clampLeftPanePct = (pct) =>
  Math.min(LEFT_PANE_MAX_PCT, Math.max(LEFT_PANE_MIN_PCT, pct));
export const readLeftPanePct = () => {
  const raw = getComputedStyle(document.documentElement).getPropertyValue("--left-pane-width");
  const n = parseFloat(raw);
  return Number.isFinite(n) ? clampLeftPanePct(n) : 44;
};
let leftPanePct = readLeftPanePct();
export const currentLeftPanePct = () => leftPanePct;
export const setLeftPanePct = (pct, { persist = false } = {}) => {
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
