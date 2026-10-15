// AdSpace Agent — Google authorization helper for the ADK web UI.
//
// Injected into the UI page by the server (same origin, so the UI's CSP of
// `script-src 'self'` allows it). It watches the chat for Google OAuth links,
// opens the consent screen in a centred popup — automatically when the browser
// still treats the moment as user-activated, otherwise from an in-page
// "Authorize" card — and hides the card / refreshes the chat when the callback
// page reports success. The report arrives over a same-origin BroadcastChannel
// (Google's sign-in pages set COOP `same-origin`, which severs `window.opener`),
// with `postMessage` to the opener as a second path.
(() => {
  "use strict";
  const AUTH_PREFIX = "https://accounts.google.com/o/oauth2/";
  const CHANNEL = "adspace:oauth";
  const RESUME_MARKER = "Google access has been granted for this chat";
  // Links expire with their signed state (15 minutes server-side).
  const LINK_TTL_SECONDS = 15 * 60;
  const seen = new WeakSet();
  // Hrefs already acted on: the UI re-renders the chat on every poll, and a
  // re-created link node must not re-open the popup or reset the card.
  const handled = new Set();
  const dismissed = new Set();
  let popup = null;
  let card = null;
  let cardAnchor = null;

  const isAuthLink = (el) =>
    el instanceof HTMLAnchorElement && typeof el.href === "string" && el.href.startsWith(AUTH_PREFIX);

  // Whether the link's signed `state` (base64url JSON payload + "." + MAC)
  // is still fresh. Unknown formats count as fresh.
  const isFresh = (url) => {
    try {
      const state = new URL(url).searchParams.get("state") || "";
      const payload = state.split(".")[0].replace(/-/g, "+").replace(/_/g, "/");
      const claims = JSON.parse(atob(payload + "===".slice((payload.length + 3) % 4)));
      return typeof claims.iat !== "number" || Date.now() / 1000 - claims.iat < LINK_TTL_SECONDS;
    } catch {
      return true;
    }
  };

  const scopeNames = (url) => {
    try {
      return (new URL(url).searchParams.get("scope") || "")
        .split(/\s+/)
        .filter(Boolean)
        .map((s) => s.slice(s.lastIndexOf("/") + 1))
        .join(", ");
    } catch {
      return "";
    }
  };

  // CSP forbids style attributes; CSSOM assignments are fine.
  const style = (el, props) => {
    for (const [k, v] of Object.entries(props)) el.style[k] = v;
  };

  const hideCard = () => {
    if (card) card.remove();
    card = null;
    cardAnchor = null;
  };

  const openPopup = (url) => {
    const w = 520;
    const h = 720;
    const left = Math.max(0, (window.screenX || 0) + ((window.outerWidth || w) - w) / 2);
    const top = Math.max(0, (window.screenY || 0) + ((window.outerHeight || h) - h) / 2);
    const win = window.open(url, "adspace_google_auth", `popup=yes,width=${w},height=${h},left=${left},top=${top}`);
    if (win) {
      popup = win;
      try { win.focus(); } catch { /* ignore */ }
    }
    return win;
  };

  const showCard = (anchor, waiting) => {
    hideCard();
    cardAnchor = anchor;
    card = document.createElement("div");
    card.setAttribute("role", "dialog");
    card.setAttribute("aria-label", "Google authorization");
    style(card, {
      position: "fixed", right: "20px", bottom: "20px", zIndex: "2147483000", maxWidth: "360px",
      padding: "14px 16px", borderRadius: "12px", background: "#1f1f1f", color: "#f5f5f5",
      boxShadow: "0 8px 28px rgba(0,0,0,.35)", font: "14px/1.45 system-ui, sans-serif",
    });
    const title = document.createElement("div");
    title.textContent = waiting ? "Waiting for Google\u2026" : "Google needs your permission";
    style(title, { fontWeight: "600", marginBottom: "4px" });
    const detail = document.createElement("div");
    const names = scopeNames(anchor.href);
    detail.textContent = waiting
      ? "Finish signing in in the popup; the chat continues automatically."
      : `The agent needs access${names ? ` (${names})` : ""} to continue.`;
    style(detail, { opacity: ".85", marginBottom: "10px" });
    const row = document.createElement("div");
    style(row, { display: "flex", gap: "8px", alignItems: "center" });
    const button = document.createElement("button");
    button.type = "button";
    button.textContent = waiting ? "Reopen" : "Authorize Google access";
    style(button, {
      border: "0", borderRadius: "8px", padding: "8px 14px", cursor: "pointer",
      background: "#8ab4f8", color: "#202124", fontWeight: "600", font: "inherit",
    });
    button.addEventListener("click", () => {
      if (openPopup(anchor.href)) showCard(anchor, true);
    });
    const dismiss = document.createElement("button");
    dismiss.type = "button";
    dismiss.textContent = "Dismiss";
    style(dismiss, { border: "0", background: "transparent", color: "inherit", opacity: ".7", cursor: "pointer", font: "inherit" });
    dismiss.addEventListener("click", () => {
      dismissed.add(anchor.href);
      hideCard();
    });
    row.append(button, dismiss);
    card.append(title, detail, row);
    document.body.appendChild(card);
  };

  // True when the chat already contains the server's resume turn after `anchor`.
  const resolvedAfter = (anchor) => {
    const walker = document.createTreeWalker(document.body, NodeFilter.SHOW_TEXT);
    let node;
    while ((node = walker.nextNode())) {
      const pos = anchor.compareDocumentPosition(node);
      if (!(pos & Node.DOCUMENT_POSITION_FOLLOWING) || pos & Node.DOCUMENT_POSITION_CONTAINED_BY) continue;
      if (node.nodeValue && node.nodeValue.includes(RESUME_MARKER)) return true;
    }
    return false;
  };

  const handleLink = (anchor) => {
    if (seen.has(anchor)) return;
    seen.add(anchor);
    anchor.addEventListener("click", (event) => {
      event.preventDefault();
      showCard(anchor, !!openPopup(anchor.href));
    });
    if (handled.has(anchor.href)) {
      // Same link re-rendered: keep the card (whatever its state) pointing at it.
      if (card && cardAnchor && cardAnchor.href === anchor.href) cardAnchor = anchor;
      return;
    }
    handled.add(anchor.href);
    if (dismissed.has(anchor.href) || !isFresh(anchor.href) || resolvedAfter(anchor)) return;
    const active = !!(navigator.userActivation && navigator.userActivation.isActive);
    showCard(anchor, active && !!openPopup(anchor.href));
  };

  const scan = (root) => {
    if (isAuthLink(root)) handleLink(root);
    if (root.querySelectorAll) root.querySelectorAll("a[href]").forEach((a) => { if (isAuthLink(a)) handleLink(a); });
  };

  const observer = new MutationObserver((mutations) => {
    for (const mutation of mutations) {
      for (const node of mutation.addedNodes) {
        if (node.nodeType !== Node.ELEMENT_NODE) continue;
        scan(node);
        if (card && node.textContent && node.textContent.includes(RESUME_MARKER)) hideCard();
      }
    }
    if (cardAnchor && !document.contains(cardAnchor)) {
      // Re-rendered elsewhere? Re-point at the live copy, else drop the card.
      const live = Array.from(document.querySelectorAll("a[href]")).find((a) => a.href === cardAnchor.href);
      if (live) cardAnchor = live;
      else hideCard();
    }
  });
  observer.observe(document.documentElement, { childList: true, subtree: true });
  scan(document.body);

  const onReport = (data) => {
    if (!data || data.type !== CHANNEL) return;
    if (data.status === "ok") {
      hideCard();
      if (popup && !popup.closed) { try { popup.close(); } catch { /* ignore */ } }
      popup = null;
      // Nudge the UI to reload the session now instead of on its next poll.
      const refresh = Array.from(document.querySelectorAll("button")).find((b) => /^\s*refresh\s*$/i.test(b.textContent || ""));
      if (refresh) refresh.click();
    } else if (cardAnchor) {
      showCard(cardAnchor, false);
    }
  };
  if ("BroadcastChannel" in window) {
    new BroadcastChannel(CHANNEL).addEventListener("message", (event) => onReport(event.data));
  }
  window.addEventListener("message", (event) => {
    if (event.origin === location.origin) onReport(event.data);
  });
})();
