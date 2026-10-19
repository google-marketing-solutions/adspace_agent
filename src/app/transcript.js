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
// Transcript & Canvas Rendering
// ---------------------------------------------------------------------------

import { closePopover, openPopover, showToast, style } from "./dom.js";
import { formatExactInt, formatUsdShort, initialsOf } from "./format.js";
import { APP_NAME, els, prefs, state } from "./state.js";
import { parseMarkdownBlocksInto } from "./markdown.js";
import { animateCounter } from "./usage/counter.js";
import { cachedEventUsage, isUserEvent } from "./usage/telemetry.js";
import { applyPreferences } from "./prefs.js";
import { showModalOverlay } from "./adk-ui/modal.js";
import { buildStandaloneCards, classifyUiResponse, extractUiResponse } from "./adk-ui/index.js";
import { updateSessionUsageUi } from "./usage/session-usage.js";

export const isPlainTextPart = (part) =>
  part &&
  typeof part.text === "string" &&
  !part.thinking &&
  !part.functionCall &&
  !part.function_call &&
  !part.functionResponse &&
  !part.function_response &&
  !part.inlineData &&
  !part.inline_data;

export const coalesceParts = (parts) => {
  const out = [];
  for (const part of Array.isArray(parts) ? parts : []) {
    if (!part || typeof part !== "object") continue;
    if (isPlainTextPart(part)) {
      const last = out[out.length - 1];
      if (last && isPlainTextPart(last)) {
        last.text += part.text;
        continue;
      }
    } else if (typeof part.thinking === "string") {
      const last = out[out.length - 1];
      if (last && typeof last.thinking === "string" && !isPlainTextPart(last)) {
        last.thinking += part.thinking;
        continue;
      }
    }
    out.push({ ...part });
  }
  return out;
};

export const extractConcatenatedText = (parts) =>
  (Array.isArray(parts) ? parts : [])
    .map((p) => (typeof p?.text === "string" ? p.text : ""))
    .join("");

export const mergeStreamedEvent = (prev, incoming) => {
  const prevParts = Array.isArray(prev?.content?.parts) ? prev.content.parts : [];
  const nextParts = Array.isArray(incoming?.content?.parts) ? incoming.content.parts : [];
  const mergedUsage =
    incoming?.usage_metadata ??
    incoming?.usageMetadata ??
    prev?.usage_metadata ??
    prev?.usageMetadata ??
    null;

  if (incoming.partial) {
    return {
      ...prev,
      ...incoming,
      usage_metadata: mergedUsage,
      content: {
        ...(prev?.content || {}),
        ...(incoming?.content || {}),
        parts: coalesceParts([...prevParts, ...nextParts]),
      },
    };
  }

  const contentComplete = incoming?.provider_metadata?.content_complete === true;
  const prevText = extractConcatenatedText(prevParts);
  const nextText = extractConcatenatedText(nextParts);

  let finalParts;
  if (nextParts.length === 0 && prevParts.length > 0) {
    finalParts = coalesceParts(prevParts);
  } else if (
    prev?.partial &&
    !contentComplete &&
    prevText &&
    (!nextText || !nextText.startsWith(prevText))
  ) {
    finalParts = coalesceParts([...prevParts, ...nextParts]);
  } else {
    finalParts = coalesceParts(nextParts);
  }

  return {
    ...prev,
    ...incoming,
    usage_metadata: mergedUsage,
    partial: false,
    content: {
      ...(prev?.content || {}),
      ...(incoming?.content || {}),
      parts: finalParts,
    },
  };
};

export const shownOverlayIds = new Set();
let overlayTriggersById = new Map();

