// AdSpace Agent — Google authorization helper for the ADK web UI.
//
// Injected into `/ui/` from the same origin (`script-src 'self'`).
// Listens to `/oauth2/events` (SSE) so the instant a tool call needs Google
// OAuth consent, a centered popup window opens automatically while the tool
// awaits completion on the server — avoiding any "Please authorize access…"
// message in the chat window or any duplicate bottom-right button.
(() => {
  "use strict";
  const AUTH_PREFIX = "https://accounts.google.com/o/oauth2/";
  const CHANNEL = "adspace:oauth";
  const EVENTS_PATH = "/oauth2/events";
  const ACK_PATH = "/oauth2/ack";
  const RESUME_MARKER = "Google access has been granted for this chat";

  const seenLinks = new WeakSet();
  const handledStates = new Set();
  let popup = null;
  let modal = null;

  // CSP forbids inline `style="..."` attributes; CSSOM assignments are allowed.
  const style = (el, props) => {
    for (const [k, v] of Object.entries(props)) el.style[k] = v;
  };

  const hideModal = () => {
    if (modal) modal.remove();
    modal = null;
  };

  const openPopup = (url) => {
    const w = 520;
    const h = 720;
    const left = Math.max(0, (window.screenX || 0) + ((window.outerWidth || w) - w) / 2);
    const top = Math.max(0, (window.screenY || 0) + ((window.outerHeight || h) - h) / 2);
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
      background: "rgba(0, 0, 0, 0.55)",
      font: "14px/1.45 system-ui, sans-serif",
    });

    const box = document.createElement("div");
    style(box, {
      maxWidth: "380px",
      width: "90%",
      padding: "20px 22px",
      borderRadius: "14px",
      background: "#1f1f1f",
      color: "#f5f5f5",
      boxShadow: "0 12px 36px rgba(0, 0, 0, 0.45)",
    });

    const title = document.createElement("div");
    title.textContent = "Google authorization required";
    style(title, { fontSize: "16px", fontWeight: "600", marginBottom: "6px" });

    const detail = document.createElement("div");
    const names = shortScopes(scopes);
    detail.textContent = `Click below to open the Google consent popup${names ? ` (${names})` : ""}. Your request will continue automatically as soon as you grant access.`;
    style(detail, { opacity: "0.88", marginBottom: "16px" });

    const row = document.createElement("div");
    style(row, { display: "flex", gap: "10px", justifyContent: "flex-end" });

    const cancel = document.createElement("button");
    cancel.type = "button";
    cancel.textContent = "Cancel";
    style(cancel, {
      border: "0",
      background: "transparent",
      color: "inherit",
      opacity: "0.75",
      cursor: "pointer",
      padding: "8px 12px",
      font: "inherit",
    });
    cancel.addEventListener("click", hideModal);

    const button = document.createElement("button");
    button.type = "button";
    button.textContent = "Continue with Google";
    style(button, {
      border: "0",
      borderRadius: "8px",
      padding: "8px 16px",
      cursor: "pointer",
      background: "#8ab4f8",
      color: "#202124",
      fontWeight: "600",
      font: "inherit",
    });
    button.addEventListener("click", () => {
      if (openPopup(authUrl)) hideModal();
    });

    row.append(cancel, button);
    box.append(title, detail, row);
    modal.appendChild(box);
    document.body.appendChild(modal);
  };

  const handleAuthRequired = async (data) => {
    if (!data || !data.auth_url || !data.state) return;
    if (handledStates.has(data.state)) return;
    handledStates.add(data.state);

    const win = openPopup(data.auth_url);
    const opened = !!win;
    try {
      const resp = await fetch(ACK_PATH, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ state: data.state, opened }),
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
      const refresh = Array.from(document.querySelectorAll("button")).find((b) =>
        /^\s*refresh\s*$/i.test(b.textContent || "")
      );
      if (refresh) refresh.click();
    }
  };

  // Real-time SSE stream from the server: fires the instant a tool call needs
  // OAuth consent, before the tool returns to the LLM.
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

  // Cross-window completion report from `/oauth2/callback`.
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
    if (isAuthLink(root)) wireLink(root);
    if (root && root.querySelectorAll) {
      root.querySelectorAll("a[href]").forEach((a) => {
        if (isAuthLink(a)) wireLink(a);
      });
    }
  };

  const observer = new MutationObserver((mutations) => {
    for (const mutation of mutations) {
      for (const node of mutation.addedNodes) {
        if (node.nodeType !== Node.ELEMENT_NODE) continue;
        scan(node);
        if (modal && node.textContent && node.textContent.includes(RESUME_MARKER)) {
          hideModal();
        }
      }
    }
  });
  observer.observe(document.documentElement, { childList: true, subtree: true });
  scan(document.body);
})();
