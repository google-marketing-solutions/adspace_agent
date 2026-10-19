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
// OAuth Popup Management (Sign-In & In-Flight Tool Consent)
// ---------------------------------------------------------------------------

import { state } from "./state.js";
import { setStatus } from "./status.js";
import { loadSession } from "./sessions.js";
import { checkAuthStatus, openPopup } from "./auth.js";

export const CHANNEL = "adspace:oauth";
export const EVENTS_PATH = "/oauth2/events";
export const ACK_PATH = "/oauth2/ack";

export const handleAuthRequired = async (data) => {
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

export const handleOAuthCompletion = async (status) => {
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