export const rebuildFromSessionEvents = (
  events,
  { forceCanvas = false, suppressOverlays = false } = {}
) => {
  state.currentEvents = Array.isArray(events) ? events : [];
  const prevSurfaceIds = state.canvasSurfaces.map((s) => s.id).join("|");
  const existingById = new Map(state.canvasSurfaces.map((s) => [s.id, s]));
  const nextSurfaces = [];
  const nextOverlays = new Map();

  state.currentEvents.forEach((ev, idx) => {
    const parts = ev?.content?.parts || [];
    let uiIndex = 0;
    parts.forEach((part) => {
      const uiResp = extractUiResponse(part);
      if (!uiResp) return;
      const slotIndex = uiIndex++;
      const surfaceId = `surface-${ev.id || idx}-${slotIndex}`;
      const overlayKey = `${state.activeSession?.id || "anon"}:${ev.id || idx}:${slotIndex}`;
      const classified = classifyUiResponse(part, uiResp);

      if (classified.toasts.length > 0) {
        const toastKey = `toast:${overlayKey}`;
        if (suppressOverlays) {
          shownOverlayIds.add(toastKey);
        } else if (!ev.partial && !shownOverlayIds.has(toastKey)) {
          shownOverlayIds.add(toastKey);
          classified.toasts.forEach(showToast);
        }
      }

      if (classified.kind === "toast") {
        return;
      }

      if (classified.kind === "modal") {
        const modalKey = `modal:${overlayKey}`;
        nextOverlays.set(surfaceId, {
          id: surfaceId,
          title: classified.modal?.title || "Dialog",
          cta: "Open Modal",
          icon: "dialogs",
          onOpen: () => showModalOverlay(classified.modal),
        });
        if (suppressOverlays) {
          shownOverlayIds.add(modalKey);
        } else if (!ev.partial && !shownOverlayIds.has(modalKey)) {
          shownOverlayIds.add(modalKey);
          showModalOverlay(classified.modal);
        }
        return;
      }

      if (classified.kind === "confirm") {
        const confirmKey = `confirm:${overlayKey}`;
        nextOverlays.set(surfaceId, {
          id: surfaceId,
          title: classified.confirmCard?.title || "Confirm Action",
          cta: "Review",
          icon: "help",
          onOpen: () => showModalOverlay(classified.confirmCard, { isConfirm: true }),
        });
        if (suppressOverlays) {
          shownOverlayIds.add(confirmKey);
        } else if (!ev.partial && !shownOverlayIds.has(confirmKey)) {
          shownOverlayIds.add(confirmKey);
          showModalOverlay(classified.confirmCard, { isConfirm: true });
        }
        return;
      }

      const prev = existingById.get(surfaceId);
      if (prev) {
        nextSurfaces.push(prev);
      } else {
        const built = buildStandaloneCards(classified.uiResp, classified.toolName);
        if (built.cards.length > 0) {
          nextSurfaces.push({
            id: surfaceId,
            title: built.title,
            raw: classified.uiResp,
            cards: built.cards,
          });
        }
      }
    });
  });
  state.canvasSurfaces = nextSurfaces;
  overlayTriggersById = nextOverlays;
  const nextSurfaceIds = state.canvasSurfaces.map((s) => s.id).join("|");

  renderTranscript();
  if (forceCanvas || prevSurfaceIds !== nextSurfaceIds) {
    renderCanvas();
  }
};

export const focusSurfaceOnCanvas = (surfaceId) => {
  state.activeSurfaceId = surfaceId;
  prefs.canvasMode = "focused";
  applyPreferences({ rerender: false });
  renderCanvas();
  const target = document.querySelector(`[data-surface-id="${CSS.escape(surfaceId)}"]`);
  if (target) {
    target.scrollIntoView({ behavior: "smooth", block: "start" });
  }
};

export const createAvatarElement = (isUser, authorLabel) => {
  const avatar = document.createElement("span");
  avatar.className = `avatar ${isUser ? "avatar-user" : "avatar-agent"}`;
  avatar.title = authorLabel;
  if (isUser) {
    if (state.currentUser?.picture) {
      const img = document.createElement("img");
      img.src = state.currentUser.picture;
      img.alt = authorLabel;
      img.referrerPolicy = "no-referrer";
      img.onerror = () => {
        avatar.textContent = initialsOf(authorLabel);
      };
      avatar.appendChild(img);
    } else {
      avatar.textContent = initialsOf(authorLabel);
    }
  } else {
    avatar.innerHTML = `<span class="google-symbols filled" aria-hidden="true">robot</span>`;
  }
  return avatar;
};

export const parseUiEventMessage = (text) => {
  if (typeof text !== "string" || !text.startsWith("[UI Event:")) return null;
  const match = text.match(/^\[UI Event:\s*([^\]]+)\]\s*(?:\r?\n|$)([\s\S]*)$/);
  if (!match) return null;
  const eventType = match[1].trim();
  const rest = match[2] || "";
  let action = "";
  const fields = [];
  let inData = false;
  for (const rawLine of rest.split(/\r?\n/)) {
    if (!rawLine.trim()) continue;
    if (!inData && rawLine.startsWith("Action:")) {
      action = rawLine.slice("Action:".length).trim();
      continue;
    }
    if (!inData && rawLine.trim() === "Data:") {
      inData = true;
      continue;
    }
    const kv = rawLine.match(/^\s*([^:]+):\s*(.*)$/);
    if (kv) {
      fields.push({ key: kv[1].trim(), value: kv[2].trim() });
    }
  }
  return { eventType, action, fields };
};

