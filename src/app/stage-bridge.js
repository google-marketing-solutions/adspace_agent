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
// Interactive Canvas Stage (Argon Dotted Grid WebGL Stage)
// ---------------------------------------------------------------------------
import { $ } from "./dom.js";
import { els, prefs } from "./state.js";
import { prefersReducedMotion } from "./usage/counter.js";
import { AdSpaceStage } from "./stage.js";

let authStageInstance = null;
let canvasStageInstance = null;
let stageAbort = null;

export const STAGE_CONFIG = {
  dark_mode_preset: "argon-dark",
  light_mode_preset: "argon-light",
  grid_step: 24,
  dot_radius: 1.4,
  hover_reach: 155,
  hover_push: 7.5,
  hover_swirl: 2.2,
  hover_scale: 1.35,
};

export const getEffectiveColorMode = () => {
  if (prefs.theme === "dark") return "dark";
  if (prefs.theme === "light") return "light";
  return window.matchMedia?.("(prefers-color-scheme: dark)")?.matches ? "dark" : "light";
};

export function syncCanvasStageTheme() {
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
export const STAGE_ACTIVITY_BY_STATUS = { running: "streaming", authorizing: "authorizing" };
export const currentStatusKind = () => {
  const match = /\bstatus-([a-z]+)/.exec(els.statusPill.className || "");
  return match ? match[1] : "ready";
};
export function syncStageStreaming(kind = currentStatusKind()) {
  const activity = STAGE_ACTIVITY_BY_STATUS[kind] || "idle";
  if (canvasStageInstance && typeof canvasStageInstance.update === "function") {
    if (canvasStageInstance.activity !== activity) {
      canvasStageInstance.update({ activity });
    }
  }
}

export function syncActiveStage() {
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

export const mountStageOn = async (stageEl, signal) => {
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

export const initCanvasStage = async () => {
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
