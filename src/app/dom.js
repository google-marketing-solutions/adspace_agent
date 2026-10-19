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

// DOM helpers shared by every module. No app state.

import { variantColors, variantIconName } from "./format.js";

export const SVG_NS = "http://www.w3.org/2000/svg";

export const $ = (id) => document.getElementById(id);

export const style = (el, props) => {
  for (const [k, v] of Object.entries(props)) el.style[k] = v;
};

export const svgEl = (tag, attrs = {}) => {
  const el = document.createElementNS(SVG_NS, tag);
  for (const [k, v] of Object.entries(attrs)) {
    if (v !== undefined && v !== null) el.setAttribute(k, String(v));
  }
  if (tag === "text") el.setAttribute("stroke", "none");
  return el;
};

// Popovers are plain `.hidden`-toggled cards. `aria-expanded` on each
// trigger is kept in sync by observing the popover's class attribute, so
// every code path that hides a popover (outside click, settings toggles,
// other popovers opening) stays consistent without threading a helper
// through all of them. Focus moves into the popover on open and returns to
// the trigger on close (including Escape).
export const POPOVER_FOCUSABLE_SELECTOR =
  'button:not([disabled]), [href], input:not([disabled]):not([type="hidden"]), select:not([disabled]), textarea:not([disabled]), [tabindex]:not([tabindex="-1"])';

export const isPopoverOpen = (popover) => Boolean(popover && !popover.classList.contains("hidden"));

export const openPopover = (trigger, popover) => {
  if (!popover || isPopoverOpen(popover)) return;
  popover.classList.remove("hidden");
  const first = popover.querySelector(POPOVER_FOCUSABLE_SELECTOR);
  if (first) {
    first.focus();
  } else {
    if (!popover.hasAttribute("tabindex")) popover.setAttribute("tabindex", "-1");
    popover.focus();
  }
};

export const closePopover = (trigger, popover) => {
  if (!popover || !isPopoverOpen(popover)) return;
  const hadFocus = popover.contains(document.activeElement);
  popover.classList.add("hidden");
  if (hadFocus && trigger && typeof trigger.focus === "function") trigger.focus();
};

export const bindPopoverTrigger = (trigger, popover) => {
  if (!trigger || !popover) return;
  if (popover.id && !trigger.hasAttribute("aria-controls")) {
    trigger.setAttribute("aria-controls", popover.id);
  }
  const syncExpanded = () =>
    trigger.setAttribute("aria-expanded", String(isPopoverOpen(popover)));
  syncExpanded();
  if (typeof MutationObserver === "function") {
    new MutationObserver(syncExpanded).observe(popover, {
      attributes: true,
      attributeFilter: ["class"],
    });
  }
};

export const FOCUSABLE_SELECTOR =
  'a[href], button:not([disabled]), input:not([disabled]):not([type="hidden"]), ' +
  'select:not([disabled]), textarea:not([disabled]), [tabindex]:not([tabindex="-1"])';

export const focusableIn = (container) =>
  container
    ? Array.from(container.querySelectorAll(FOCUSABLE_SELECTOR)).filter(
        (el) => !el.closest(".hidden") && el.getClientRects().length > 0
      )
    : [];

export const showToast = (comp) => {
  const stack = $("toast-stack");
  if (!stack || !comp) return;
  const vc = variantColors(comp.variant);
  const toast = document.createElement("div");
  toast.className = "ui-toast";
  toast.setAttribute("role", "status");

  const iconWrap = document.createElement("span");
  iconWrap.className = "ui-toast-icon";
  style(iconWrap, { color: vc.fg });
  const icon = document.createElement("span");
  icon.className = "google-symbols filled";
  icon.setAttribute("aria-hidden", "true");
  icon.textContent = variantIconName(comp.variant);
  iconWrap.appendChild(icon);

  const body = document.createElement("div");
  body.className = "ui-toast-body";
  const title = document.createElement("div");
  title.className = "ui-toast-title";
  title.textContent = comp.message || comp.title || "Notification";
  body.appendChild(title);
  if (comp.description) {
    const desc = document.createElement("div");
    desc.className = "ui-toast-desc";
    desc.textContent = comp.description;
    body.appendChild(desc);
  }

  toast.append(iconWrap, body);

  let dismissed = false;
  let timerId = 0;
  const requestedMs = Number(comp.duration);
  let remainingMs = Number.isFinite(requestedMs) ? Math.max(0, requestedMs) : 5000;
  let startedAt = 0;

  const dismiss = () => {
    if (dismissed) return;
    dismissed = true;
    if (timerId) clearTimeout(timerId);
    toast.classList.add("leaving");
    setTimeout(() => toast.remove(), 190);
  };

  // A toast that never auto-dismisses must always be closable by hand.
  if (comp.dismissible !== false || remainingMs === 0) {
    const closeBtn = document.createElement("button");
    closeBtn.type = "button";
    closeBtn.className = "ui-toast-close";
    closeBtn.setAttribute("aria-label", "Dismiss notification");
    closeBtn.innerHTML = "&times;";
    closeBtn.addEventListener("click", dismiss);
    toast.appendChild(closeBtn);
  }

  if (remainingMs > 0) {
    const bar = document.createElement("div");
    bar.className = "ui-toast-timer";
    style(bar, {
      background: vc.fg,
      animationDuration: `${remainingMs}ms`,
    });
    toast.appendChild(bar);

    const startTimer = () => {
      if (dismissed || remainingMs <= 0) return;
      startedAt = performance.now();
      timerId = window.setTimeout(dismiss, remainingMs);
    };

    const pauseTimer = () => {
      if (dismissed || !timerId) return;
      clearTimeout(timerId);
      timerId = 0;
      remainingMs = Math.max(0, remainingMs - (performance.now() - startedAt));
    };

    toast.addEventListener("pointerenter", pauseTimer);
    toast.addEventListener("pointerleave", startTimer);
    // Keyboard users tabbing onto the close button get the same grace period.
    toast.addEventListener("focusin", pauseTimer);
    toast.addEventListener("focusout", (ev) => {
      if (!toast.contains(ev.relatedTarget)) startTimer();
    });
    startTimer();
  }

  stack.appendChild(toast);
};