export const renderUiEventInto = (container, parsed) => {
  container.replaceChildren();
  const wrap = document.createElement("div");
  wrap.className = "ui-event-card";

  const header = document.createElement("div");
  header.className = "ui-event-header";

  const badge = document.createElement("span");
  badge.className = "ui-event-badge";
  const icon = document.createElement("span");
  icon.className = "google-symbols";
  icon.setAttribute("aria-hidden", "true");
  icon.textContent = /form/i.test(parsed.eventType) ? "fact_check" : "ads_click";
  const title = document.createElement("span");
  title.className = "ui-event-title";
  title.textContent = parsed.eventType;
  badge.append(icon, title);
  header.appendChild(badge);

  if (parsed.action) {
    const actionCode = document.createElement("code");
    actionCode.className = "ui-event-action";
    actionCode.textContent = parsed.action;
    header.appendChild(actionCode);
  }
  wrap.appendChild(header);

  if (parsed.fields.length > 0) {
    const grid = document.createElement("div");
    grid.className = "ui-event-fields";
    for (const f of parsed.fields) {
      const row = document.createElement("div");
      row.className = "ui-event-field";
      const keyEl = document.createElement("span");
      keyEl.className = "ui-event-key";
      keyEl.textContent = f.key;
      const valEl = document.createElement("code");
      valEl.className = "ui-event-val";
      valEl.textContent = f.value || "—";
      row.append(keyEl, valEl);
      grid.appendChild(row);
    }
    wrap.appendChild(grid);
  }

  container.appendChild(wrap);
};

export const updateUsageFooter = (parentEl, { isUser, usage }) => {
  const usageSig =
    !isUser && usage && prefs.showMessageUsage
      ? `${usage.totalTokens}:${usage.inputTokens}:${usage.outputTokens}:${usage.thinkingTokens}:${usage.cacheReadTokens}:${prefs.showCost ? usage.totalCost.toFixed(6) : "nocost"}`
      : "";
  if (parentEl.dataset.usageSig === usageSig) return;
  parentEl.dataset.usageSig = usageSig;

  let usageEl = parentEl.querySelector(":scope > .message-usage");
  if (usageSig && usage) {
    if (!usageEl) {
      usageEl = document.createElement("button");
      usageEl.type = "button";
      usageEl.className = "message-usage";
      usageEl.setAttribute("aria-haspopup", "dialog");
      if (els.sessionUsagePopover?.id) usageEl.setAttribute("aria-controls", els.sessionUsagePopover.id);
      usageEl.addEventListener("click", (e) => {
        e.stopPropagation();
        if (els.sessionUsagePopover && prefs.showSessionUsage) {
          closePopover(els.settingsBtn, els.settingsMenu);
          closePopover(els.userBtn, els.userMenu);
          updateSessionUsageUi();
          openPopover(els.sessionUsageBtn, els.sessionUsagePopover);
        }
      });
      parentEl.appendChild(usageEl);
    }
    let leftSpan = usageEl.querySelector(":scope > .message-usage-tokens");
    if (!leftSpan) {
      leftSpan = document.createElement("span");
      leftSpan.className = "message-usage-tokens";
      usageEl.prepend(leftSpan);
    }
    animateCounter(
      leftSpan,
      usage.totalTokens,
      (v) => {
        const r = usage.totalTokens > 0 ? v / usage.totalTokens : 1;
        const detailParts = [
          `${formatExactInt(usage.inputTokens * r)} in`,
          `${formatExactInt(usage.outputTokens * r)} out`,
        ];
        if (usage.thinkingTokens > 0) {
          detailParts.push(`${formatExactInt(usage.thinkingTokens * r)} thinking`);
        }
        if (usage.cacheReadTokens > 0) {
          detailParts.push(`${formatExactInt(usage.cacheReadTokens * r)} cached`);
        }
        return `${formatExactInt(v)} tok (${detailParts.join(" · ")})`;
      },
      {
        formatSig: `${usage.inputTokens}:${usage.outputTokens}:${usage.thinkingTokens}:${usage.cacheReadTokens}`,
      }
    );
    usageEl.title = `gen_ai.request.model=${usage.model} · gen_ai.usage.input_tokens=${usage.inputTokens} · gen_ai.usage.output_tokens=${usage.outputTokens} · gen_ai.usage.total_tokens=${usage.totalTokens}`;
    let rightSpan = usageEl.querySelector(":scope > .message-usage-cost");
    if (prefs.showCost) {
      if (!rightSpan) {
        rightSpan = document.createElement("span");
        rightSpan.className = "message-usage-cost";
        usageEl.appendChild(rightSpan);
      }
      animateCounter(rightSpan, usage.totalCost, formatUsdShort);
    } else if (rightSpan) {
      rightSpan.remove();
    }
  } else if (usageEl) {
    usageEl.remove();
  }
};

