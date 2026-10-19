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

// Status pill + error banner setters shared by every module.

import { els } from "./state.js";

export const showError = (msg) => {
  if (!msg) {
    els.errorBanner.classList.add("hidden");
    return;
  }
  els.errorBannerText.textContent = msg;
  els.errorBanner.classList.remove("hidden");
};

// Lets the canvas stage react to the status (streaming wave while the
// agent is running). Bound once the stage instances exist, see
// `syncStageStreaming` in stage-bridge.js.
// Late binding: stage-bridge.js sits above this module, so app.js injects
// `syncStageStreaming` via `onStatusChange(fn)` instead of importing it here.
let statusListener = null;
export const onStatusChange = (fn) => {
  statusListener = fn;
};

export const setStatus = (kind, label) => {
  els.statusPill.className = `status status-${kind}`;
  els.statusText.textContent = label || kind;
  if (typeof statusListener === "function") statusListener(kind);
};
