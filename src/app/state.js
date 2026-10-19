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

// Shared mutable application state, persisted preferences and the DOM
// element handles every module reads. ES module `let` exports are read-only
// to importers, so the mutable singletons live on the `state` object.

import { $ } from "./dom.js";

export const APP_NAME = "adspace_agent";

export const STORAGE_KEYS = {
  theme: "adspace.studio.theme",
  canvasMode: "adspace.studio.canvasMode",
  showTools: "adspace.studio.showTools",
  showSessionUsage: "adspace.studio.showSessionUsage",
  showMessageUsage: "adspace.studio.showMessageUsage",
  showCost: "adspace.studio.showCost",
  autoScroll: "adspace.studio.autoScroll",
  leftWidth: "adspace.studio.leftWidth",
  model: "adspace.studio.model",
  imageModel: "adspace.studio.imageModel",
  videoModel: "adspace.studio.videoModel",
  pricingTier: "adspace.studio.pricingTier",
  modelPricing: "adspace.studio.modelPricing",
};

export const DEFAULT_MODELS = {
  model: "gemini-3.8-flash",
  imageModel: "gemini-3.1-flash-image",
  videoModel: "veo-3.1-fast-generate-001",
};

export const PRESET_MODEL_OPTIONS = {
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

// Application State
export const state = {
  currentUser: null,
  sessions: [],
  activeSession: null,
  isRunning: false,
  abortController: null,
  attachments: [],
  canvasSurfaces: [], // [{ id, title, eyebrow, raw, cards }]
  activeSurfaceId: "all",
  popup: null,
  handledAuthRequests: new Set(),
  // Whether the signed-in user may hot-swap the global models (`/auth/me` → `is_admin`).
  // Absent (e.g. OAuth unconfigured) is treated as allowed.
  canManageModels: true,
  // Per-model Paid Tier Standard rates served by the backend (`config::model_pricing`)
  // in the `/auth/models` payload, keyed by lower-cased model name. The server is the
  // source of truth; the local heuristics in `getDefaultModelPricing` are only a
  // fallback for models the server did not price (or older servers without `pricing`).
  serverModelPricing: {},
  currentEvents: [],
};

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
export let prefs = {
  theme: localStorage.getItem(STORAGE_KEYS.theme) || "system",
  canvasMode: localStorage.getItem(STORAGE_KEYS.canvasMode) || "stacked",
  showTools: localStorage.getItem(STORAGE_KEYS.showTools) !== "false",
  showSessionUsage: localStorage.getItem(STORAGE_KEYS.showSessionUsage) !== "false",
  showMessageUsage: localStorage.getItem(STORAGE_KEYS.showMessageUsage) !== "false",
  showCost: localStorage.getItem(STORAGE_KEYS.showCost) !== "false",
  autoScroll: localStorage.getItem(STORAGE_KEYS.autoScroll) === "true",
  model: localStorage.getItem(STORAGE_KEYS.model) || DEFAULT_MODELS.model,
  imageModel: localStorage.getItem(STORAGE_KEYS.imageModel) || DEFAULT_MODELS.imageModel,
  videoModel: localStorage.getItem(STORAGE_KEYS.videoModel) || DEFAULT_MODELS.videoModel,
  pricingTier: localStorage.getItem(STORAGE_KEYS.pricingTier) || "standard",
  modelPricing: loadStoredModelPricing(),
};

// DOM Elements
export const els = {
  statusPill: $("status-pill"),
  statusText: $("status-text"),
  sessionControls: $("session-controls"),
  sessionSelect: $("session-select"),
  newSessionBtn: $("new-session-btn"),
  settingsBtn: $("settings-btn"),
  settingsMenu: $("settings-menu"),
  userMenuAnchor: $("user-menu-anchor"),
  userBtn: $("user-btn"),
  userMenu: $("user-menu"),
  logoutBtn: $("logout-btn"),
  deleteSessionBtn: $("delete-session-btn"),
  errorBanner: $("error-banner"),
  errorBannerText: $("error-banner-text"),
  errorBannerClose: $("error-banner-close"),
  authGate: $("auth-gate"),
  authGateHint: $("auth-gate-hint"),
  googleLoginBtn: $("google-login-btn"),
  workspace: $("workspace"),
  sessionUsageBtn: $("session-usage-btn"),
  sessionUsagePopover: $("session-usage-popover"),
  refreshSessionBtn: $("refresh-session-btn"),
  chatTranscript: $("chat-transcript"),
  attachmentChips: $("attachment-chips"),
  fileInput: $("file-input"),
  chatInput: $("chat-input"),
  sendBtn: $("send-btn"),
  composerContextHint: $("composer-context-hint"),
  composerUserHint: $("composer-user-hint"),
  paneResizer: $("pane-resizer"),
  canvasTabs: $("canvas-tabs"),
  canvasEmpty: $("canvas-empty"),
  canvasSurfacesEl: $("canvas-surfaces"),
  exportUiBtn: $("export-ui-btn"),
  clearUiBtn: $("clear-ui-btn"),
};