export const updateMessageArticle = (
  article,
  { isUser, partial, textContent, visibleTools, usage }
) => {
  // 1. Streaming indicator
  let st = article.querySelector(":scope > .streaming-tag");
  if (partial) {
    if (!st) {
      st = document.createElement("div");
      st.className = "streaming-tag";
      st.textContent = "streaming…";
      article.prepend(st);
    }
  } else if (st) {
    st.remove();
  }

  // 2. Message text container (updated in place only when text changed)
  let textEl = article.querySelector(":scope > .message-text");
  if (textContent) {
    if (!textEl) {
      textEl = document.createElement("div");
      textEl.className = "message-text adk-ui-md";
      if (st && st.nextSibling) {
        article.insertBefore(textEl, st.nextSibling);
      } else if (st) {
        article.appendChild(textEl);
      } else {
        article.prepend(textEl);
      }
    }
    if (textEl._renderedText !== textContent) {
      textEl._renderedText = textContent;
      const parsedUiEvent = isUser ? parseUiEventMessage(textContent) : null;
      if (parsedUiEvent) {
        article.classList.add("message-ui-event");
        textEl.classList.add("message-text-ui-event");
        renderUiEventInto(textEl, parsedUiEvent);
      } else {
        article.classList.remove("message-ui-event");
        textEl.classList.remove("message-text-ui-event");
        if (isUser) {
          textEl.textContent = textContent;
        } else {
          textEl.replaceChildren();
          parseMarkdownBlocksInto(textEl, textContent);
        }
      }
    }
  } else {
    article.classList.remove("message-ui-event");
    if (textEl) textEl.remove();
  }

  // Clear any legacy usage footer inside bubble
  const legacyUsage = article.querySelector(":scope > .message-usage");
  if (legacyUsage) legacyUsage.remove();
};

// Serialised tool parts are compared on every render; loaded events keep
// the same part objects, so memoise the JSON per object.
export const toolPartSigCache = new WeakMap();
export const toolPartSig = (tp) => {
  if (!tp || typeof tp !== "object") return String(tp);
  let sig = toolPartSigCache.get(tp);
  if (sig === undefined) {
    sig = JSON.stringify(tp);
    toolPartSigCache.set(tp, sig);
  }
  return sig;
};

