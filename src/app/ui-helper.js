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

// AdSpace Agent — Google authorization helper & live `adk-ui` host for `/ui/`.
//
// Injected into the third-party ADK web UI as `<script type="module"
// src="/oauth2/helper.js">` (same origin: the page's CSP is `script-src
// 'self'`; `/app/ui-helper.js` serves the same bytes).
// 1. Listens to `/oauth2/events` (SSE, bound to the signed-in user) so the
//    instant a tool call needs Google OAuth consent, a centered popup window
//    opens automatically while the tool awaits completion on the server.
// 2. Watches `details.tool-call` blocks in `/ui/` for `adk-ui` (`UiResponse`)
//    component trees (`render_chart`, `render_table`, `render_layout`,
//    `render_card`, `render_form`, `render_confirm`, `render_modal`,
//    `render_alert`, `render_progress`, `render_toast`) and renders them with
//    the SAME renderer the studio uses (src/app/adk-ui/*), as standalone cards
//    directly in `.transcript`, toasts, or the modal overlay. This file is only
//    the host adapter: OAuth plumbing, DOM scanning of the React transcript,
//    and the bindings the shared renderer needs from a host.
//
// Imports are absolute because this module is served from `/oauth2/`.
import {
  buildStandaloneCards,
  classifyUiResponse,
  createStandaloneCard,
  extractUiResponse,
} from "/app/adk-ui/index.js";
import { bindSendMessage, renderInnerComponent } from "/app/adk-ui/components.js";
import { bindModalComponentRenderer, bindModalHost, showModalOverlay } from "/app/adk-ui/modal.js";
import { bindIconRenderer, showToast, style, svgEl } from "/app/dom.js";

const AUTH_PREFIX = "https://accounts.google.com/o/oauth2/";
const CHANNEL = "adspace:oauth";
const EVENTS_PATH = "/oauth2/events";
const ACK_PATH = "/oauth2/ack";
const RESUME_MARKER = "Google access has been granted for this chat";
const SHARED_CSS_PATH = "/app/adk-ui.css";

const seenLinks = new WeakSet();
const seenDetails = new WeakSet();
// `request_id`s of `auth_required` events already handled (the event carries
// only the consent URL and this opaque id, never the signed OAuth state).
const handledRequests = new Set();
// Upper bound on `handledRequests` (oldest ids are evicted first).
const MAX_HANDLED_REQUESTS = 200;
let popup = null;
let modal = null;

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

// ---------------------------------------------------------------------------
// Host bootstrap: stylesheet, helper rules, modal root, icons, send adapter
// ---------------------------------------------------------------------------

const head = document.head || document.documentElement;

// The shared renderer's stylesheet (tokens, toasts, modal, alert cards,
// markdown). The helper's own rules below live in a constructed stylesheet,
// which the cascade orders after every document stylesheet, so they win
// every tie against adk-ui.css.
const sharedCss = document.createElement("link");
sharedCss.rel = "stylesheet";
sharedCss.href = SHARED_CSS_PATH;
head.appendChild(sharedCss);

