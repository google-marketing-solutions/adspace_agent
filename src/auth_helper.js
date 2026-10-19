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

// AdSpace Agent — Google authorization helper & live `adk-ui` renderer for `/ui/`.
//
// Injected into `/ui/` from the same origin (`script-src 'self'`).
// 1. Listens to `/oauth2/events` (SSE, bound to the signed-in user) so the
//    instant a tool call needs Google OAuth consent, a centered popup window
//    opens automatically while the tool awaits completion on the server.
// 2. Watches `details.tool-call` blocks in `/ui/` for `adk-ui` (`UiResponse`)
//    component trees (`render_chart`, `render_table`, `render_layout`,
//    `render_card`, `render_form`, `render_confirm`, `render_modal`,
//    `render_alert`, `render_progress`, `render_toast`) and renders them as
//    standalone, theme-native cards directly in `.transcript`.
(() => {
  "use strict";
  const AUTH_PREFIX = "https://accounts.google.com/o/oauth2/";
  const CHANNEL = "adspace:oauth";
  const EVENTS_PATH = "/oauth2/events";
  const ACK_PATH = "/oauth2/ack";
  const RESUME_MARKER = "Google access has been granted for this chat";
  const SVG_NS = "http://www.w3.org/2000/svg";
  // `input_type` values a `text_input` component may request (anything else → "text").
  const SAFE_INPUT_TYPES = ["text", "email", "url", "tel", "password", "search", "number"];

  // Series palette harmonized with ADK Web UI's `--accent-primary` (#0f8a8a / #4fd1c5).
  const PALETTE = [
    "var(--accent-primary)",
    "#3b82f6",
    "#f59e0b",
    "#10b981",
    "#8b5cf6",
    "#ec4899",
    "#06b6d4",
    "#f97316",
  ];

  const seenLinks = new WeakSet();
  const seenDetails = new WeakSet();
  // `request_id`s of `auth_required` events already handled (the event carries
  // only the consent URL and this opaque id, never the signed OAuth state).
  const handledRequests = new Set();
  // Upper bound on `handledRequests` (oldest ids are evicted first).
  const MAX_HANDLED_REQUESTS = 200;
  let popup = null;
  let modal = null;

  // CSP forbids inline `style="..."` attributes; CSSOM assignments are allowed.
  const style = (el, props) => {
    for (const [k, v] of Object.entries(props)) el.style[k] = v;
  };

  // Adds/removes a class only when it actually changes (keeps `class` attribute
  // churn on React-owned nodes to a minimum).
  const toggleClass = (el, cls, on) => {
    if (!el || !el.classList) return;
    if (on) {
      if (!el.classList.contains(cls)) el.classList.add(cls);
    } else if (el.classList.contains(cls)) {
      el.classList.remove(cls);
    }
  };

  // Inline `<style>` text is also blocked by CSP, but `CSSStyleSheet.insertRule`
  // (CSSOM) is not: rules that must target React-owned nodes by class live here.
  const helperSheet = (() => {
    try {
      const el = document.createElement("style");
      el.setAttribute("data-adspace-helper", "");
      (document.head || document.documentElement).appendChild(el);
      return el.sheet;
    } catch {
      return null;
    }
  })();
  const addRules = (rules) => {
    if (!helperSheet) return;
    for (const rule of rules) {
      try {
        helperSheet.insertRule(rule, helperSheet.cssRules.length);
      } catch {
        /* selector unsupported by this browser: skip the rule */
      }
    }
  };

  // Layout for `<article class="message">` blocks that hold `render_*` UI cards.
  // React owns the article's children, so they are never re-parented: the
  // article gets one state class and each run of visible non-card children is
  // tagged `adk-ui-seg[-start|-end]` so the chat-bubble chrome is drawn by CSS.
  const BUBBLE_BORDER = "1px solid var(--border-default)";
  addRules([
    ".adk-ui-hidden { display: none !important; }",
    "article.message.adk-ui-unboxed, article.message.adk-ui-split { display: flex; flex-direction: column; background: transparent; border: 0; box-shadow: none; border-radius: 0; min-height: 0; overflow: visible; }",
    "article.message.adk-ui-unboxed { gap: 14px; }",
    "article.message.adk-ui-unboxed > header { display: none; }",
    `article.message.adk-ui-split > .adk-ui-seg { background: var(--surface-card); border-left: ${BUBBLE_BORDER}; border-right: ${BUBBLE_BORDER}; }`,
    `article.message.adk-ui-split > .adk-ui-seg-start { border-top: ${BUBBLE_BORDER}; border-top-left-radius: 10px; border-top-right-radius: 10px; }`,
    `article.message.adk-ui-split > .adk-ui-seg-end { border-bottom: ${BUBBLE_BORDER}; border-bottom-left-radius: 10px; border-bottom-right-radius: 10px; }`,
    "article.message.adk-ui-split > .adk-ui-card ~ .adk-ui-seg-start, article.message.adk-ui-split > .adk-ui-seg ~ .adk-ui-card, article.message.adk-ui-split > .adk-ui-card ~ .adk-ui-card { margin-top: 14px; }",
    // `tabs` component: active state is a `data-active` attribute, not re-styling
    // every button on select. `!important` because the host page styles `button`.
    "button.adk-ui-tab { border: 0 !important; border-bottom: 2px solid transparent !important; background: transparent !important; color: var(--dm-text-secondary, var(--text-secondary)) !important; }",
    'button.adk-ui-tab[data-active="true"] { color: var(--dm-text-primary, var(--text-primary)) !important; border-bottom-color: var(--dm-text-primary, var(--text-primary)) !important; }',
  ]);
  let tabsUid = 0;

  // Prevent `/ui/`'s 4-second session poll (`useEffect([events])`) from yanking
  // `.transcript` to the bottom via `C.current.scrollTo({ top: scrollHeight })`.
  // Only swallowed while the user has scrolled up to read (> 80px from the
  // bottom); when they are already at the bottom the call is harmless and
  // keeps new messages in view. Trade-off: a *deliberate* scroll-to-bottom
  // (e.g. on a new message) is also suppressed while the user is scrolled up,
  // which is what a reader expects anyway.
  const SCROLL_PIN_SLACK_PX = 80;
  const origScrollTo = Element.prototype.scrollTo;
  if (typeof origScrollTo === "function") {
    Element.prototype.scrollTo = function (...args) {
      if (
        this &&
        this.classList &&
        this.classList.contains("transcript") &&
        this.scrollHeight - this.scrollTop - this.clientHeight > SCROLL_PIN_SLACK_PX
      ) {
        return;
      }
      return origScrollTo.apply(this, args);
    };
  }

  const svgEl = (tag, attrs = {}) => {
    const el = document.createElementNS(SVG_NS, tag);
    for (const [k, v] of Object.entries(attrs)) {
      if (v !== undefined && v !== null) el.setAttribute(k, String(v));
    }
    if (tag === "text") {
      el.setAttribute("stroke", "none");
    }
    return el;
  };

  // Element that had focus before the fallback dialog opened (restored on close).
  let modalReturnFocus = null;

  const hideModal = () => {
    if (modal) modal.remove();
    modal = null;
    const target = modalReturnFocus;
    modalReturnFocus = null;
    if (target && typeof target.focus === "function" && target.isConnected) {
      try {
        target.focus();
      } catch {
        /* ignore */
      }
    }
  };

  const openPopup = (url) => {
    const w = 520;
    const h = 720;
    const left = Math.max(0, (window.screenX || 0) + ((window.outerWidth || w) - w) / 2);
    const top = Math.max(0, (window.outerHeight ? (window.screenY || 0) + (window.outerHeight - h) / 2 : 80));
    const win = window.open(
      url,
      "adspace_google_auth",
      `popup=yes,width=${w},height=${h},left=${left},top=${top}`
    );
    if (win) {
      popup = win;
      try {
        win.focus();
      } catch {
        /* ignore */
      }
    }
    return win;
  };

  const shortScopes = (scopes) =>
    (Array.isArray(scopes) ? scopes : [])
      .map((s) => String(s).slice(String(s).lastIndexOf("/") + 1))
      .filter(Boolean)
      .join(", ");

  // Centered modal shown ONLY if both `window.open` was blocked by the browser
  // AND the server could not open the consent screen locally (e.g. Cloud Run).
  const showFallbackModal = (authUrl, scopes) => {
    hideModal();
    modal = document.createElement("div");
    modal.setAttribute("role", "dialog");
    modal.setAttribute("aria-modal", "true");
    modal.setAttribute("aria-label", "Google authorization required");
    style(modal, {
      position: "fixed",
      inset: "0",
      zIndex: "2147483000",
      display: "flex",
      alignItems: "center",
      justifyContent: "center",
      background: "rgba(5, 10, 25, 0.6)",
      font: "14px/1.5 -apple-system, BlinkMacSystemFont, Segoe UI, sans-serif",
    });

    const box = document.createElement("div");
    style(box, {
      maxWidth: "380px",
      width: "90%",
      padding: "18px 20px",
      borderRadius: "10px",
      background: "var(--surface-card)",
      color: "var(--text-primary)",
      border: "1px solid var(--border-default)",
      boxShadow: "var(--shadow-md)",
    });

    const title = document.createElement("div");
    title.textContent = "Google authorization required";
    style(title, { fontSize: "15px", fontWeight: "650", marginBottom: "6px" });

    const detail = document.createElement("div");
    const names = shortScopes(scopes);
    detail.textContent = `Click below to open the Google consent popup${names ? ` (${names})` : ""}. Your request will continue automatically as soon as you grant access.`;
    style(detail, { color: "var(--text-secondary)", fontSize: "13px", marginBottom: "16px" });

    const row = document.createElement("div");
    style(row, { display: "flex", gap: "8px", justifyContent: "flex-end" });

    const cancel = document.createElement("button");
    cancel.type = "button";
    cancel.textContent = "Cancel";
    style(cancel, {
      border: "1px solid var(--border-default)",
      borderRadius: "6px",
      background: "var(--bg-primary)",
      color: "var(--text-secondary)",
      cursor: "pointer",
      padding: "6px 12px",
      fontSize: "12px",
    });
    cancel.addEventListener("click", hideModal);

    const button = document.createElement("button");
    button.type = "button";
    button.textContent = "Continue with Google";
    style(button, {
      border: "1px solid var(--accent-primary)",
      borderRadius: "6px",
      padding: "6px 14px",
      cursor: "pointer",
      background: "var(--accent-primary)",
      color: "#fff",
      fontWeight: "650",
      fontSize: "12px",
    });
    button.addEventListener("click", () => {
      if (openPopup(authUrl)) hideModal();
    });

    row.append(cancel, button);
    box.append(title, detail, row);
    modal.appendChild(box);

    // Focus management: remember the opener, focus the primary action, keep
    // Tab inside the dialog, and let Escape dismiss it.
    modalReturnFocus = document.activeElement;
    modal.addEventListener("keydown", (event) => {
      if (event.key === "Escape") {
        event.preventDefault();
        hideModal();
        return;
      }
      if (event.key !== "Tab") return;
      const focusables = [cancel, button];
      const idx = focusables.indexOf(document.activeElement);
      if (event.shiftKey) {
        if (idx <= 0) {
          event.preventDefault();
          focusables[focusables.length - 1].focus();
        }
      } else if (idx === -1 || idx === focusables.length - 1) {
        event.preventDefault();
        focusables[0].focus();
      }
    });
    document.body.appendChild(modal);
    button.focus();
  };

  const handleAuthRequired = async (data) => {
    if (!data || !data.auth_url || !data.request_id) return;
    // Only ever open Google's consent screen, whatever the event says.
    if (!String(data.auth_url).startsWith(AUTH_PREFIX)) return;
    if (handledRequests.has(data.request_id)) return;
    handledRequests.add(data.request_id);
    while (handledRequests.size > MAX_HANDLED_REQUESTS) {
      handledRequests.delete(handledRequests.values().next().value);
    }

    const win = openPopup(data.auth_url);
    const opened = !!win;
    if (win) {
      // Some popup blockers hand back a window and close it right away;
      // re-check after a tick and fall back to the modal in that case
      // (`popup !== win` means the flow already completed and closed it).
      setTimeout(() => {
        if (win.closed && popup === win) {
          showFallbackModal(data.auth_url, data.scopes);
        }
      }, 500);
    }
    try {
      const resp = await fetch(ACK_PATH, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ request_id: data.request_id, opened }),
      });
      if (!opened && resp.ok) {
        const body = await resp.json();
        if (!body.fallback_opened) {
          showFallbackModal(data.auth_url, data.scopes);
        }
      } else if (!opened) {
        showFallbackModal(data.auth_url, data.scopes);
      }
    } catch {
      if (!opened) showFallbackModal(data.auth_url, data.scopes);
    }
  };

  const handleCompletion = (status) => {
    hideModal();
    if (status === "ok") {
      if (popup && !popup.closed) {
        try {
          popup.close();
        } catch {
          /* ignore */
        }
      }
      popup = null;
      // Couples to the ADK web UI's DOM: its session "Refresh" button has no
      // stable id. Prefer an accessible label, fall back to the visible text.
      const refresh =
        document.querySelector('button[aria-label*="refresh" i]') ||
        Array.from(document.querySelectorAll("button")).find((b) =>
          /^\s*refresh\s*$/i.test(b.textContent || "")
        );
      if (refresh) refresh.click();
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
          handleCompletion(data.status);
        }
      } catch {
        /* ignore malformed event */
      }
    });
  }

  const onReport = (data) => {
    if (!data || data.type !== CHANNEL) return;
    handleCompletion(data.status);
  };
  if ("BroadcastChannel" in window) {
    new BroadcastChannel(CHANNEL).addEventListener("message", (event) => onReport(event.data));
  }
  window.addEventListener("message", (event) => {
    if (event.origin === location.origin) onReport(event.data);
  });

  // ---------------------------------------------------------------------------
  // Live `adk-ui` (`UiResponse`) Standalone Card Renderer for `/ui/`
  // ---------------------------------------------------------------------------

  const sendMessageToComposer = (text) => {
    const ta =
      document.querySelector('textarea[aria-label="Message agent"]') ||
      document.querySelector(".composer-wrap textarea") ||
      document.querySelector("textarea");
    if (!ta) return false;
    const setter = Object.getOwnPropertyDescriptor(
      window.HTMLTextAreaElement.prototype,
      "value"
    )?.set;
    if (setter) setter.call(ta, text);
    else ta.value = text;
    ta.dispatchEvent(new Event("input", { bubbles: true }));
    setTimeout(() => {
      const sendBtn =
        document.querySelector("button.send-button:not(.stop-button)") ||
        document.querySelector('button[aria-label="Send message"]');
      if (sendBtn && !sendBtn.disabled) {
        sendBtn.click();
      } else {
        ta.focus();
      }
    }, 45);
    return true;
  };

  const toNumber = (val) => {
    if (typeof val === "number" && Number.isFinite(val)) return val;
    if (typeof val === "string") {
      const cleaned = val.replace(/[$,%\s,]/g, "");
      const n = Number(cleaned);
      if (cleaned !== "" && Number.isFinite(n)) return n;
    }
    return NaN;
  };

  const formatCompact = (n) => {
    if (!Number.isFinite(n)) return String(n ?? "");
    const abs = Math.abs(n);
    if (abs >= 1e9) return `${(n / 1e9).toFixed(1).replace(/\.0$/, "")}B`;
    if (abs >= 1e6) return `${(n / 1e6).toFixed(1).replace(/\.0$/, "")}M`;
    if (abs >= 1e3) return `${(n / 1e3).toFixed(1).replace(/\.0$/, "")}K`;
    if (Number.isInteger(n)) return String(n);
    return n.toFixed(2).replace(/\.?0+$/, "");
  };

  const formatCellValue = (val) => {
    if (val === null || val === undefined) return "—";
    if (typeof val === "number") {
      return Number.isInteger(val)
        ? val.toLocaleString()
        : val.toLocaleString(undefined, { maximumFractionDigits: 2 });
    }
    if (typeof val === "boolean") return val ? "Yes" : "No";
    if (typeof val === "object") return JSON.stringify(val);
    return String(val);
  };

  // Uses `/ui/`'s native CSS custom properties so light/dark mode matches 100%.
  const variantColors = (variant) => {
    switch (String(variant || "default").toLowerCase()) {
      case "success":
      case "operational":
      case "ok":
        return {
          bg: "color-mix(in srgb, var(--accent-success) 14%, var(--bg-primary))",
          fg: "var(--accent-success)",
          border: "color-mix(in srgb, var(--accent-success) 42%, var(--border-default))",
        };
      case "warning":
      case "degraded":
        return {
          bg: "color-mix(in srgb, var(--accent-warning) 14%, var(--bg-primary))",
          fg: "var(--accent-warning)",
          border: "color-mix(in srgb, var(--accent-warning) 45%, var(--border-default))",
        };
      case "error":
      case "danger":
      case "down":
      case "outage":
        return {
          bg: "color-mix(in srgb, var(--accent-error) 14%, var(--bg-primary))",
          fg: "var(--accent-error)",
          border: "color-mix(in srgb, var(--accent-error) 42%, var(--border-default))",
        };
      case "info":
      case "primary":
        return {
          bg: "var(--accent-wash)",
          fg: "var(--accent-primary)",
          border: "color-mix(in srgb, var(--accent-primary) 40%, var(--border-default))",
        };
      default:
        return {
          bg: "var(--bg-primary)",
          fg: "var(--text-secondary)",
          border: "var(--border-default)",
        };
    }
  };

  const alertVariantLabel = (variant) => {
    const v = String(variant || "info").toLowerCase();
    if (v === "info") return "Info";
    if (v === "warn" || v === "warning") return "Warning";
    if (v === "error" || v === "danger" || v === "outage") return "Error";
    if (v === "success" || v === "ok") return "Success";
    return v.charAt(0).toUpperCase() + v.slice(1);
  };

  const createHelperAlert = (comp, isInline = false) => {
    const variantLabel = alertVariantLabel(comp.variant);
    const titleText = String(comp.title || comp.message || "Alert");

    const card = document.createElement("section");
    card.className = isInline ? "dm-alert-inline" : "adk-ui-surface adk-ui-card dm-alert-card";
    card.setAttribute("data-ui-scope", "true");
    style(card, {
      position: "relative",
      display: "flex",
      flexDirection: "column",
      alignItems: "flex-start",
      gap: isInline ? "14px" : "18px",
      padding: isInline ? "18px 20px" : "24px 26px",
      borderRadius: isInline ? "20px" : "24px",
      background: "var(--surface-card, #ffffff)",
      border: "1px solid var(--dm-outline, rgba(33, 34, 38, 0.08))",
      boxShadow: isInline
        ? "none"
        : "0 12px 32px -12px rgba(18, 19, 23, 0.08), 0 2px 8px -2px rgba(18, 19, 23, 0.04)",
      maxWidth: isInline ? "100%" : "760px",
      width: "100%",
    });

    const content = document.createElement("div");
    style(content, { display: "flex", flexDirection: "column", gap: "6px", minWidth: "0", width: "100%" });
    const titleEl = document.createElement("h3");
    titleEl.textContent = titleText;
    style(titleEl, {
      margin: "0",
      fontSize: isInline ? "16.5px" : "20px",
      fontWeight: "450",
      lineHeight: isInline ? "1.34" : "1.3",
      letterSpacing: "-0.008em",
      color: "var(--dm-text-primary, var(--text-primary, #121317))",
      textWrap: "pretty",
    });
    content.appendChild(titleEl);

    if (comp.description && String(comp.description).trim()) {
      const descEl = document.createElement("p");
      descEl.textContent = String(comp.description).trim();
      style(descEl, {
        margin: "0",
        fontSize: isInline ? "14px" : "15px",
        fontWeight: "400",
        lineHeight: isInline ? "1.48" : "1.5",
        color: "var(--dm-text-primary, var(--text-primary, #121317))",
      });
      content.appendChild(descEl);
    }

    const statusChip = document.createElement("span");
    statusChip.textContent = variantLabel;
    style(statusChip, {
      display: "inline-flex",
      alignItems: "center",
      justifyContent: "center",
      padding: isInline ? "5px 12px" : "6px 14px",
      borderRadius: "999px",
      border: "1px solid var(--protected-btn-outline, rgba(33, 34, 38, 0.12))",
      background: "transparent",
      color: "var(--dm-text-primary, var(--text-primary, #121317))",
      fontSize: isInline ? "12.5px" : "13px",
      fontWeight: "500",
      lineHeight: "1.2",
      letterSpacing: "0.004em",
      userSelect: "none",
    });

    card.append(content, statusChip);
    return card;
  };

  // Creates a standalone top-level card matching `.message` in `index-FuuY_PBH.css`.
  const createStandaloneCard = ({ title, description, headerRight, flushBody = false }) => {
    const card = document.createElement("section");
    card.className = "adk-ui-surface adk-ui-card";
    card.setAttribute("data-ui-scope", "true");
    style(card, {
      maxWidth: "760px",
      width: "100%",
      border: "1px solid var(--dm-outline, var(--border-default))",
      borderRadius: "24px",
      background: "var(--surface-card)",
      boxShadow:
        "0 12px 32px -12px rgba(18, 19, 23, 0.08), 0 2px 8px -2px rgba(18, 19, 23, 0.04)",
      overflow: "hidden",
      display: "flex",
      flexDirection: "column",
    });

    if (title || description || headerRight) {
      const hdr = document.createElement("header");
      style(hdr, {
        display: "flex",
        alignItems: "flex-start",
        justifyContent: "space-between",
        gap: "16px",
        padding: flushBody ? "20px 24px 16px" : "22px 24px 6px",
        borderBottom: flushBody ? "1px solid var(--dm-outline, var(--border-default))" : "0",
        background: "transparent",
      });

      const left = document.createElement("div");
      style(left, {
        display: "flex",
        flexDirection: "column",
        alignItems: "flex-start",
        gap: "4px",
        minWidth: "0",
      });

      if (title) {
        const strong = document.createElement("strong");
        strong.textContent = title;
        style(strong, {
          fontSize: "20px",
          fontWeight: "450",
          lineHeight: "1.22",
          letterSpacing: "-0.008em",
          color: "var(--dm-text-primary, var(--text-primary))",
        });
        left.appendChild(strong);
      }

      if (description) {
        const sub = document.createElement("span");
        sub.textContent = description;
        style(sub, {
          fontSize: "14.5px",
          fontWeight: "400",
          lineHeight: "1.45",
          color: "var(--dm-text-secondary, var(--text-muted))",
        });
        left.appendChild(sub);
      }

      hdr.appendChild(left);
      if (headerRight) hdr.appendChild(headerRight);
      card.appendChild(hdr);
    }

    const body = document.createElement("div");
    style(body, {
      padding: flushBody ? "0" : "10px 24px 24px",
      display: "flex",
      flexDirection: "column",
      gap: "16px",
    });
    card.appendChild(body);

    return { card, body };
  };

  // Donut chart with a legend (`pie`). Returns the row holding the SVG and legend.
  const renderPieChart = ({ data, xKey, yKeys, colors, chartLabel }) => {
    const yKey = yKeys[0];
    const slices = data
      .map((row, idx) => ({
        label: String(row[xKey] ?? `Item ${idx + 1}`),
        value: Math.max(0, toNumber(row[yKey]) || 0),
        color: colors[idx % colors.length],
      }))
      .filter((s) => s.value > 0);
    const total = slices.reduce((acc, s) => acc + s.value, 0) || 1;

    const rowWrap = document.createElement("div");
    style(rowWrap, {
      display: "flex",
      flexWrap: "wrap",
      alignItems: "center",
      gap: "24px",
      padding: "4px 0",
    });

    const svg = svgEl("svg", { viewBox: "0 0 210 210", role: "img", "aria-label": chartLabel });
    // Override global `svg { width:17px; height:17px; stroke:currentColor }`
    style(svg, {
      width: "180px",
      height: "180px",
      flexShrink: "0",
      fill: "none",
      stroke: "none",
    });

    const cx = 105;
    const cy = 105;
    const rOuter = 88;
    const rInner = 52;
    let angle = -Math.PI / 2;

    for (const s of slices) {
      const frac = s.value / total;
      const sweep = frac * Math.PI * 2;
      if (frac >= 0.9999) {
        const c = svgEl("circle", {
          cx,
          cy,
          r: (rOuter + rInner) / 2,
          fill: "none",
          stroke: s.color,
          "stroke-width": rOuter - rInner,
        });
        const t = svgEl("title");
        t.textContent = `${s.label}: ${formatCellValue(s.value)} (100%)`;
        c.appendChild(t);
        svg.appendChild(c);
      } else {
        const end = angle + sweep;
        const large = sweep > Math.PI ? 1 : 0;
        const x1 = cx + rOuter * Math.cos(angle);
        const y1 = cy + rOuter * Math.sin(angle);
        const x2 = cx + rOuter * Math.cos(end);
        const y2 = cy + rOuter * Math.sin(end);
        const x3 = cx + rInner * Math.cos(end);
        const y3 = cy + rInner * Math.sin(end);
        const x4 = cx + rInner * Math.cos(angle);
        const y4 = cy + rInner * Math.sin(angle);
        const d = [
          `M ${x1} ${y1}`,
          `A ${rOuter} ${rOuter} 0 ${large} 1 ${x2} ${y2}`,
          `L ${x3} ${y3}`,
          `A ${rInner} ${rInner} 0 ${large} 0 ${x4} ${y4}`,
          "Z",
        ].join(" ");
        const path = svgEl("path", {
          d,
          fill: s.color,
          stroke: "var(--surface-card)",
          "stroke-width": "2",
        });
        const t = svgEl("title");
        t.textContent = `${s.label}: ${formatCellValue(s.value)} (${(frac * 100).toFixed(1)}%)`;
        path.appendChild(t);
        svg.appendChild(path);
        angle = end;
      }
    }

    const centerTop = svgEl("text", {
      x: cx,
      y: cy - 4,
      "text-anchor": "middle",
      fill: "var(--dm-text-secondary, var(--text-muted))",
      "font-size": "10.5",
    });
    centerTop.textContent = yKey;
    const centerVal = svgEl("text", {
      x: cx,
      y: cy + 14,
      "text-anchor": "middle",
      fill: "var(--dm-text-primary, var(--text-primary))",
      "font-size": "16",
      "font-weight": "500",
    });
    centerVal.textContent = formatCompact(total);
    svg.append(centerTop, centerVal);

    const legend = document.createElement("div");
    style(legend, {
      display: "flex",
      flexDirection: "column",
      gap: "8px",
      flex: "1",
      minWidth: "180px",
    });
    for (const s of slices) {
      const item = document.createElement("div");
      style(item, {
        display: "flex",
        alignItems: "center",
        justifyContent: "space-between",
        gap: "10px",
        fontSize: "13px",
      });
      const left = document.createElement("div");
      style(left, { display: "flex", alignItems: "center", gap: "8px", minWidth: "0" });
      const swatch = document.createElement("span");
      style(swatch, {
        width: "9px",
        height: "9px",
        borderRadius: "999px",
        background: s.color,
        flexShrink: "0",
      });
      const lbl = document.createElement("span");
      lbl.textContent = s.label;
      style(lbl, {
        color: "var(--dm-text-primary, var(--text-primary))",
        overflow: "hidden",
        textOverflow: "ellipsis",
        whiteSpace: "nowrap",
      });
      left.append(swatch, lbl);

      const right = document.createElement("span");
      right.textContent = `${formatCompact(s.value)} (${((s.value / total) * 100).toFixed(1)}%)`;
      style(right, {
        color: "var(--dm-text-secondary, var(--text-secondary))",
        fontFamily: "var(--mono)",
        fontSize: "12px",
        flexShrink: "0",
      });
      item.append(left, right);
      legend.appendChild(item);
    }

    rowWrap.append(svg, legend);
    return rowWrap;
  };

  // Cartesian SVG chart (`bar`, `line`, `area`, `scatter`, `sparkline`).
  // Appends the SVG (and legend) to `wrap`.
  const appendCartesianChart = (wrap, comp, { data, xKey, yKeys, colors, kind, chartLabel }) => {
    const isSpark = kind === "sparkline";
    const W = 680;
    const H = isSpark ? 84 : 245;
    const padL = isSpark ? 8 : 52;
    const padR = isSpark ? 8 : 16;
    const padT = isSpark ? 8 : 16;
    const padB = isSpark ? 10 : 44;
    const plotW = W - padL - padR;
    const plotH = H - padT - padB;

    let minVal = 0;
    let maxVal = 0;
    for (const row of data) {
      for (const yk of yKeys) {
        const v = toNumber(row[yk]);
        if (Number.isFinite(v)) {
          if (v < minVal) minVal = v;
          if (v > maxVal) maxVal = v;
        }
      }
    }
    if (maxVal === minVal) maxVal = minVal + 1;
    const span = maxVal - minVal;
    const yToPx = (v) => padT + plotH - ((v - minVal) / span) * plotH;

    const svg = svgEl("svg", { viewBox: `0 0 ${W} ${H}`, role: "img", "aria-label": chartLabel });
    style(svg, {
      width: "100%",
      height: "auto",
      maxHeight: isSpark ? "90px" : "260px",
      overflow: "visible",
      fill: "none",
      stroke: "none",
    });

    if (!isSpark) {
      const steps = 4;
      for (let i = 0; i <= steps; i++) {
        const val = minVal + (span * i) / steps;
        const y = yToPx(val);
        const line = svgEl("line", {
          x1: padL,
          y1: y,
          x2: W - padR,
          y2: y,
          stroke: "var(--dm-outline, var(--border-default))",
          "stroke-width": "1",
        });
        const label = svgEl("text", {
          x: padL - 8,
          y: y + 3.5,
          "text-anchor": "end",
          fill: "var(--dm-text-secondary, var(--text-muted))",
          "font-size": "10.5",
        });
        label.textContent = formatCompact(val);
        svg.append(line, label);
      }
    }

    const n = data.length;
    if (kind === "bar") {
      const groupStep = plotW / Math.max(1, n);
      const groupW = groupStep * 0.72;
      const barW = Math.max(4, groupW / Math.max(1, yKeys.length) - 2);
      const zeroY = yToPx(Math.max(0, minVal));

      data.forEach((row, i) => {
        const gx = padL + i * groupStep + (groupStep - groupW) / 2;
        yKeys.forEach((yk, sIdx) => {
          const raw = row[yk];
          const v = toNumber(raw);
          if (!Number.isFinite(v)) return;
          const yVal = yToPx(v);
          const top = Math.min(yVal, zeroY);
          const h = Math.max(2, Math.abs(zeroY - yVal));
          const bx = gx + sIdx * (barW + 2);
          const rect = svgEl("rect", {
            x: bx,
            y: top,
            width: barW,
            height: h,
            rx: "6",
            fill: colors[sIdx % colors.length],
            stroke: "none",
          });
          const title = svgEl("title");
          title.textContent = `${row[xKey] ?? i} · ${yk}: ${formatCellValue(raw)}`;
          rect.appendChild(title);
          svg.appendChild(rect);

          if (n <= 12 && yKeys.length === 1) {
            const valTxt = svgEl("text", {
              x: bx + barW / 2,
              y: Math.max(10, top - 5),
              "text-anchor": "middle",
              fill: "var(--dm-text-secondary, var(--text-secondary))",
              "font-size": "10",
            });
            valTxt.textContent = formatCompact(v);
            svg.appendChild(valTxt);
          }
        });

        const stepEvery = Math.max(1, Math.ceil(n / 10));
        if (i % stepEvery === 0) {
          const rawLbl = String(row[xKey] ?? "");
          const shortLbl = rawLbl.length > 13 ? `${rawLbl.slice(0, 12)}…` : rawLbl;
          const xTxt = svgEl("text", {
            x: padL + (i + 0.5) * groupStep,
            y: H - padB + 17,
            "text-anchor": "middle",
            fill: "var(--dm-text-secondary, var(--text-muted))",
            "font-size": "10.5",
          });
          xTxt.textContent = shortLbl;
          const t = svgEl("title");
          t.textContent = rawLbl;
          xTxt.appendChild(t);
          svg.appendChild(xTxt);
        }
      });
    } else {
      const xForIdx = (i) =>
        n === 1 ? padL + plotW / 2 : padL + (i / (n - 1)) * plotW;
      const baseY = yToPx(Math.max(0, minVal));

      yKeys.forEach((yk, sIdx) => {
        const color = colors[sIdx % colors.length];
        const pts = [];
        data.forEach((row, i) => {
          const v = toNumber(row[yk]);
          if (Number.isFinite(v)) {
            pts.push({ x: xForIdx(i), y: yToPx(v), raw: row[yk], label: row[xKey] ?? i });
          }
        });
        if (!pts.length) return;

        if (kind === "area" && pts.length > 1) {
          const areaPts = [
            `${pts[0].x},${baseY}`,
            ...pts.map((p) => `${p.x},${p.y}`),
            `${pts[pts.length - 1].x},${baseY}`,
          ].join(" ");
          svg.appendChild(
            svgEl("polygon", {
              points: areaPts,
              fill: color,
              "fill-opacity": "0.14",
              stroke: "none",
            })
          );
        }

        if (kind !== "scatter" && pts.length > 1) {
          svg.appendChild(
            svgEl("polyline", {
              points: pts.map((p) => `${p.x},${p.y}`).join(" "),
              fill: "none",
              stroke: color,
              "stroke-width": isSpark ? "2" : "2.2",
              "stroke-linecap": "round",
              "stroke-linejoin": "round",
            })
          );
        }

        if (!isSpark) {
          for (const p of pts) {
            const dot = svgEl("circle", {
              cx: p.x,
              cy: p.y,
              r: kind === "scatter" ? "4.5" : "3.2",
              fill: color,
              stroke: "var(--surface-card)",
              "stroke-width": "1.5",
            });
            const t = svgEl("title");
            t.textContent = `${p.label} · ${yk}: ${formatCellValue(p.raw)}`;
            dot.appendChild(t);
            svg.appendChild(dot);
          }
        }
      });

      if (!isSpark) {
        const stepEvery = Math.max(1, Math.ceil(n / 8));
        data.forEach((row, i) => {
          if (i % stepEvery !== 0 && i !== n - 1) return;
          const rawLbl = String(row[xKey] ?? "");
          const shortLbl = rawLbl.length > 13 ? `${rawLbl.slice(0, 12)}…` : rawLbl;
          const xTxt = svgEl("text", {
            x: xForIdx(i),
            y: H - padB + 17,
            "text-anchor": "middle",
            fill: "var(--dm-text-secondary, var(--text-muted))",
            "font-size": "10.5",
          });
          xTxt.textContent = shortLbl;
          svg.appendChild(xTxt);
        });
      }
    }

    wrap.appendChild(svg);

    if (comp.show_legend !== false && !isSpark && yKeys.length > 0) {
      const legend = document.createElement("div");
      style(legend, {
        display: "flex",
        flexWrap: "wrap",
        gap: "16px",
        justifyContent: "center",
        fontSize: "12.5px",
        color: "var(--dm-text-secondary, var(--text-secondary))",
      });
      yKeys.forEach((yk, sIdx) => {
        const item = document.createElement("span");
        style(item, { display: "inline-flex", alignItems: "center", gap: "7px" });
        const dot = document.createElement("span");
        style(dot, {
          width: "8px",
          height: "8px",
          borderRadius: "999px",
          background: colors[sIdx % colors.length],
        });
        const txt = document.createElement("span");
        txt.textContent = yk;
        item.append(dot, txt);
        legend.appendChild(item);
      });
      wrap.appendChild(legend);
    }
  };

  // Renders SVG charts (`bar`, `line`, `area`, `pie`, `scatter`, `sparkline`).
  const renderChartInner = (comp) => {
    const wrap = document.createElement("div");
    style(wrap, {
      display: "flex",
      flexDirection: "column",
      gap: "10px",
    });

    const data = Array.isArray(comp.data) ? comp.data : [];
    const xKey = comp.x_key || (data[0] ? Object.keys(data[0])[0] : "x");
    const yKeys =
      Array.isArray(comp.y_keys) && comp.y_keys.length
        ? comp.y_keys
        : data[0]
          ? Object.keys(data[0]).filter((k) => k !== xKey)
          : [];
    const colors = Array.isArray(comp.colors) && comp.colors.length ? comp.colors : PALETTE;
    const kind = String(comp.kind || "bar").toLowerCase();
    // Accessible name for the `<svg>` (the visible title lives in the card header).
    const chartLabel = comp.title ? String(comp.title) : `${kind} chart`;

    if (!data.length || !yKeys.length) {
      const empty = document.createElement("div");
      empty.textContent = "No chart data available.";
      style(empty, { color: "var(--text-muted)", fontSize: "12px", padding: "8px 0" });
      wrap.appendChild(empty);
      return wrap;
    }

    const ctx = { data, xKey, yKeys, colors, kind, chartLabel };
    if (kind === "pie") {
      wrap.appendChild(renderPieChart(ctx));
    } else {
      appendCartesianChart(wrap, comp, ctx);
    }
    return wrap;
  };

  // Renders interactive sortable/filterable table content (`table`).
  // Returns `{ element, toolbar }` so a parent card can mount `toolbar` right in its header!
  const renderTableParts = (comp) => {
    const wrap = document.createElement("div");
    style(wrap, { display: "flex", flexDirection: "column" });

    const rawRows = Array.isArray(comp.data) ? [...comp.data] : [];
    const columns =
      Array.isArray(comp.columns) && comp.columns.length
        ? comp.columns
        : rawRows[0]
          ? Object.keys(rawRows[0]).map((k) => ({ header: k, accessor_key: k, sortable: true }))
          : [];

    let filterQuery = "";
    let sortKey = null;
    let sortAsc = true;
    let page = 0;
    const pageSize = comp.page_size && comp.page_size > 0 ? Number(comp.page_size) : 0;

    const toolbar = document.createElement("div");
    style(toolbar, { display: "flex", alignItems: "center", gap: "10px" });
    const countChip = document.createElement("span");
    countChip.textContent = `${rawRows.length} rows`;
    style(countChip, {
      fontSize: "12.5px",
      fontWeight: "450",
      color: "var(--dm-text-secondary, var(--text-muted))",
    });
    toolbar.appendChild(countChip);

    if (rawRows.length > 4) {
      const search = document.createElement("input");
      search.type = "search";
      search.placeholder = "Filter…";
      style(search, {
        padding: "6px 14px",
        borderRadius: "999px",
        border: "1px solid var(--dm-outline, var(--border-default))",
        background: "var(--dm-surface-gray, var(--bg-primary))",
        color: "var(--dm-text-primary, var(--text-primary))",
        fontSize: "13px",
        width: "180px",
      });
      // Debounced: filtering re-sorts and rebuilds the whole body.
      let filterTimer = 0;
      search.addEventListener("input", () => {
        clearTimeout(filterTimer);
        filterTimer = setTimeout(() => {
          filterQuery = search.value.trim().toLowerCase();
          page = 0;
          rebuildBody();
        }, 100);
      });
      toolbar.appendChild(search);
    }

    const scroller = document.createElement("div");
    style(scroller, { overflowX: "auto" });

    const table = document.createElement("table");
    style(table, {
      width: "100%",
      borderCollapse: "collapse",
      fontSize: "13.5px",
      color: "var(--dm-text-primary, var(--text-primary))",
    });

    const thead = document.createElement("thead");
    const headRow = document.createElement("tr");
    style(headRow, {
      background: "transparent",
      borderBottom: "1px solid var(--dm-outline, var(--border-default))",
    });

    const tbody = document.createElement("tbody");
    const pager = document.createElement("div");

    const rebuildHeader = () => {
      headRow.replaceChildren();
      for (const col of columns) {
        const th = document.createElement("th");
        th.scope = "col";
        const canSort = comp.sortable !== false && col.sortable !== false;
        const isActive = sortKey === col.accessor_key;
        const indicator = isActive ? (sortAsc ? " ↑" : " ↓") : canSort ? " ↕" : "";
        style(th, {
          textAlign: "left",
          padding: "12px 18px",
          fontWeight: "500",
          fontSize: "13px",
          letterSpacing: "0.005em",
          color: "var(--dm-text-primary, var(--text-secondary))",
          whiteSpace: "nowrap",
          userSelect: "none",
        });
        if (canSort) {
          // Keyboard-reachable sort control; `aria-sort` reflects the active column.
          th.setAttribute("aria-sort", isActive ? (sortAsc ? "ascending" : "descending") : "none");
          const sortBtn = document.createElement("button");
          sortBtn.type = "button";
          sortBtn.textContent = `${col.header}${indicator}`;
          style(sortBtn, {
            border: "0",
            background: "transparent",
            padding: "0",
            margin: "0",
            cursor: "pointer",
            font: "inherit",
            color: "inherit",
            letterSpacing: "inherit",
            whiteSpace: "inherit",
          });
          sortBtn.addEventListener("click", () => {
            if (sortKey === col.accessor_key) sortAsc = !sortAsc;
            else {
              sortKey = col.accessor_key;
              sortAsc = true;
            }
            rebuildHeader();
            rebuildBody();
            // Keep focus on the (rebuilt) control for keyboard users.
            headRow.querySelector(`th[aria-sort="${sortAsc ? "ascending" : "descending"}"] > button`)?.focus();
          });
          th.appendChild(sortBtn);
        } else {
          th.textContent = `${col.header}${indicator}`;
        }
        headRow.appendChild(th);
      }
    };

    const rebuildBody = () => {
      tbody.replaceChildren();
      let rows = rawRows.filter((r) => {
        if (!filterQuery) return true;
        return columns.some((c) =>
          String(r[c.accessor_key] ?? "")
            .toLowerCase()
            .includes(filterQuery)
        );
      });

      if (sortKey) {
        rows.sort((a, b) => {
          const va = a[sortKey];
          const vb = b[sortKey];
          const na = toNumber(va);
          const nb = toNumber(vb);
          const cmp =
            Number.isFinite(na) && Number.isFinite(nb)
              ? na - nb
              : String(va ?? "").localeCompare(String(vb ?? ""));
          return sortAsc ? cmp : -cmp;
        });
      }

      const totalRows = rows.length;
      if (pageSize > 0) {
        const start = page * pageSize;
        rows = rows.slice(start, start + pageSize);
      }

      rows.forEach((row, idx) => {
        const tr = document.createElement("tr");
        const isLast = idx === rows.length - 1 && !(pageSize > 0 && totalRows > pageSize);
        style(tr, {
          borderBottom: isLast ? "0" : "1px solid var(--dm-outline, var(--border-default))",
          background:
            comp.striped && idx % 2 === 1
              ? "var(--dm-surface-gray, var(--bg-primary))"
              : "transparent",
        });
        columns.forEach((col, colIdx) => {
          const td = document.createElement("td");
          const raw = row[col.accessor_key];
          const text = formatCellValue(raw);
          const isNumeric =
            typeof raw === "number" ||
            (typeof raw === "string" && /^[$+-]?[\d,.]+%?$/.test(raw.trim()));
          td.textContent = text;
          style(td, {
            padding: "12px 18px",
            textAlign: isNumeric ? "right" : "left",
            fontFamily: isNumeric ? "var(--mono)" : "inherit",
            fontSize: isNumeric ? "13px" : "13.5px",
            color:
              colIdx === 0 && !isNumeric
                ? "var(--dm-text-primary, var(--text-primary))"
                : "var(--dm-text-secondary, var(--text-secondary))",
            whiteSpace: isNumeric ? "nowrap" : "normal",
          });
          tr.appendChild(td);
        });
        tbody.appendChild(tr);
      });

      if (pageSize > 0 && totalRows > pageSize) {
        pager.replaceChildren();
        const totalPages = Math.ceil(totalRows / pageSize);
        style(pager, {
          display: "flex",
          justifyContent: "space-between",
          alignItems: "center",
          padding: "12px 18px",
          fontSize: "13px",
          color: "var(--dm-text-secondary, var(--text-muted))",
          background: "transparent",
          borderTop: "1px solid var(--dm-outline, var(--border-default))",
        });
        const info = document.createElement("span");
        info.textContent = `Page ${page + 1} of ${totalPages}`;
        const btnWrap = document.createElement("div");
        style(btnWrap, { display: "flex", gap: "8px" });
        const mkBtn = (lbl, disabled, onClick) => {
          const b = document.createElement("button");
          b.type = "button";
          b.textContent = lbl;
          b.disabled = disabled;
          style(b, {
            padding: "6px 14px",
            borderRadius: "999px",
            border: "1px solid var(--dm-outline, var(--border-default))",
            background: "var(--dm-surface-gray, var(--surface-card))",
            color: "var(--dm-text-primary, var(--text-secondary))",
            cursor: disabled ? "default" : "pointer",
            opacity: disabled ? "0.4" : "1",
            fontSize: "12.5px",
            fontWeight: "500",
          });
          b.addEventListener("click", onClick);
          return b;
        };
        btnWrap.append(
          mkBtn("Prev", page <= 0, () => {
            page--;
            rebuildBody();
          }),
          mkBtn("Next", page >= totalPages - 1, () => {
            page++;
            rebuildBody();
          })
        );
        pager.append(info, btnWrap);
      } else {
        style(pager, { display: "none" });
      }
    };

    rebuildHeader();
    rebuildBody();
    thead.appendChild(headRow);
    table.append(thead, tbody);
    scroller.appendChild(table);
    wrap.append(scroller, pager);
    return { element: wrap, toolbar };
  };

  // Renderers for inner `adk_ui::Component`s, keyed by `type`. Each receives
  // `(comp, surfaceRoot, inGrid, type)`; see `renderInnerComponent`.
  const RENDERERS = {
    text(comp, surfaceRoot, inGrid) {
      const v = String(comp.variant || "body").toLowerCase();
      const el = document.createElement(v === "code" ? "code" : "div");
      el.textContent = comp.content ?? "";
      const map = {
        h1: {
          fontSize: "22px",
          fontWeight: "450",
          letterSpacing: "-0.012em",
          lineHeight: "1.2",
          color: "var(--dm-text-primary, var(--text-primary))",
        },
        h2: {
          fontSize: inGrid ? "28px" : "19px",
          fontWeight: "450",
          letterSpacing: inGrid ? "-0.015em" : "-0.01em",
          lineHeight: inGrid ? "1.12" : "1.22",
          color: "var(--dm-text-primary, var(--text-primary))",
        },
        h3: {
          fontSize: "16.5px",
          fontWeight: "450",
          letterSpacing: "-0.006em",
          lineHeight: "1.32",
          color: "var(--dm-text-primary, var(--text-primary))",
        },
        h4: {
          fontSize: "14.5px",
          fontWeight: "500",
          lineHeight: "1.4",
          color: "var(--dm-text-primary, var(--text-primary))",
        },
        caption: {
          fontSize: "13px",
          color: "var(--dm-text-secondary, var(--text-muted))",
          lineHeight: "1.45",
        },
        code: {
          fontFamily: "var(--mono)",
          fontSize: "12.5px",
          background: "var(--dm-surface-gray-subtle, var(--bg-primary))",
          border: "1px solid var(--dm-outline, var(--border-default))",
          padding: "1px 6px",
          borderRadius: "6px",
        },
        body: {
          fontSize: "14.5px",
          lineHeight: "1.55",
          color: "var(--dm-text-primary, var(--text-primary))",
        },
      };
      style(el, map[v] || map.body);
      return el;
    },

    badge(comp) {
      const span = document.createElement("span");
      span.textContent = comp.label ?? "";
      const vc = variantColors(comp.variant);
      style(span, {
        display: "inline-flex",
        alignItems: "center",
        padding: "4px 10px",
        borderRadius: "999px",
        fontSize: "12px",
        fontWeight: "500",
        background: vc.bg,
        color: vc.fg,
        border: "1px solid transparent",
        alignSelf: "flex-start",
      });
      return span;
    },

    divider(comp) {
      const hr = document.createElement("hr");
      style(hr, {
        border: "0",
        borderTop: "1px solid var(--dm-outline, var(--border-default))",
        margin: "6px 0",
        width: "100%",
      });
      return hr;
    },

    stack(comp, surfaceRoot, inGrid) {
      const div = document.createElement("div");
      const horiz = String(comp.direction || "vertical").toLowerCase() === "horizontal";
      const gapPx = Math.max(1, Number(comp.gap ?? 3)) * 4;
      style(div, {
        display: "flex",
        flexDirection: horiz ? "row" : "column",
        flexWrap: horiz ? "wrap" : "nowrap",
        alignItems: horiz ? "center" : "stretch",
        gap: `${gapPx}px`,
      });
      for (const child of comp.children || []) {
        div.appendChild(renderInnerComponent(child, surfaceRoot, inGrid));
      }
      return div;
    },

    grid(comp, surfaceRoot) {
      const div = document.createElement("div");
      const cols = Math.max(1, Math.min(6, Number(comp.columns) || 2));
      style(div, {
        display: "grid",
        gridTemplateColumns: `repeat(auto-fit, minmax(${cols >= 3 ? "165px" : "220px"}, 1fr))`,
        gap: "14px",
        alignItems: "stretch",
      });
      for (const child of comp.children || []) {
        div.appendChild(renderInnerComponent(child, surfaceRoot, true));
      }
      return div;
    },

    container(comp, surfaceRoot, inGrid) {
      const div = document.createElement("div");
      style(div, {
        display: "flex",
        flexDirection: "column",
        gap: "12px",
      });
      for (const child of comp.children || []) {
        div.appendChild(renderInnerComponent(child, surfaceRoot, inGrid));
      }
      return div;
    },

    // Nested card (e.g. KPI stat tile inside a `Grid`); also `modal` (aliased below).
    card(comp, surfaceRoot, inGrid) {
      const tile = document.createElement("div");
      tile.setAttribute("data-ui-scope", "true");
      style(tile, {
        display: "flex",
        flexDirection: "column",
        justifyContent: "space-between",
        gap: "8px",
        padding: inGrid ? "18px 20px" : "20px 22px",
        borderRadius: "20px",
        background: "var(--dm-surface-gray, var(--bg-primary))",
        border: "1px solid var(--dm-outline, var(--border-default))",
      });

      if (comp.title) {
        const t = document.createElement("div");
        t.textContent = comp.title;
        style(
          t,
          inGrid
            ? {
                fontSize: "13.5px",
                fontWeight: "450",
                letterSpacing: "0.005em",
                color: "var(--dm-text-secondary, var(--text-muted))",
              }
            : {
                fontSize: "16.5px",
                fontWeight: "450",
                letterSpacing: "-0.006em",
                color: "var(--dm-text-primary, var(--text-primary))",
              }
        );
        tile.appendChild(t);
      }
      if (comp.description) {
        const d = document.createElement("div");
        d.textContent = comp.description;
        style(d, {
          fontSize: "13.5px",
          lineHeight: "1.45",
          color: "var(--dm-text-secondary, var(--text-muted))",
        });
        tile.appendChild(d);
      }
      if (Array.isArray(comp.content) && comp.content.length) {
        const body = document.createElement("div");
        style(body, { display: "flex", flexDirection: "column", gap: "8px" });
        for (const child of comp.content) {
          body.appendChild(renderInnerComponent(child, tile, inGrid));
        }
        tile.appendChild(body);
      }
      if (Array.isArray(comp.footer) && comp.footer.length) {
        const ftr = document.createElement("div");
        style(ftr, {
          display: "flex",
          flexWrap: "wrap",
          gap: "8px",
          justifyContent: "flex-end",
          paddingTop: "10px",
        });
        for (const child of comp.footer) {
          ftr.appendChild(renderInnerComponent(child, tile, false));
        }
        tile.appendChild(ftr);
      }
      return tile;
    },

    tabs(comp, surfaceRoot) {
      const wrap = document.createElement("div");
      style(wrap, { display: "flex", flexDirection: "column", gap: "14px" });
      const bar = document.createElement("div");
      bar.setAttribute("role", "tablist");
      style(bar, {
        display: "flex",
        gap: "4px",
        borderBottom: "1px solid var(--dm-outline, var(--border-default))",
        paddingBottom: "0",
      });
      const panel = document.createElement("div");
      panel.setAttribute("role", "tabpanel");
      panel.tabIndex = 0;
      const tabs = Array.isArray(comp.tabs) ? comp.tabs : [];
      const uid = `adk-ui-tabs-${++tabsUid}`;
      panel.id = `${uid}-panel`;
      let activeIdx = 0;

      // Active look comes from the `.adk-ui-tab[data-active]` stylesheet rule.
      const selectTab = (idx, focus = false) => {
        activeIdx = idx;
        Array.from(bar.children).forEach((btn, i) => {
          const active = i === activeIdx;
          btn.dataset.active = active ? "true" : "false";
          btn.setAttribute("aria-selected", active ? "true" : "false");
          btn.tabIndex = active ? 0 : -1;
          if (active) {
            panel.setAttribute("aria-labelledby", btn.id);
            if (focus) btn.focus();
          }
        });
        panel.replaceChildren();
        for (const child of tabs[idx]?.content || []) {
          panel.appendChild(renderInnerComponent(child, surfaceRoot, false));
        }
      };

      tabs.forEach((tab, idx) => {
        const b = document.createElement("button");
        b.type = "button";
        b.className = "adk-ui-tab";
        b.id = `${uid}-tab-${idx}`;
        b.setAttribute("role", "tab");
        b.setAttribute("aria-controls", panel.id);
        b.textContent = tab.label || `Tab ${idx + 1}`;
        style(b, {
          borderRadius: "0",
          padding: "10px 16px",
          marginBottom: "-1px",
          fontSize: "14px",
          fontWeight: "500",
          cursor: "pointer",
        });
        b.addEventListener("click", () => selectTab(idx));
        b.addEventListener("keydown", (event) => {
          let next = null;
          if (event.key === "ArrowRight") next = (idx + 1) % tabs.length;
          else if (event.key === "ArrowLeft") next = (idx - 1 + tabs.length) % tabs.length;
          else if (event.key === "Home") next = 0;
          else if (event.key === "End") next = tabs.length - 1;
          if (next === null) return;
          event.preventDefault();
          selectTab(next, true);
        });
        bar.appendChild(b);
      });
      wrap.append(bar, panel);
      if (tabs.length) selectTab(0);
      return wrap;
    },

    chart(comp) {
      return renderChartInner(comp);
    },

    table(comp) {
      const { element, toolbar } = renderTableParts(comp);
      const box = document.createElement("div");
      style(box, {
        border: "1px solid var(--dm-outline, var(--border-default))",
        borderRadius: "18px",
        overflow: "hidden",
      });
      const bar = document.createElement("div");
      style(bar, {
        display: "flex",
        justifyContent: "flex-end",
        padding: "10px 16px",
        background: "transparent",
        borderBottom: "1px solid var(--dm-outline, var(--border-default))",
      });
      bar.appendChild(toolbar);
      box.append(bar, element);
      return box;
    },

    key_value(comp) {
      const dl = document.createElement("div");
      style(dl, {
        display: "grid",
        gridTemplateColumns: "minmax(140px, auto) 1fr",
        gap: "10px 20px",
        padding: "16px 18px",
        borderRadius: "18px",
        background: "var(--dm-surface-gray, var(--bg-primary))",
        border: "1px solid var(--dm-outline, var(--border-default))",
        fontSize: "13.5px",
      });
      for (const pair of comp.pairs || []) {
        const k = document.createElement("div");
        k.textContent = pair.key;
        style(k, { color: "var(--dm-text-secondary, var(--text-muted))", fontWeight: "450" });
        const v = document.createElement("div");
        v.textContent = pair.value;
        style(v, { color: "var(--dm-text-primary, var(--text-primary))", fontWeight: "500" });
        dl.append(k, v);
      }
      return dl;
    },

    list(comp) {
      const list = document.createElement(comp.ordered ? "ol" : "ul");
      style(list, {
        margin: "0",
        paddingLeft: "22px",
        display: "flex",
        flexDirection: "column",
        gap: "6px",
        fontSize: "14.5px",
        lineHeight: "1.55",
        color: "var(--dm-text-primary, var(--text-primary))",
      });
      for (const item of comp.items || []) {
        const li = document.createElement("li");
        li.textContent = item;
        list.appendChild(li);
      }
      return list;
    },

    code_block(comp) {
      const pre = document.createElement("pre");
      pre.textContent = comp.code ?? "";
      style(pre, {
        margin: "0",
        padding: "14px 16px",
        borderRadius: "14px",
        background: "var(--dm-surface-gray, var(--bg-primary))",
        border: "1px solid var(--dm-outline, var(--border-default))",
        color: "var(--dm-text-primary, var(--text-primary))",
        fontSize: "12.5px",
        overflowX: "auto",
        fontFamily: "var(--mono)",
      });
      return pre;
    },

    // `alert` and `toast` (aliased below).
    alert(comp) {
      return createHelperAlert(comp, true);
    },

    progress(comp) {
      const wrap = document.createElement("div");
      style(wrap, { display: "flex", flexDirection: "column", gap: "8px" });
      const pct = Math.max(0, Math.min(100, Number(comp.value) || 0));
      const top = document.createElement("div");
      style(top, { display: "flex", justifyContent: "space-between", fontSize: "13.5px" });
      const lbl = document.createElement("span");
      lbl.textContent = comp.label || "Progress";
      style(lbl, { color: "var(--dm-text-primary, var(--text-primary))", fontWeight: "450" });
      const val = document.createElement("span");
      val.textContent = `${pct}%`;
      style(val, { color: "var(--dm-text-secondary, var(--text-muted))", fontFamily: "var(--mono)" });
      top.append(lbl, val);

      const track = document.createElement("div");
      style(track, {
        height: "8px",
        borderRadius: "999px",
        background: "var(--dm-surface-gray, var(--bg-primary))",
        overflow: "hidden",
      });
      const bar = document.createElement("div");
      style(bar, {
        width: `${pct}%`,
        height: "100%",
        borderRadius: "999px",
        background: "var(--dm-text-primary, var(--accent-primary))",
      });
      track.appendChild(bar);
      wrap.append(top, track);
      return wrap;
    },

    // Form controls: `text_input`, `number_input`, `date_input`, `textarea`,
    // `select`, `multi_select`, `slider`, `switch` (aliased below); `type` picks the control.
    text_input(comp, surfaceRoot, inGrid, type) {
      const field = document.createElement("label");
      style(field, { display: "flex", flexDirection: "column", gap: "6px", fontSize: "13.5px" });
      const lbl = document.createElement("span");
      lbl.textContent = `${comp.label || comp.name}${comp.required ? " *" : ""}`;
      style(lbl, { fontWeight: "450", color: "var(--dm-text-secondary, var(--text-secondary))" });
      field.appendChild(lbl);

      let control;
      if (type === "textarea") {
        control = document.createElement("textarea");
        control.rows = Number(comp.rows) || 3;
        control.placeholder = comp.placeholder || "";
        control.value = comp.default_value || "";
      } else if (type === "select" || type === "multi_select") {
        control = document.createElement("select");
        if (type === "multi_select") control.multiple = true;
        for (const opt of comp.options || []) {
          const o = document.createElement("option");
          o.value = opt.value;
          o.textContent = opt.label;
          control.appendChild(o);
        }
      } else if (type === "switch") {
        const row = document.createElement("div");
        style(row, { display: "flex", alignItems: "center", gap: "10px", padding: "4px 0" });
        control = document.createElement("input");
        control.type = "checkbox";
        control.checked = !!comp.default_checked;
        row.append(control, lbl);
        field.replaceChildren(row);
      } else {
        control = document.createElement("input");
        // `input_type` comes from tool output: only allow plain text-like types.
        const requested = String(comp.input_type || "text").toLowerCase();
        control.type =
          type === "number_input"
            ? "number"
            : type === "date_input"
              ? "date"
              : type === "slider"
                ? "range"
                : SAFE_INPUT_TYPES.includes(requested)
                  ? requested
                  : "text";
        if (comp.placeholder) control.placeholder = comp.placeholder;
        if (comp.min !== undefined && comp.min !== null) control.min = String(comp.min);
        if (comp.max !== undefined && comp.max !== null) control.max = String(comp.max);
        if (comp.step !== undefined && comp.step !== null) control.step = String(comp.step);
        if (comp.default_value !== undefined && comp.default_value !== null) {
          control.value = String(comp.default_value);
        }
      }

      if (type !== "switch") {
        style(control, {
          padding: type === "select" ? "10px 34px 10px 14px" : "10px 14px",
          borderRadius: "12px",
          border: "1px solid var(--dm-outline, var(--border-default))",
          background: "var(--dm-surface-gray, var(--bg-primary))",
          color: "var(--dm-text-primary, var(--text-primary))",
          fontSize: "14px",
        });
        field.appendChild(control);
      }

      control.setAttribute("data-ui-field", comp.name || comp.label || "field");
      return field;
    },

    button(comp, surfaceRoot) {
      const btn = document.createElement("button");
      btn.type = "button";
      btn.textContent = comp.label || "Submit";
      btn.disabled = !!comp.disabled;
      const v = String(comp.variant || "primary").toLowerCase();
      const btnStyles =
        v === "primary"
          ? {
              background: "var(--primary-btn-bg, #121317)",
              color: "var(--primary-btn-fg, #fff)",
              border: "1px solid transparent",
            }
          : v === "danger"
            ? {
                background: "var(--accent-error)",
                color: "#fff",
                border: "1px solid var(--accent-error)",
              }
            : v === "outline"
              ? {
                  background: "transparent",
                  color: "var(--dm-text-primary, var(--text-primary))",
                  border: "1px solid var(--dm-outline, var(--border-default))",
                  boxShadow: "0 0 0 1px var(--dm-outline, var(--border-default))",
                }
              : v === "ghost"
                ? {
                    background: "transparent",
                    color: "var(--dm-text-secondary, var(--text-secondary))",
                    border: "1px solid transparent",
                  }
                : {
                    background: "var(--dm-surface-gray, var(--bg-primary))",
                    color: "var(--dm-text-primary, var(--text-secondary))",
                    border: "1px solid var(--dm-outline, var(--border-default))",
                  };
      style(btn, {
        padding: "10px 22px",
        borderRadius: "999px",
        fontSize: "14px",
        fontWeight: "500",
        letterSpacing: "-0.004em",
        cursor: btn.disabled ? "default" : "pointer",
        opacity: btn.disabled ? "0.45" : "1",
        alignSelf: "flex-start",
        ...btnStyles,
      });

      btn.addEventListener("click", () => {
        const scope = btn.closest('[data-ui-scope="true"]') || surfaceRoot;
        const inputs = scope ? Array.from(scope.querySelectorAll("[data-ui-field]")) : [];
        const actionId = comp.action_id || comp.label || "click";
        let msg;
        if (inputs.length > 0) {
          const lines = inputs.map((inp) => {
            const name = inp.getAttribute("data-ui-field");
            const val =
              inp.type === "checkbox"
                ? String(inp.checked)
                : inp.multiple
                  ? Array.from(inp.selectedOptions)
                      .map((o) => o.value)
                      .join(", ")
                  : inp.value;
            return `  ${name}: ${val}`;
          });
          msg = `[UI Event: Form submitted]\nAction: ${actionId}\nData:\n${lines.join("\n")}`;
        } else {
          msg = `[UI Event: Button clicked]\nAction: ${actionId}`;
        }
        if (sendMessageToComposer(msg)) {
          const orig = btn.textContent;
          btn.textContent = `${orig} ✓`;
          setTimeout(() => {
            btn.textContent = orig;
          }, 1800);
        }
      });
      return btn;
    },

  };
  RENDERERS.modal = RENDERERS.card;
  RENDERERS.toast = RENDERERS.alert;
  RENDERERS.number_input = RENDERERS.text_input;
  RENDERERS.date_input = RENDERERS.text_input;
  RENDERERS.textarea = RENDERERS.text_input;
  RENDERERS.select = RENDERERS.text_input;
  RENDERERS.multi_select = RENDERERS.text_input;
  RENDERERS.slider = RENDERERS.text_input;
  RENDERERS.switch = RENDERERS.text_input;

  // Recursively renders an inner `adk_ui::Component` inside a card body.
  const renderInnerComponent = (comp, surfaceRoot, inGrid = false) => {
    if (!comp || typeof comp !== "object") return document.createTextNode("");
    const type = String(comp.type || "").toLowerCase();
    const renderer = Object.prototype.hasOwnProperty.call(RENDERERS, type)
      ? RENDERERS[type]
      : null;
    return renderer ? renderer(comp, surfaceRoot, inGrid, type) : document.createTextNode("");
  };

  // Converts a top-level `UiResponse` into one or more standalone `.adk-ui-card` elements
  // so every chart, table, KPI section, or form renders as its own card in `.transcript`.
  const buildStandaloneCards = (uiResp) => {
    const comps = Array.isArray(uiResp.components) ? uiResp.components : [];
    const cards = [];

    // Pattern 1: `render_table` emits `[Text(H3, title), Table]` or `[Table]`.
    if (
      (comps.length === 2 &&
        comps[0]?.type === "text" &&
        comps[1]?.type === "table") ||
      (comps.length === 1 && comps[0]?.type === "table")
    ) {
      const title = comps.length === 2 ? comps[0].content : comps[0].title || "Data Table";
      const tableComp = comps.length === 2 ? comps[1] : comps[0];
      const { element, toolbar } = renderTableParts(tableComp);
      const { card, body } = createStandaloneCard({
        title,
        headerRight: toolbar,
        flushBody: true,
      });
      body.appendChild(element);
      cards.push(card);
      return cards;
    }

    // Pattern 2: `render_chart` emits `[Chart]`.
    if (comps.length === 1 && comps[0]?.type === "chart") {
      const chartComp = comps[0];
      const { card, body } = createStandaloneCard({
        title: chartComp.title || "Chart",
      });
      // `renderChartInner` only uses `title` for the SVG's accessible name.
      body.appendChild(renderChartInner(chartComp));
      cards.push(card);
      return cards;
    }

    // Pattern 3: `render_layout` or multi-component responses.
    let pendingTitle = null;
    let pendingSubtitle = null;

    for (let i = 0; i < comps.length; i++) {
      const comp = comps[i];
      if (!comp || typeof comp !== "object") continue;
      const t = String(comp.type || "").toLowerCase();

      // Capture leading dashboard title/caption to attach to the first section card.
      if (t === "text" && (comp.variant === "h1" || comp.variant === "h2") && !pendingTitle) {
        pendingTitle = comp.content;
        continue;
      }
      if (t === "text" && comp.variant === "caption" && pendingTitle && !pendingSubtitle) {
        pendingSubtitle = comp.content;
        continue;
      }

      // `build_metric_section` from `render_layout`: `Stack` of `[Text(H3, title), Grid(statCards)]`
      if (
        t === "stack" &&
        Array.isArray(comp.children) &&
        comp.children.length === 2 &&
        comp.children[0]?.type === "text" &&
        comp.children[1]?.type === "grid"
      ) {
        const secTitle = comp.children[0].content;
        const { card, body } = createStandaloneCard({
          title: secTitle,
          description: pendingTitle ? `${pendingTitle}${pendingSubtitle ? ` · ${pendingSubtitle}` : ""}` : null,
        });
        pendingTitle = null;
        pendingSubtitle = null;
        body.appendChild(renderInnerComponent(comp.children[1], card, true));
        cards.push(card);
        continue;
      }

      // Top-level `Card` or `Modal` (from `render_card`, `render_form`, `render_confirm`, `render_modal`, or `render_layout` sections)
      if (t === "card" || t === "modal") {
        const contentArr = Array.isArray(comp.content) ? comp.content : [];
        const singleTable = contentArr.length === 1 && contentArr[0]?.type === "table";
        const singleChart = contentArr.length === 1 && contentArr[0]?.type === "chart";

        let headerRight = null;
        let tableElement = null;
        if (singleTable) {
          const parts = renderTableParts(contentArr[0]);
          headerRight = parts.toolbar;
          tableElement = parts.element;
        }

        const { card, body } = createStandaloneCard({
          title: comp.title || pendingTitle || "Details",
          description: comp.description || (comp.title && pendingTitle ? pendingSubtitle : null),
          headerRight,
          flushBody: singleTable,
        });
        pendingTitle = null;
        pendingSubtitle = null;

        if (singleTable && tableElement) {
          body.appendChild(tableElement);
        } else if (singleChart) {
          body.appendChild(renderChartInner({ ...contentArr[0], title: contentArr[0].title || comp.title }));
        } else {
          for (const child of contentArr) {
            body.appendChild(renderInnerComponent(child, card, false));
          }
        }

        if (Array.isArray(comp.footer) && comp.footer.length) {
          const ftr = document.createElement("footer");
          style(ftr, {
            display: "flex",
            flexWrap: "wrap",
            gap: "10px",
            justifyContent: "flex-end",
            padding: "4px 24px 22px",
            background: "transparent",
          });
          for (const child of comp.footer) {
            ftr.appendChild(renderInnerComponent(child, card, false));
          }
          card.appendChild(ftr);
        }

        cards.push(card);
        continue;
      }

      if (t === "toast") {
        showHelperToast(comp);
        continue;
      }

      if (t === "alert") {
        pendingTitle = null;
        pendingSubtitle = null;
        cards.push(createHelperAlert(comp, false));
        continue;
      }

      // Any other top-level component (`progress`, `table`, `chart`, etc.)
      const { card, body } = createStandaloneCard({
        title: comp.title || pendingTitle || null,
        description: pendingSubtitle || null,
      });
      pendingTitle = null;
      pendingSubtitle = null;
      body.appendChild(renderInnerComponent(comp, card, false));
      cards.push(card);
    }

    return cards;
  };

  let helperToastStack = null;

  const showHelperToast = (comp) => {
    if (!comp || !document.body) return;
    if (!helperToastStack) {
      helperToastStack = document.createElement("div");
      helperToastStack.setAttribute("role", "status");
      helperToastStack.setAttribute("aria-live", "polite");
      style(helperToastStack, {
        position: "fixed",
        bottom: "24px",
        right: "24px",
        zIndex: "9999",
        display: "flex",
        flexDirection: "column-reverse",
        gap: "12px",
        maxWidth: "min(420px, calc(100vw - 40px))",
        pointerEvents: "none",
      });
      document.body.appendChild(helperToastStack);
    }

    const toast = document.createElement("div");
    style(toast, {
      pointerEvents: "auto",
      display: "flex",
      alignItems: "flex-start",
      gap: "14px",
      padding: "18px 20px",
      borderRadius: "22px",
      background: "var(--dm-surface-gray, var(--surface-card, #eff2f7))",
      border: "1px solid var(--dm-outline, rgba(33, 34, 38, 0.08))",
      boxShadow:
        "0 20px 44px -14px rgba(18, 19, 23, 0.16), 0 4px 14px -2px rgba(18, 19, 23, 0.08)",
      color: "var(--dm-text-primary, var(--text-primary, #121317))",
      fontSize: "15.5px",
    });

    const msgCol = document.createElement("div");
    style(msgCol, { flex: "1", minWidth: "0" });
    const title = document.createElement("div");
    title.textContent = comp.message || comp.title || "Notification";
    style(title, {
      fontWeight: "450",
      fontSize: "16px",
      lineHeight: "1.35",
      letterSpacing: "-0.008em",
      color: "var(--dm-text-primary, var(--text-primary, #121317))",
    });
    msgCol.appendChild(title);
    if (comp.description) {
      const desc = document.createElement("div");
      desc.textContent = comp.description;
      style(desc, {
        fontSize: "14px",
        lineHeight: "1.48",
        marginTop: "4px",
        color: "var(--dm-text-secondary, var(--text-secondary, #45474d))",
      });
      msgCol.appendChild(desc);
    }
    toast.appendChild(msgCol);

    const dismiss = () => toast.remove();
    if (comp.dismissible !== false) {
      const closeBtn = document.createElement("button");
      closeBtn.type = "button";
      closeBtn.textContent = "\u00d7";
      closeBtn.setAttribute("aria-label", "Dismiss");
      style(closeBtn, {
        border: "0",
        background: "transparent",
        color: "var(--dm-text-secondary, inherit)",
        cursor: "pointer",
        fontSize: "18px",
        lineHeight: "1",
        width: "28px",
        height: "28px",
        borderRadius: "50%",
        padding: "0",
      });
      closeBtn.addEventListener("click", dismiss);
      toast.appendChild(closeBtn);
    }

    const duration = Math.max(0, Number(comp.duration ?? 5000));
    if (duration > 0) {
      setTimeout(dismiss, duration);
    }
    helperToastStack.appendChild(toast);
  };

  // Extracts an `adk_ui::UiResponse` (`{ components: [...] }`) from a tool JSON object.
  const extractUiResponse = (obj) => {
    if (!obj || typeof obj !== "object") return null;
    if (
      Array.isArray(obj.components) &&
      obj.components.length > 0 &&
      typeof obj.components[0]?.type === "string"
    ) {
      return obj;
    }
    const candidates = [
      obj.functionResponse?.response,
      obj.function_response?.response,
      obj.response,
      obj.result,
    ];
    for (const c of candidates) {
      if (
        c &&
        typeof c === "object" &&
        Array.isArray(c.components) &&
        c.components.length > 0 &&
        typeof c.components[0]?.type === "string"
      ) {
        return c;
      }
    }
    return null;
  };

  // Promotes an `<article class="message">` into a clean unboxed slot when it
  // only holds `render_*` calls/results, so UI cards sit directly in `.transcript`
  // as their own standalone cards instead of inside a chat bubble.
  const ARTICLE_STATES = ["adk-ui-hidden", "adk-ui-unboxed", "adk-ui-split"];
  const SEGMENT_CLASSES = ["adk-ui-seg", "adk-ui-seg-start", "adk-ui-seg-end"];

  const setArticleState = (article, state) => {
    for (const cls of ARTICLE_STATES) toggleClass(article, cls, cls === state);
  };

  // Tags each run of visible, non-card children as a bubble segment (see the
  // `adk-ui-split` rules). Only classes are touched; children stay in place.
  const markSegments = (article, enabled) => {
    let runStart = true;
    let last = null;
    for (const ch of Array.from(article.children)) {
      if (ch.classList.contains("adk-ui-card")) {
        if (last) toggleClass(last, "adk-ui-seg-end", true);
        last = null;
        runStart = true;
        continue;
      }
      if (!enabled || ch.classList.contains("adk-ui-hidden")) {
        for (const cls of SEGMENT_CLASSES) toggleClass(ch, cls, false);
        continue;
      }
      toggleClass(ch, "adk-ui-seg", true);
      toggleClass(ch, "adk-ui-seg-start", runStart);
      toggleClass(ch, "adk-ui-seg-end", false);
      runStart = false;
      last = ch;
    }
    if (last) toggleClass(last, "adk-ui-seg-end", true);
  };

  const promoteArticleIfPureUi = (article) => {
    if (!article) return;
    const hasText = !!article.querySelector(".message-text");
    const visibleTools = Array.from(article.querySelectorAll("details.tool-call")).filter(
      (d) => !d.classList.contains("adk-ui-hidden")
    );
    const uiCards = article.querySelectorAll(".adk-ui-card");

    if (!hasText && visibleTools.length === 0) {
      if (uiCards.length === 0) {
        // Pure `Tool · render_*` call message with no text and no rendered cards: hide completely.
        setArticleState(article, "adk-ui-hidden");
      } else {
        // Pure `render_*` result message: strip outer chat bubble chrome so cards are standalone.
        setArticleState(article, "adk-ui-unboxed");
      }
      markSegments(article, false);
      return;
    }

    // Article has text or non-UI tool calls.
    if (uiCards.length === 0) {
      setArticleState(article, null);
      markSegments(article, false);
      return;
    }

    // Article has both chat content AND UI cards: separate the chat bubble from standalone UI cards.
    setArticleState(article, "adk-ui-split");
    markSegments(article, true);
  };

  // Articles whose layout must be re-evaluated, processed once per frame.
  const dirtyArticles = new Set();
  let flushScheduled = false;
  const flushDirtyArticles = () => {
    flushScheduled = false;
    const batch = Array.from(dirtyArticles);
    dirtyArticles.clear();
    for (const article of batch) {
      if (article.isConnected && article.querySelector(".adk-ui-card, details.tool-call")) {
        promoteArticleIfPureUi(article);
      }
    }
  };
  const markArticleDirty = (article) => {
    if (!article) return;
    dirtyArticles.add(article);
    if (!flushScheduled) {
      flushScheduled = true;
      requestAnimationFrame(flushDirtyArticles);
    }
  };

  const wireToolDetails = (details) => {
    if (seenDetails.has(details)) return;
    const pre = details.querySelector("pre");
    if (!pre || !pre.textContent) return;

    let parsed;
    try {
      parsed = JSON.parse(pre.textContent);
    } catch {
      return;
    }
    seenDetails.add(details);

    const article = details.closest("article.message");

    // A `render_*` *result* (`{ components }`, possibly wrapped in `response` /
    // `result`) is rendered as cards. Checked before the call-hiding branch
    // because such a block may also carry `name: "render_*"`.
    const uiResp = extractUiResponse(parsed);
    if (uiResp) {
      toggleClass(details, "adk-ui-hidden", true);
      const cards = buildStandaloneCards(uiResp);
      const parent = details.parentNode;
      for (const card of cards) {
        parent?.insertBefore(card, details);
      }
      markArticleDirty(article);
      return;
    }

    // Hide `Tool · render_*` functionCall blocks so only the rendered UI cards appear.
    const callName =
      parsed?.name || parsed?.functionCall?.name || parsed?.function_call?.name || "";
    if (typeof callName === "string" && callName.startsWith("render_")) {
      toggleClass(details, "adk-ui-hidden", true);
      markArticleDirty(article);
    }
  };

  // If an OAuth link is ever clicked directly in the chat transcript, open it
  // in the centered popup instead of navigating away.
  const isAuthLink = (el) =>
    el instanceof HTMLAnchorElement &&
    typeof el.href === "string" &&
    el.href.startsWith(AUTH_PREFIX);

  const wireLink = (anchor) => {
    if (seenLinks.has(anchor)) return;
    seenLinks.add(anchor);
    anchor.addEventListener("click", (event) => {
      event.preventDefault();
      openPopup(anchor.href);
    });
  };

  const scan = (root) => {
    if (!root || root.nodeType !== Node.ELEMENT_NODE) return;
    if (isAuthLink(root)) wireLink(root);
    if (root.matches && root.matches("details.tool-call")) wireToolDetails(root);
    if (root.querySelectorAll) {
      root.querySelectorAll("a[href]").forEach((a) => {
        if (isAuthLink(a)) wireLink(a);
      });
      root.querySelectorAll("details.tool-call").forEach(wireToolDetails);
    }
    const parentArticle = root.closest ? root.closest("article.message") : null;
    if (parentArticle) markArticleDirty(parentArticle);
  };

  const observer = new MutationObserver((mutations) => {
    for (const mutation of mutations) {
      if (mutation.type === "characterData") {
        // A tool `<pre>` whose JSON was still streaming: retry once the text changes.
        const details = mutation.target.parentElement?.closest("details.tool-call");
        if (details) wireToolDetails(details);
        continue;
      }
      for (const node of mutation.addedNodes) {
        if (node.nodeType !== Node.ELEMENT_NODE) continue;
        scan(node);
        if (modal && node.textContent && node.textContent.includes(RESUME_MARKER)) {
          hideModal();
        }
      }
    }
  });
  observer.observe(document.documentElement, { childList: true, subtree: true, characterData: true });
  scan(document.body);
})();