export const updateMessageCol = (col, item) => {
  const hasBubble = item.partial || Boolean(item.textContent);
  let article = col.querySelector(":scope > article.message");

  if (hasBubble) {
    if (!article) {
      article = document.createElement("article");
      article.className = `message ${item.isUser ? "message-user" : "message-agent"}`;
      col.prepend(article);
    }
    updateMessageArticle(article, item);
  } else if (article) {
    article.remove();
  }

  // Tool calls live outside the chat bubble as soft-gray pills
  const toolSigs = item.visibleTools.map(toolPartSig);
  const toolsSig = toolSigs.join("\u0001");
  if (col.dataset.toolsSig !== toolsSig) {
    col.dataset.toolsSig = toolsSig;
    let toolGroup = col.querySelector(":scope > .chat-tool-calls");
    if (item.visibleTools.length === 0) {
      if (toolGroup) toolGroup.remove();
    } else {
      if (!toolGroup) {
        toolGroup = document.createElement("div");
        toolGroup.className = "chat-tool-calls";
        const beforeEl =
          col.querySelector(":scope > .chat-canvas-buttons") ||
          col.querySelector(":scope > .message-usage");
        if (beforeEl) {
          col.insertBefore(toolGroup, beforeEl);
        } else {
          col.appendChild(toolGroup);
        }
      }
      // Reconcile by position: keep an unchanged <details> (so its open
      // state survives) and only build the ones that are new or changed.
      const existing = Array.from(toolGroup.children);
      item.visibleTools.forEach((tp, i) => {
        const sig = toolSigs[i];
        const prevEl = existing[i];
        if (prevEl && prevEl._toolSig === sig) return;
        const details = document.createElement("details");
        details.className = "tool-call";
        details._toolSig = sig;
        const summary = document.createElement("summary");
        const isResp = "functionResponse" in tp || "function_response" in tp;
        const name =
          tp.name ||
          tp.functionCall?.name ||
          tp.function_call?.name ||
          tp.functionResponse?.name ||
          tp.function_response?.name ||
          "tool";
        summary.textContent = isResp ? `Result · ${name}` : `Tool · ${name}`;
        const pre = document.createElement("pre");
        pre.textContent = JSON.stringify(tp, null, 2);
        details.append(summary, pre);
        if (prevEl) {
          toolGroup.replaceChild(details, prevEl);
        } else {
          toolGroup.appendChild(details);
        }
      });
      while (toolGroup.children.length > item.visibleTools.length) {
        toolGroup.lastElementChild.remove();
      }
    }
  }

  // Canvas jump buttons live outside the chat bubble (styled like .cover__buttons)
  const surfacesSig = item.surfaceRefs.map((s) => s.id).join("|");
  if (col.dataset.surfacesSig !== surfacesSig) {
    col.dataset.surfacesSig = surfacesSig;
    let btnGroup = col.querySelector(":scope > .chat-canvas-buttons");
    if (item.surfaceRefs.length === 0) {
      if (btnGroup) btnGroup.remove();
    } else {
      if (!btnGroup) {
        btnGroup = document.createElement("div");
        btnGroup.className = "chat-canvas-buttons";
        const standaloneUsage = col.querySelector(":scope > .message-usage");
        if (standaloneUsage) {
          col.insertBefore(btnGroup, standaloneUsage);
        } else {
          col.appendChild(btnGroup);
        }
      }
      btnGroup.replaceChildren();
      for (const surf of item.surfaceRefs) {
        const btn = document.createElement("button");
        btn.type = "button";
        btn.className = "button button--protected canvas-jump-btn";

        const icon = document.createElement("span");
        icon.className = "google-symbols filled canvas-jump-icon";
        icon.setAttribute("aria-hidden", "true");
        icon.textContent = surf.icon || "spark";

        const label = document.createElement("span");
        label.className = "button__label canvas-jump-label";
        const strong = document.createElement("strong");
        strong.textContent = surf.title;
        label.appendChild(strong);

        const cta = document.createElement("span");
        cta.className = "canvas-jump-cta";
        cta.textContent = surf.cta || "View on Canvas →";

        btn.append(icon, label, cta);
        btn.addEventListener("click", () => {
          if (typeof surf.onOpen === "function") {
            surf.onOpen();
          } else {
            focusSurfaceOnCanvas(surf.id);
          }
        });
        btnGroup.appendChild(btn);
      }
    }
  }

  // Per-turn token telemetry & cost printed cleanly below the message/pills
  updateUsageFooter(col, { isUser: item.isUser, usage: item.usage });
};

