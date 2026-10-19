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

// `adk-ui` modal overlay (dialog / confirm cards) with focus trap and inert host content.

import { $, createIcon, focusableIn, style } from "../dom.js";

let activeModalCleanup = null;

// Host binding: where the overlay mounts and what becomes `inert` while it is
// open. Defaults suit a page that ships `#ui-modal-root` (studio.html); the
// studio passes its workspace as `inertTarget`, `/ui/` passes the React root.
let modalHost = { root: null, inertTarget: null };
export const bindModalHost = ({ root = null, inertTarget = null } = {}) => {
  modalHost = { root, inertTarget };
};
const modalRoot = () => modalHost.root || $("ui-modal-root");

// Late binding: `renderInnerComponent` (adk-ui/components.js) sits above this
// module and itself calls `closeModalOverlay`, so app.js injects it via
// `bindModalComponentRenderer` instead of importing it here.
let renderInnerComponent = null;
export const bindModalComponentRenderer = (fn) => {
  renderInnerComponent = fn;
};

export const closeModalOverlay = () => {
  const root = modalRoot();
  if (activeModalCleanup) {
    activeModalCleanup();
    activeModalCleanup = null;
  }
  if (root) {
    root.classList.add("hidden");
    root.replaceChildren();
  }
};

export const showModalOverlay = (spec, { isConfirm = false } = {}) => {
  const root = modalRoot();
  if (!root || !spec) return;
  closeModalOverlay();

  const closable = spec.closable !== false;
  const rawSize = String(spec.size || (isConfirm ? "small" : "medium")).toLowerCase();
  const size = ["small", "medium", "large", "full"].includes(rawSize) ? rawSize : "medium";

  const backdrop = document.createElement("div");
  backdrop.className = "ui-modal-backdrop";
  if (closable) {
    backdrop.addEventListener("click", closeModalOverlay);
  }

  const dialog = document.createElement("div");
  dialog.className = `ui-modal-dialog ui-modal--${size}`;
  dialog.setAttribute("data-ui-scope", "true");
  dialog.setAttribute("data-ui-modal", "true");

  const hdr = document.createElement("header");
  hdr.className = "ui-modal-header";
  const titleWrap = document.createElement("div");
  titleWrap.className = "ui-modal-title-wrap";

  const hasDangerAction =
    isConfirm &&
    Array.isArray(spec.footer) &&
    spec.footer.some((b) => String(b?.variant || "").toLowerCase() === "danger");

  if (hasDangerAction) {
    const warnChip = document.createElement("span");
    style(warnChip, {
      color: "var(--accent-error)",
      width: "34px",
      height: "34px",
      borderRadius: "50%",
      background: "var(--dm-surface-gray-subtle)",
      display: "inline-flex",
      alignItems: "center",
      justifyContent: "center",
      flexShrink: "0",
    });
    warnChip.appendChild(createIcon("warning", { filled: true, size: 19 }));
    titleWrap.appendChild(warnChip);
  }

  const titleEl = document.createElement("h3");
  titleEl.className = "ui-modal-title";
  titleEl.textContent = spec.title || (isConfirm ? "Confirm Action" : "Details");
  titleWrap.appendChild(titleEl);
  hdr.appendChild(titleWrap);

  if (closable) {
    const closeBtn = document.createElement("button");
    closeBtn.type = "button";
    closeBtn.className = "ui-modal-close";
    closeBtn.setAttribute("aria-label", "Close dialog");
    closeBtn.appendChild(createIcon("close", { size: 18 }));
    closeBtn.addEventListener("click", closeModalOverlay);
    hdr.appendChild(closeBtn);
  }

  const body = document.createElement("div");
  body.className = "ui-modal-body";
  const contentArr = Array.isArray(spec.content) ? spec.content : [];
  for (const child of contentArr) {
    body.appendChild(renderInnerComponent(child, dialog, { inModal: true }));
  }

  dialog.append(hdr, body);

  if (Array.isArray(spec.footer) && spec.footer.length) {
    const ftr = document.createElement("footer");
    ftr.className = "ui-modal-footer";
    for (const child of spec.footer) {
      ftr.appendChild(renderInnerComponent(child, dialog, { inModal: true }));
    }
    dialog.appendChild(ftr);
  }

  const onKeyDown = (ev) => {
    if (ev.key === "Escape" && closable) {
      closeModalOverlay();
      return;
    }
    // Keep Tab / Shift+Tab cycling inside the dialog.
    if (ev.key === "Tab") {
      const focusables = focusableIn(dialog);
      if (!focusables.length) {
        ev.preventDefault();
        dialog.focus();
        return;
      }
      const first = focusables[0];
      const last = focusables[focusables.length - 1];
      const active = document.activeElement;
      if (ev.shiftKey && (active === first || !dialog.contains(active))) {
        ev.preventDefault();
        last.focus();
      } else if (!ev.shiftKey && (active === last || !dialog.contains(active))) {
        ev.preventDefault();
        first.focus();
      }
    }
  };
  window.addEventListener("keydown", onKeyDown);

  // Focus management: remember the opener, make the host content inert, move
  // focus into the dialog, and restore it all in `closeModalOverlay`.
  const previouslyFocused = document.activeElement;
  const { inertTarget } = modalHost;
  titleEl.id = "ui-modal-title";
  root.setAttribute("aria-labelledby", "ui-modal-title");
  dialog.tabIndex = -1;
  if (inertTarget) inertTarget.inert = true;
  activeModalCleanup = () => {
    window.removeEventListener("keydown", onKeyDown);
    if (inertTarget) inertTarget.inert = false;
    root.removeAttribute("aria-labelledby");
    if (previouslyFocused && typeof previouslyFocused.focus === "function") {
      try {
        previouslyFocused.focus();
      } catch {
        /* ignore */
      }
    }
  };

  root.replaceChildren(backdrop, dialog);
  root.classList.remove("hidden");
  // Prefer the first form control; fall back to the close button, then the
  // dialog itself, so keyboard users are never left on the inert page.
  const initialFocus =
    focusableIn(body)[0] || dialog.querySelector(".ui-modal-close") || dialog;
  initialFocus.focus();
};