// CSP forbids inline `style="..."` attributes and inline `<style>` text, but
// CSSOM is not restricted: a constructed `CSSStyleSheet` (falling back to an
// empty `<style>` element plus `insertRule` where unsupported). Rules that
// must target React-owned nodes by class live here.
const helperSheet = (() => {
  try {
    if ("adoptedStyleSheets" in document && typeof CSSStyleSheet === "function") {
      const sheet = new CSSStyleSheet();
      document.adoptedStyleSheets = [...document.adoptedStyleSheets, sheet];
      return sheet;
    }
  } catch {
    /* fall through to the <style> element */
  }
  try {
    const el = document.createElement("style");
    el.setAttribute("data-adspace-helper", "");
    head.appendChild(el);
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
  // adk-ui.css ships the studio's `--dm-*` surface tokens; inside `/ui/` map
  // them onto the ADK UI's own palette so cards stay theme-native in both
  // light and dark mode (`!important` outranks adk-ui.css's dark-theme
  // blocks, whose selectors are more specific than `:root`).
  ":root { --dm-text-primary: var(--text-primary) !important; --dm-text-secondary: var(--text-secondary) !important; --dm-outline: var(--border-default) !important; --dm-surface-gray: var(--bg-primary) !important; --dm-surface-gray-subtle: var(--bg-canvas) !important; --dm-depth-shadow: var(--shadow-md) !important; --dm-modal-shadow: var(--shadow-md) !important; }",
  ".adk-ui-hidden { display: none !important; }",
  "article.message.adk-ui-unboxed, article.message.adk-ui-split { display: flex; flex-direction: column; background: transparent; border: 0; box-shadow: none; border-radius: 0; min-height: 0; overflow: visible; }",
  "article.message.adk-ui-unboxed { gap: 14px; }",
  "article.message.adk-ui-unboxed > header { display: none; }",
  `article.message.adk-ui-split > .adk-ui-seg { background: var(--surface-card); border-left: ${BUBBLE_BORDER}; border-right: ${BUBBLE_BORDER}; }`,
  `article.message.adk-ui-split > .adk-ui-seg-start { border-top: ${BUBBLE_BORDER}; border-top-left-radius: 10px; border-top-right-radius: 10px; }`,
  `article.message.adk-ui-split > .adk-ui-seg-end { border-bottom: ${BUBBLE_BORDER}; border-bottom-left-radius: 10px; border-bottom-right-radius: 10px; }`,
  "article.message.adk-ui-split > .adk-ui-card ~ .adk-ui-seg-start, article.message.adk-ui-split > .adk-ui-seg ~ .adk-ui-card, article.message.adk-ui-split > .adk-ui-card ~ .adk-ui-card { margin-top: 14px; }",
  // `tabs` component: the shared renderer marks the active tab with
  // `data-active`. `!important` because the host page styles `button`.
  "button.adk-ui-tab { border: 0 !important; border-bottom: 2px solid transparent !important; background: transparent !important; color: var(--dm-text-secondary) !important; }",
  'button.adk-ui-tab[data-active="true"] { color: var(--dm-text-primary) !important; border-bottom-color: var(--dm-text-primary) !important; }',
]);

// The modal overlay mounts in its own root next to React's `#root`, which is
// made inert while a dialog is open (same contract as studio.html).
const ensureModalRoot = () => {
  let root = document.getElementById("ui-modal-root");
  if (!root) {
    root = document.createElement("div");
    root.id = "ui-modal-root";
    root.className = "ui-modal-root hidden";
    root.setAttribute("role", "dialog");
    root.setAttribute("aria-modal", "true");
    document.body.appendChild(root);
  }
  return root;
};
bindModalHost({ root: ensureModalRoot(), inertTarget: document.getElementById("root") });
bindModalComponentRenderer(renderInnerComponent);

// `/ui/`'s CSP (`default-src 'self'`) blocks the Google Symbols web font, so
// the shared renderer's icons are drawn as inline stroke SVGs here (Feather
// style, matching the ADK UI's own icons). Unknown names fall back to "info".
const ICON_PATHS = {
  check_circle: ["M22 11.08V12a10 10 0 1 1-5.93-9.14", "M22 4 12 14.01l-3-3"],
  check: ["M20 6 9 17l-5-5"],
  warning: [
    "M10.29 3.86 1.82 18a2 2 0 0 0 1.71 3h16.94a2 2 0 0 0 1.71-3L13.71 3.86a2 2 0 0 0-3.42 0z",
    "M12 9v4",
    "M12 17h.01",
  ],
  error: ["M12 2a10 10 0 1 0 0 20 10 10 0 0 0 0-20z", "M12 8v4", "M12 16h.01"],
  info: ["M12 2a10 10 0 1 0 0 20 10 10 0 0 0 0-20z", "M12 16v-4", "M12 8h.01"],
  help: ["M12 2a10 10 0 1 0 0 20 10 10 0 0 0 0-20z", "M9.09 9a3 3 0 0 1 5.83 1c0 2-3 3-3 3", "M12 17h.01"],
  close: ["M18 6 6 18", "M6 6l12 12"],
};
bindIconRenderer((name, { size }) => {
  const px = size || 18;
  const svg = svgEl("svg", { viewBox: "0 0 24 24", "aria-hidden": "true", focusable: "false" });
  // Explicit sizing/stroke: the host page styles every `svg` (17px, 1.8 stroke).
  style(svg, {
    width: `${px}px`,
    height: `${px}px`,
    fill: "none",
    stroke: "currentColor",
    strokeWidth: "2",
    strokeLinecap: "round",
    strokeLinejoin: "round",
    flexShrink: "0",
    display: "inline-block",
  });
  for (const d of ICON_PATHS[name] || ICON_PATHS.info) svg.appendChild(svgEl("path", { d }));
  return svg;
});

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

// Couples to the ADK web UI's composer DOM (no stable ids): fills the
// textarea through React's value setter and clicks Send.
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
// While a run streams, the ADK UI turns its Send button into a Stop button;
// a programmatic send then must be refused rather than abort the run.
bindSendMessage(sendMessageToComposer, {
  isBusy: () => Boolean(document.querySelector("button.send-button.stop-button")),
});

// ---------------------------------------------------------------------------
// OAuth popup / fallback dialog
// ---------------------------------------------------------------------------

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
// This is the auth fallback dialog, not the adk-ui modal overlay.
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
// Live `adk-ui` (`UiResponse`) rendering into the React transcript
// ---------------------------------------------------------------------------

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

// Whether a tool block just streamed in live (as opposed to being replayed
// from session history on load or on a session switch): the helper has been
// running for a while and the block sits in the newest message. Dialogs only
// auto-open for live blocks; history keeps an "Open dialog" card instead.
const LIVE_AFTER_MS = 4000;
const startedAt = performance.now();
const isLiveBlock = (details) => {
  if (performance.now() - startedAt < LIVE_AFTER_MS) return false;
  const article = details.closest("article.message");
  return !!article && article.parentElement?.lastElementChild === article;
};

// A standalone card standing in for a `render_modal` result in the transcript
// so the dialog can be (re)opened later.
const createDialogCard = (spec) => {
  const { card, body } = createStandaloneCard({
    title: spec.title || "Dialog",
    description: "Dialog",
  });
  const open = document.createElement("button");
  open.type = "button";
  open.textContent = "Open dialog";
  style(open, {
    alignSelf: "flex-start",
    padding: "10px 22px",
    borderRadius: "999px",
    border: "1px solid var(--dm-outline)",
    background: "var(--dm-surface-gray)",
    color: "var(--dm-text-primary)",
    fontSize: "14px",
    fontWeight: "500",
    cursor: "pointer",
  });
  open.addEventListener("click", () => showModalOverlay(spec));
  body.appendChild(open);
  return card;
};

// Renders one `render_*` result block: toasts go to the toast stack, a
// `render_modal` becomes a dialog card (auto-opened when live), everything
// else (incl. `render_confirm`, kept inline so its buttons stay one click
// away) becomes standalone cards inserted before the hidden `<details>`.
const renderUiBlock = (details, parsed, uiResp) => {
  const classified = classifyUiResponse(parsed, uiResp);
  classified.toasts.forEach(showToast);
  const parent = details.parentNode;
  const insert = (el) => parent?.insertBefore(el, details);

  if (classified.kind === "toast") return;
  if (classified.kind === "modal") {
    insert(createDialogCard(classified.modal));
    if (isLiveBlock(details)) showModalOverlay(classified.modal);
    return;
  }
  const resp =
    classified.kind === "confirm" ? { components: [classified.confirmCard] } : classified.uiResp;
  for (const card of buildStandaloneCards(resp, classified.toolName).cards) insert(card);
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
    renderUiBlock(details, parsed, uiResp);
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