export const renderTranscript = () => {
  updateSessionUsageUi();
  // Decide whether to follow the bottom *before* mutating the DOM: only
  // stick when the user is already near the bottom (or the session changed),
  // so scrolling up to read an earlier message is not fought by streaming.
  const sessionKey = state.activeSession?.id || "";
  const isNewSessionView = els.chatTranscript._renderedSessionId !== sessionKey;
  els.chatTranscript._renderedSessionId = sessionKey;
  const nearBottom =
    els.chatTranscript.scrollHeight - els.chatTranscript.scrollTop - els.chatTranscript.clientHeight < 40;
  const stickToBottom = prefs.autoScroll && (nearBottom || isNewSessionView);
  const visibleItems = [];

  state.currentEvents.forEach((ev, idx) => {
    const parts = ev?.content?.parts || [];
    const hasToolOrUi = parts.some(
      (p) =>
        p &&
        ("functionCall" in p ||
          "function_call" in p ||
          "functionResponse" in p ||
          "function_response" in p)
    );
    const isUser = !hasToolOrUi && isUserEvent(ev);

    const textContent = parts
      .map((p) => (typeof p?.text === "string" ? p.text : ""))
      .filter(Boolean)
      .join("");

    const toolParts = [];
    const surfaceRefs = [];
    let uiIndex = 0;

    parts.forEach((part) => {
      if (!part || typeof part !== "object") return;
      const callName =
        part?.name || part?.functionCall?.name || part?.function_call?.name || "";
      if (callName && String(callName).startsWith("render_")) {
        return; // Skip raw `render_*` functionCall in chat
      }
      const uiResp = extractUiResponse(part);
      if (uiResp) {
        const surfaceId = `surface-${ev.id || idx}-${uiIndex++}`;
        const found =
          state.canvasSurfaces.find((s) => s.id === surfaceId) || overlayTriggersById.get(surfaceId);
        if (found) surfaceRefs.push(found);
        return;
      }
      if ("name" in part || "functionResponse" in part || "function_response" in part) {
        toolParts.push(part);
      }
    });

    const visibleTools = prefs.showTools ? toolParts : [];
    if (!textContent && visibleTools.length === 0 && surfaceRefs.length === 0) {
      return;
    }

    const authorLabel = isUser
      ? state.currentUser?.name || state.currentUser?.email || "You"
      : ev.author === APP_NAME
        ? "AdSpace Agent"
        : ev.author || "Agent";

    visibleItems.push({
      isUser,
      authorLabel,
      partial: !!ev.partial,
      textContent,
      visibleTools,
      surfaceRefs,
      usage: isUser ? null : cachedEventUsage(ev),
    });
  });

  if (!visibleItems.length) {
    els.chatTranscript.replaceChildren();
    const empty = document.createElement("div");
    empty.className = "transcript-empty-placeholder";
    style(empty, {
      margin: "auto",
      textAlign: "center",
      color: "var(--text-muted)",
      padding: "32px 18px",
      fontSize: "12.5px",
    });
    empty.textContent =
      "Ask a question on the left or pick a starter card on the right to begin.";
    els.chatTranscript.appendChild(empty);
    return;
  }

  els.chatTranscript
    .querySelectorAll(":scope > .transcript-empty-placeholder")
    .forEach((el) => el.remove());

  // On a session switch, drop the old rows instead of reusing them so the
  // per-message counters do not animate from the previous session's values.
  if (isNewSessionView) els.chatTranscript.replaceChildren();

  const existingRows = Array.from(els.chatTranscript.querySelectorAll(":scope > .message-row"));

  visibleItems.forEach((item, idx) => {
    const roleKey = item.isUser ? "user" : "agent";
    let row = existingRows[idx];

    if (!row || row.dataset.roleKey !== roleKey || row.dataset.authorLabel !== item.authorLabel) {
      const newRow = document.createElement("div");
      newRow.className = `message-row ${item.isUser ? "message-row-user" : "message-row-agent"}`;
      newRow.dataset.roleKey = roleKey;
      newRow.dataset.authorLabel = item.authorLabel;

      const avatar = createAvatarElement(item.isUser, item.authorLabel);
      const col = document.createElement("div");
      col.className = "message-col";
      newRow.append(avatar, col);

      if (row) {
        els.chatTranscript.replaceChild(newRow, row);
      } else {
        els.chatTranscript.appendChild(newRow);
      }
      row = newRow;
    }

    const col = row.querySelector(":scope > .message-col");
    updateMessageCol(col, item);
  });

  while (els.chatTranscript.children.length > visibleItems.length) {
    els.chatTranscript.lastElementChild.remove();
  }

  if (stickToBottom) {
    els.chatTranscript.scrollTop = els.chatTranscript.scrollHeight;
  }
};

export const canvasTabIdFor = (surfaceId) => `canvas-tab-${surfaceId}`;
export const canvasPanelIdFor = (surfaceId) => `canvas-panel-${surfaceId}`;

