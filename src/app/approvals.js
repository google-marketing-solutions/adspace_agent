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
// Write-API Tool Confirmations (`/api/approvals` & auto-resume)
// ---------------------------------------------------------------------------

import { APP_NAME, state } from "./state.js";
import { showError } from "./status.js";

const APPROVAL_STATES_QUERY = "pending,approved,rejected,responded,expired,cancelled";

let renderTranscriptHook = null;
let resumeSessionHook = null;

/**
 * Binds higher-level transcript and SSE resume handlers after module load
 * to avoid import cycles between `approvals.js`, `transcript.js`, and `sse.js`.
 */
export const bindApprovalHandlers = ({ renderTranscript, resumeSession }) => {
  renderTranscriptHook = renderTranscript;
  resumeSessionHook = resumeSession;
};

const notifyRender = () => {
  state.approvalsGeneration += 1;
  if (typeof renderTranscriptHook === "function") renderTranscriptHook();
};

export const extractConfirmationRequest = (ev) =>
  ev?.actions?.tool_confirmation || ev?.actions?.toolConfirmation || null;

export const isConfirmationPendingPart = (part) => {
  const resp = part?.functionResponse?.response || part?.function_response?.response;
  return Boolean(resp && typeof resp === "object" && resp.status === "confirmation_pending");
};

export const isBoilerplateConfirmationText = (text, toolName) => {
  if (typeof text !== "string") return false;
  const trimmed = text.trim();
  if (!trimmed) return false;
  const exact = `Tool confirmation required for '${toolName}'. Provide approve/deny decision to continue.`;
  return trimmed === exact;
};

export const stableJson = (value) => {
  if (value === null || typeof value !== "object") return JSON.stringify(value);
  if (Array.isArray(value)) return `[${value.map(stableJson).join(",")}]`;
  const keys = Object.keys(value).sort();
  return `{${keys.map((k) => `${JSON.stringify(k)}:${stableJson(value[k])}`).join(",")}}`;
};

const resolvedCallMap = (events) => {
  const resolved = new Map();
  events.forEach((ev, evIdx) => {
    const decision =
      ev?.actions?.tool_confirmation_decision || ev?.actions?.toolConfirmationDecision || null;
    const parts = Array.isArray(ev?.content?.parts) ? ev.content.parts : [];
    for (const part of parts) {
      const fr = part?.functionResponse || part?.function_response;
      if (!fr || isConfirmationPendingPart(part)) continue;
      const callId = part?.id || fr?.id;
      if (!callId) continue;
      const resp = fr.response;
      const derived =
        decision === "approve"
          ? "approved"
          : decision === "deny"
            ? "rejected"
            : resp && typeof resp === "object" && typeof resp.error === "string" && resp.error.includes("denied")
              ? "rejected"
              : "approved";
      resolved.set(callId, { evIdx, state: derived });
    }
  });
  return resolved;
};

/**
 * Matches each event's `tool_confirmation` payload against the session's
 * `/api/approvals` records and transcript outcomes so every confirmation card
 * knows whether it is pending, approved, rejected, expired, or cancelled.
 */
