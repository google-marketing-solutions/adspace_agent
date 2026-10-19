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
// Sending Messages & SSE Streaming (`POST /api/run_sse`)
// ---------------------------------------------------------------------------

import { $ } from "./dom.js";
import { APP_NAME, els, prefs, state } from "./state.js";
import { setStatus, showError } from "./status.js";
import {
  coalesceParts,
  mergeStreamedEvent,
  rebuildFromSessionEvents,
  renderTranscript,
} from "./transcript.js";
import { apiUrl, loadSession } from "./sessions.js";

export const renderAttachments = () => {
  els.attachmentChips.replaceChildren();
  if (!state.attachments.length) {
    els.attachmentChips.classList.add("hidden");
    return;
  }
  els.attachmentChips.classList.remove("hidden");
  state.attachments.forEach((file, idx) => {
    const chip = document.createElement("button");
    chip.type = "button";
    const nameSpan = document.createElement("span");
    nameSpan.textContent = file.name;
    const closeB = document.createElement("b");
    closeB.textContent = "×";
    chip.append(nameSpan, " ", closeB);
    chip.addEventListener("click", () => {
      state.attachments.splice(idx, 1);
      renderAttachments();
    });
    els.attachmentChips.appendChild(chip);
  });
};

els.fileInput.addEventListener("change", () => {
  const files = Array.from(els.fileInput.files || []);
  state.attachments.push(...files);
  els.fileInput.value = "";
  renderAttachments();
});

export const extractSseChunkData = (chunk) => {
  const dataLines = chunk
    .split("\n")
    .filter((l) => l.startsWith("data:"))
    .map((l) => l.slice(5).trimStart());
  return dataLines.length ? dataLines.join("\n") : null;
};

export const parseSseStream = async function* (body) {
  const reader = body.getReader();
  const decoder = new TextDecoder();
  let buffer = "";
  while (true) {
    const { value, done } = await reader.read();
    if (done) {
      buffer += decoder.decode();
      break;
    }
    // Normalise CRLF on the accumulated buffer (not just this chunk) so a
    // `\r\n` split across two TCP reads still forms an event boundary.
    buffer = (buffer + decoder.decode(value, { stream: true })).replace(/\r\n/g, "\n");
    let boundary;
    while ((boundary = buffer.indexOf("\n\n")) !== -1) {
      const chunk = buffer.slice(0, boundary);
      buffer = buffer.slice(boundary + 2);
      const data = extractSseChunkData(chunk);
      if (data) yield data;
    }
  }
  const trailing = extractSseChunkData(buffer.replace(/\r\n/g, "\n"));
  if (trailing) yield trailing;
};

// Aborts the in-flight run, if any. Only the send button toggles between
// send and stop; Enter, starter cards and adk-ui forms never stop a run.
export const stopRun = () => {
  if (state.isRunning && state.abortController) state.abortController.abort();
};

// SSE chunks arrive far faster than frames; coalesce the transcript/canvas
// rebuild to at most once per animation frame while streaming. The stream
// end (and error paths) flush or cancel the pending frame explicitly.
let streamRenderRaf = 0;
export const flushStreamRender = () => {
  if (streamRenderRaf) {
    cancelAnimationFrame(streamRenderRaf);
    streamRenderRaf = 0;
  }
  rebuildFromSessionEvents(state.currentEvents);
};
export const scheduleStreamRender = () => {
  if (streamRenderRaf) return;
  streamRenderRaf = requestAnimationFrame(() => {
    streamRenderRaf = 0;
    rebuildFromSessionEvents(state.currentEvents);
  });
};
export const cancelStreamRender = () => {
  if (streamRenderRaf) {
    cancelAnimationFrame(streamRenderRaf);
    streamRenderRaf = 0;
  }
};

// Polite screen-reader announcement via the visually hidden `#sr-announcer`
// live region (null-guarded: the element lives in studio.html). Clearing
// first makes a repeated identical message re-announce.
let announceTimer = null;
export const announce = (text) => {
  const el = $("sr-announcer");
  if (!el) return;
  if (announceTimer) clearTimeout(announceTimer);
  el.textContent = "";
  announceTimer = setTimeout(() => {
    announceTimer = null;
    el.textContent = text;
  }, 50);
};