export const renderCanvas = () => {
  // Rebuilding the tablist destroys the focused tab; remember whether focus
  // was inside so it can be restored on the newly active tab afterwards.
  const hadTabFocus = els.canvasTabs.contains(document.activeElement);
  els.canvasSurfacesEl.replaceChildren();
  els.canvasTabs.replaceChildren();

  if (state.canvasSurfaces.length === 0) {
    els.canvasEmpty.classList.remove("hidden");
    els.canvasTabs.classList.add("hidden");
    return;
  }

  if (prefs.canvasMode === "stacked") {
    state.activeSurfaceId = "all";
  } else if (
    state.activeSurfaceId === "all" ||
    !state.canvasSurfaces.some((s) => s.id === state.activeSurfaceId)
  ) {
    state.activeSurfaceId = state.canvasSurfaces[state.canvasSurfaces.length - 1].id;
  }

  els.canvasEmpty.classList.add("hidden");
  els.canvasTabs.classList.remove("hidden");

  const makeTab = (id, label, isActive, controlsId) => {
    const tab = document.createElement("button");
    tab.type = "button";
    tab.id = canvasTabIdFor(id);
    tab.className = `canvas-tab ${isActive ? "active" : ""}`;
    tab.setAttribute("role", "tab");
    tab.setAttribute("aria-selected", String(isActive));
    tab.setAttribute("aria-controls", controlsId);
    // Roving tabindex: only the selected tab is in the Tab order.
    tab.tabIndex = isActive ? 0 : -1;
    tab.textContent = label;
    return tab;
  };

  const allActive = prefs.canvasMode === "stacked";
  const allBtn = makeTab(
    "all",
    `All Cards (${state.canvasSurfaces.length})`,
    allActive,
    els.canvasSurfacesEl.id || "canvas-surfaces"
  );
  allBtn.addEventListener("click", () => {
    state.activeSurfaceId = "all";
    prefs.canvasMode = "stacked";
    applyPreferences({ rerender: false });
    renderCanvas();
  });
  els.canvasTabs.appendChild(allBtn);

  state.canvasSurfaces.forEach((surf) => {
    const isActive = prefs.canvasMode === "focused" && state.activeSurfaceId === surf.id;
    const tab = makeTab(surf.id, surf.title, isActive, canvasPanelIdFor(surf.id));
    tab.addEventListener("click", () => {
      focusSurfaceOnCanvas(surf.id);
    });
    els.canvasTabs.appendChild(tab);
  });

  const visibleSurfaces =
    prefs.canvasMode === "focused" && state.activeSurfaceId !== "all"
      ? state.canvasSurfaces.filter((s) => s.id === state.activeSurfaceId)
      : state.canvasSurfaces;

  for (const surf of visibleSurfaces) {
    const group = document.createElement("div");
    group.setAttribute("data-surface-id", surf.id);
    group.id = canvasPanelIdFor(surf.id);
    group.setAttribute("role", "tabpanel");
    group.setAttribute("aria-labelledby", canvasTabIdFor(allActive ? "all" : surf.id));
    style(group, { display: "flex", flexDirection: "column", gap: "14px" });
    for (const card of surf.cards) {
      group.appendChild(card);
    }
    els.canvasSurfacesEl.appendChild(group);
  }

  if (hadTabFocus) {
    const selected = els.canvasTabs.querySelector('[role="tab"][aria-selected="true"]');
    if (selected) selected.focus();
  }
};

// Arrow / Home / End move between canvas tabs and activate the target
// (activation re-renders, which re-focuses the selected tab above).
els.canvasTabs.addEventListener("keydown", (e) => {
  const tabs = Array.from(els.canvasTabs.querySelectorAll('[role="tab"]'));
  if (!tabs.length) return;
  const idx = tabs.indexOf(document.activeElement);
  let next;
  if (e.key === "ArrowRight") next = idx < 0 ? 0 : (idx + 1) % tabs.length;
  else if (e.key === "ArrowLeft") next = idx < 0 ? tabs.length - 1 : (idx - 1 + tabs.length) % tabs.length;
  else if (e.key === "Home") next = 0;
  else if (e.key === "End") next = tabs.length - 1;
  else return;
  e.preventDefault();
  tabs[next].focus();
  tabs[next].click();
});

els.exportUiBtn.addEventListener("click", () => {
  if (!state.canvasSurfaces.length) return;
  const payload = state.canvasSurfaces.map((s) => s.raw);
  const blob = new Blob([JSON.stringify(payload, null, 2)], { type: "application/json" });
  const url = URL.createObjectURL(blob);
  const a = document.createElement("a");
  a.href = url;
  a.download = `adspace-ui-${state.activeSession?.id?.slice(0, 8) || "canvas"}.json`;
  a.click();
  // Revoke after the click has been dispatched; revoking synchronously can
  // cancel the download in some browsers.
  setTimeout(() => URL.revokeObjectURL(url), 0);
});

els.clearUiBtn.addEventListener("click", () => {
  state.canvasSurfaces = [];
  renderCanvas();
});