export const buildEventConfirmationMap = (events, approvals) => {
  const list = Array.isArray(events) ? events : [];
  const items = Array.isArray(approvals) ? approvals : [];
  const resolved = resolvedCallMap(list);
  const usedIds = new Set();
  const map = new Map();

  list.forEach((ev, evIdx) => {
    const req = extractConfirmationRequest(ev);
    if (!req) return;

    const toolName = req.toolName || req.tool_name || "tool";
    const callId = req.functionCallId || req.function_call_id || null;
    const reqArgs = req.args ?? {};
    const reqArgsJson = stableJson(reqArgs);

    const laterResolved = callId ? resolved.get(callId) : null;
    const isResolvedInTranscript = Boolean(laterResolved && laterResolved.evIdx > evIdx);

    let best = null;
    let bestScore = -1;
    for (const item of items) {
      if (!item || usedIds.has(item.id) || item.toolName !== toolName) continue;
      let score = 0;
      const itemPending = item.state === "pending";
      if (itemPending === !isResolvedInTranscript) score += 4;
      if (
        item.showsCallArgs &&
        item.argsRedacted !== undefined &&
        stableJson(item.argsRedacted) === reqArgsJson
      ) {
        score += 2;
      }
      if (score > bestScore) {
        best = item;
        bestScore = score;
      }
    }
    if (best) usedIds.add(best.id);

    let decisionState;
    if (isResolvedInTranscript) {
      decisionState =
        best && best.state && best.state !== "pending" ? best.state : laterResolved.state;
    } else if (best?.state) {
      decisionState = best.state;
    } else {
      decisionState = "pending";
    }

    map.set(evIdx, {
      eventId: ev.id || `ev-${evIdx}`,
      callId,
      toolName,
      args: best?.argsRedacted ?? reqArgs,
      summary: best?.summary || "",
      risk: best?.risk || "medium",
      approval: best,
      decisionState,
    });
  });

  return map;
};

export const clearSessionApprovals = () => {
  state.sessionApprovals = [];
  state.submittingApprovalIds.clear();
  state.approvalsGeneration += 1;
};

export const setSessionApprovals = (items) => {
  state.sessionApprovals = Array.isArray(items) ? items : [];
  notifyRender();
};

export const fetchSessionApprovals = async (sessionId) => {
  if (!sessionId) return [];
  try {
    const resp = await fetch(
      `/api/approvals?app=${encodeURIComponent(APP_NAME)}&session=${encodeURIComponent(
        sessionId
      )}&state=${APPROVAL_STATES_QUERY}`
    );
    if (!resp.ok) return null;
    const data = await resp.json();
    return Array.isArray(data?.items) ? data.items : [];
  } catch {
    return null;
  }
};

export const refreshSessionApprovals = async (sessionId) => {
  if (!sessionId || state.activeSession?.id !== sessionId) return null;
  const items = await fetchSessionApprovals(sessionId);
  if (items !== null && state.activeSession?.id === sessionId) {
    setSessionApprovals(items);
  }
  return items;
};

const readProblemMessage = async (resp) => {
  try {
    const data = await resp.json();
    return data?.detail || data?.error || data?.title || `HTTP ${resp.status}`;
  } catch {
    return `HTTP ${resp.status}`;
  }
};

/**
 * Records an `approve` or `reject` decision on `POST /api/approvals/{id}/decide`
 * and automatically resumes the paused run once no pending approvals remain in
 * the active session.
 */
export const decideApproval = async (approval, action, reason = "") => {
  const session = state.activeSession;
  if (!approval?.id || !session?.id || state.isRunning) return;
  if (state.submittingApprovalIds.has(approval.id)) return;

  state.submittingApprovalIds.add(approval.id);
  notifyRender();

  let shouldResume = false;
  try {
    const payload = {
      expectedVersion: approval.version,
      action,
    };
    if (reason) payload.reason = reason;

    const resp = await fetch(`/api/approvals/${encodeURIComponent(approval.id)}/decide`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify(payload),
    });

    if (!resp.ok) {
      const msg = await readProblemMessage(resp);
      showError(`Could not record decision: ${msg}`);
      if (resp.status === 409) {
        await refreshSessionApprovals(session.id);
      }
      return;
    }

    const updated = await resp.json();
    if (state.activeSession?.id !== session.id) return;

    const next = state.sessionApprovals.slice();
    const idx = next.findIndex((item) => item.id === updated.id);
    if (idx !== -1) next[idx] = updated;
    else next.push(updated);
    state.sessionApprovals = next;

    shouldResume = !state.sessionApprovals.some((item) => item.state === "pending");
  } catch (err) {
    showError(`Decision request failed: ${err.message || err}`);
  } finally {
    state.submittingApprovalIds.delete(approval.id);
    notifyRender();
  }

  if (shouldResume && typeof resumeSessionHook === "function" && state.activeSession?.id === session.id) {
    await resumeSessionHook(session);
  }
};