export const sendMessage = async (overrideText) => {
  if (state.isRunning) return;
  if (!state.currentUser || !state.activeSession) return;

  const rawText =
    typeof overrideText === "string" ? overrideText.trim() : els.chatInput.value.trim();
  if (!rawText && state.attachments.length === 0) return;

  // Pin the session this run streams for. A session switch mid-stream must
  // not render this run's chunks into (or reload) the newly active session.
  const runSession = state.activeSession;
  const pendingFiles = state.attachments;

  if (typeof overrideText !== "string") {
    els.chatInput.value = "";
    syncComposerHeight();
  }
  state.attachments = [];
  renderAttachments();

  // Flip to the running state synchronously, before any `await`, so a second
  // Enter/click while attachments are still being encoded cannot start a
  // concurrent request.
  state.isRunning = true;
  setStatus("running", "streaming");
  els.sendBtn.classList.add("stop-button");
  if ($("send-icon")) {
    $("send-icon").textContent = "stop";
    $("send-icon").classList.add("filled");
  }
  state.abortController = new AbortController();
  let failed = false;

  try {
    const parts = [];
    if (rawText) parts.push({ text: rawText });

    for (const file of pendingFiles) {
      const bytes = new Uint8Array(await file.arrayBuffer());
      let binary = "";
      for (let i = 0; i < bytes.length; i += 32768) {
        binary += String.fromCharCode(...bytes.subarray(i, i + 32768));
      }
      parts.push({
        inlineData: {
          displayName: file.name,
          data: btoa(binary),
          mimeType: file.type || "application/octet-stream",
        },
      });
    }

    // Encoding large attachments can take a while; if the user switched
    // sessions (or hit stop) meanwhile, bail before the optimistic bubble
    // lands in the wrong session's transcript.
    if (state.abortController.signal.aborted || state.activeSession?.id !== runSession.id) {
      throw new DOMException("Run superseded before send", "AbortError");
    }

    // Optimistically append user message
    const userEvent = {
      id: `local-user-${Date.now()}`,
      author: "user",
      content: { role: "user", parts: [{ text: rawText || "(attachment)" }] },
    };
    state.currentEvents.push(userEvent);
    renderTranscript();
    // The user's own message always scrolls into view.
    if (prefs.autoScroll) els.chatTranscript.scrollTop = els.chatTranscript.scrollHeight;

    const resp = await fetch(apiUrl("/run_sse"), {
      method: "POST",
      signal: state.abortController.signal,
      headers: {
        "Content-Type": "application/json",
        "x-adk-ui-protocol": "a2ui",
      },
      body: JSON.stringify({
        appName: APP_NAME,
        userId: state.currentUser.id,
        sessionId: runSession.id,
        streaming: true,
        newMessage: { role: "user", parts },
      }),
    });

    if (!resp.ok) {
      const errText = await resp.text();
      throw new Error(errText || `HTTP ${resp.status}`);
    }

    if (resp.body) {
      for await (const dataStr of parseSseStream(resp.body)) {
        if (state.abortController.signal.aborted || state.activeSession?.id !== runSession.id) {
          // The user switched sessions; stop streaming into the wrong one.
          state.abortController.abort();
          break;
        }
        if (!dataStr || dataStr === "[DONE]") continue;
        // Only the parse is allowed to fail quietly (a frame may be split
        // mid-JSON); renderer exceptions must surface, not be swallowed.
        let parsed;
        try {
          parsed = JSON.parse(dataStr);
        } catch {
          continue;
        }
        const ev = parsed.event ?? parsed;
        if (ev && typeof ev === "object") {
          if (ev.error_message || ev.errorMessage) {
            showError(`Agent error: ${ev.error_message || ev.errorMessage}`);
          }
          const existingIdx = state.currentEvents.findIndex((e) => e.id && e.id === ev.id);
          if (existingIdx !== -1) {
            state.currentEvents[existingIdx] = mergeStreamedEvent(state.currentEvents[existingIdx], ev);
          } else {
            state.currentEvents.push({
              ...ev,
              content: ev.content
                ? { ...ev.content, parts: coalesceParts(ev.content.parts) }
                : ev.content,
            });
          }
          scheduleStreamRender();
        }
      }
      // Flush whatever the last frame did not get to before reloading.
      flushStreamRender();
    }
    // Reload only if nobody switched away meanwhile. `loadSession` aborts the
    // controller on an explicit switch before `activeSession` changes, so the
    // signal is checked too; otherwise this reload would supersede the
    // user's pending switch.
    if (!state.abortController.signal.aborted && state.activeSession?.id === runSession.id) {
      // Single polite announcement per completed turn (the transcript
      // itself is no longer a live region).
      announce("Agent replied");
      await loadSession(runSession.id);
    }
  } catch (err) {
    if (err.name !== "AbortError") {
      failed = true;
      showError(`Agent request failed: ${err.message || err}`);
    }
  } finally {
    // Paint whatever the last frame did not get to (e.g. after stopRun(),
    // which skips the post-loop flush); drop it if the user switched away.
    if (state.activeSession?.id === runSession.id) {
      flushStreamRender();
    } else {
      cancelStreamRender();
    }
    state.isRunning = false;
    state.abortController = null;
    els.sendBtn.classList.remove("stop-button");
    if ($("send-icon")) {
      $("send-icon").textContent = "arrow_upward";
      $("send-icon").classList.remove("filled");
    }
    // `failed` sticks until the next send flips the pill back to running.
    setStatus(failed ? "failed" : "ready", failed ? "failed" : "ready");
  }
};

export const syncComposerHeight = () => {
  els.chatInput.style.height = "36px";
  const next = Math.min(160, Math.max(36, els.chatInput.scrollHeight));
  els.chatInput.style.height = `${next}px`;
};

els.chatInput.addEventListener("input", syncComposerHeight);
els.sendBtn.addEventListener("click", () => (state.isRunning ? stopRun() : sendMessage()));
els.chatInput.addEventListener("keydown", (e) => {
  // Ignore the Enter that confirms an IME composition (CJK etc.); `keyCode
  // 229` is the legacy signal some browsers still send for it.
  if (e.isComposing || e.keyCode === 229) return;
  if (e.key === "Enter" && !e.shiftKey) {
    e.preventDefault();
    if (!state.isRunning) sendMessage();
  }
});

// Starter cards on empty canvas
document.querySelectorAll(".starter-card").forEach((card) => {
  card.addEventListener("click", () => {
    const prompt = card.getAttribute("data-prompt");
    if (prompt && !state.isRunning) sendMessage(prompt);
  });
});
