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
// ADK Session Management (Scoped to `currentUser.id`)
// ---------------------------------------------------------------------------

import { $ } from "./dom.js";
import { APP_NAME, els, state } from "./state.js";
import { showError } from "./status.js";
import { syncSettingsAvailability } from "./prefs.js";
import { closeModalOverlay } from "./adk-ui/modal.js";
import { rebuildFromSessionEvents } from "./transcript.js";

export const apiUrl = (path) => `/api${path}`;

export const loadUserSessions = async () => {
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

export const renderSessionOptions = (selectedId) => {
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

export const createNewSession = () => {
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

export const loadSession = async (sessionId) => {
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
