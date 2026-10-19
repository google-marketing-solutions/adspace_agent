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
// User Authentication (`/auth/me`, `/auth/login`, `/auth/logout`)
// ---------------------------------------------------------------------------

import { $ } from "./dom.js";
import { initialsOf } from "./format.js";
import { els, prefs, state } from "./state.js";
import { showError } from "./status.js";
import { ingestServerPricing } from "./usage/pricing.js";
import { applyPreferences, syncSettingsAvailability } from "./prefs.js";
import { syncActiveStage } from "./stage-bridge.js";
import { loadUserSessions } from "./sessions.js";

export const openPopup = (url, name = "adspace_google_auth") => {
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

export const setAvatarImage = (imgEl, fallbackEl, user) => {
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

export const checkAuthStatus = () => {
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
