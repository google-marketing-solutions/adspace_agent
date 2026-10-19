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

import { onStatusChange } from "./status.js";
import { applyPreferences, bindPreferenceRenderers, syncSettingsAvailability } from "./prefs.js";
import { initCanvasStage, syncStageStreaming } from "./stage-bridge.js";
import { bindModalComponentRenderer } from "./adk-ui/modal.js";
import { bindSendMessage, renderInnerComponent } from "./adk-ui/components.js";
import { updateSessionUsageUi } from "./usage/session-usage.js";
import { renderCanvas, renderTranscript } from "./transcript.js";
import { checkAuthStatus } from "./auth.js";
import { sendMessage } from "./sse.js";
// Side-effect-only modules (they register their own listeners at load time).
import "./oauth-popup.js";
import "./settings-ui.js";

// Late bindings that break import cycles (each setter is documented at its
// definition): low modules call up into higher ones through these hooks.
onStatusChange(syncStageStreaming);
bindPreferenceRenderers({ renderTranscript, renderCanvas, updateSessionUsageUi });
bindModalComponentRenderer(renderInnerComponent);
bindSendMessage(sendMessage);

// Initialize
applyPreferences();
syncSettingsAvailability();
initCanvasStage();
checkAuthStatus();
