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
// OpenTelemetry GenAI Token Usage (`adk_telemetry` / `gen_ai.usage.*`) & Cost
// ---------------------------------------------------------------------------

import { DEFAULT_MODELS, prefs, state } from "../state.js";
import { getModelPricing } from "./pricing.js";

export const estimateStreamedPartsTokens = (parts) => {
  let textChars = 0;
  let thinkingChars = 0;
  for (const p of Array.isArray(parts) ? parts : []) {
    if (!p || typeof p !== "object") continue;
    if (typeof p.thinking === "string") {
      thinkingChars += p.thinking.length;
    } else if (typeof p.text === "string") {
      textChars += p.text.length;
    }
    const fc = p.functionCall || p.function_call;
    if (fc && typeof fc === "object") {
      textChars += (fc.name || "").length + JSON.stringify(fc.args || {}).length;
    }
  }
  const thinkingTokens = Math.ceil(thinkingChars / 4);
  const textTokens = Math.ceil(textChars / 4);
  return {
    thinkingTokens,
    outputTokens: textTokens + thinkingTokens,
  };
};

export const isUserEvent = (ev) => ev?.content?.role === "user" || ev?.author === "user";

export const extractEventUsage = (ev) => {
  if (isUserEvent(ev)) return null;

  const u = ev?.usage_metadata || ev?.usageMetadata;
  const streamedEst = ev?.partial
    ? estimateStreamedPartsTokens(ev?.content?.parts)
    : { thinkingTokens: 0, outputTokens: 0 };

  if (!u || typeof u !== "object") {
    if (!ev?.partial || streamedEst.outputTokens === 0) return null;
    const model = prefs.model || DEFAULT_MODELS.model;
    const p = getModelPricing(model);
    const outputCost = (streamedEst.outputTokens / 1e6) * p.output;
    return {
      model,
      inputTokens: 0,
      uncachedInputTokens: 0,
      outputTokens: streamedEst.outputTokens,
      totalTokens: streamedEst.outputTokens,
      cacheReadTokens: 0,
      cacheCreationTokens: 0,
      thinkingTokens: streamedEst.thinkingTokens,
      audioInputTokens: 0,
      audioOutputTokens: 0,
      inputCost: 0,
      cacheReadCost: 0,
      audioCost: 0,
      outputCost,
      totalCost: outputCost,
    };
  }

  const prov = u.provider_usage && typeof u.provider_usage === "object" ? u.provider_usage : {};
  const model =
    prov["gen_ai.request.model"] || prov.model || prefs.model || DEFAULT_MODELS.model;

  const inputTokens = Math.max(
    0,
    Number(
      u.prompt_token_count ?? prov["gen_ai.usage.input_tokens"] ?? prov.promptTokenCount ?? 0
    ) || 0
  );
  const rawOutputTokens = Math.max(
    0,
    Number(
      u.candidates_token_count ??
        prov["gen_ai.usage.output_tokens"] ??
        prov.candidatesTokenCount ??
        0
    ) || 0
  );
  const outputTokens = ev?.partial
    ? Math.max(rawOutputTokens, streamedEst.outputTokens)
    : rawOutputTokens;
  const rawTotalTokens = Math.max(
    0,
    Number(
      u.total_token_count ??
        prov["gen_ai.usage.total_tokens"] ??
        prov.totalTokenCount ??
        inputTokens + outputTokens
    ) || 0
  );
  const totalTokens = Math.max(rawTotalTokens, inputTokens + outputTokens);
  const cacheReadTokens = Math.min(
    inputTokens,
    Math.max(
      0,
      Number(
        u.cache_read_input_token_count ??
          prov["gen_ai.usage.cache_read_tokens"] ??
          prov.cachedContentTokenCount ??
          0
      ) || 0
    )
  );
  const cacheCreationTokens = Math.max(
    0,
    Number(
      u.cache_creation_input_token_count ?? prov["gen_ai.usage.cache_creation_tokens"] ?? 0
    ) || 0
  );
  const rawThinkingTokens = Math.max(
    0,
    Number(
      u.thinking_token_count ??
        prov["gen_ai.usage.thinking_tokens"] ??
        prov.thoughtsTokenCount ??
        0
    ) || 0
  );
  const thinkingTokens = ev?.partial
    ? Math.max(rawThinkingTokens, streamedEst.thinkingTokens)
    : rawThinkingTokens;
  const remainingInput = Math.max(0, inputTokens - cacheReadTokens);
  const audioInputTokens = Math.min(
    remainingInput,
    Math.max(
      0,
      Number(u.audio_input_token_count ?? prov["gen_ai.usage.audio_input_tokens"] ?? 0) || 0
    )
  );
  const audioOutputTokens = Math.max(
    0,
    Number(u.audio_output_token_count ?? prov["gen_ai.usage.audio_output_tokens"] ?? 0) || 0
  );
  const uncachedInputTokens = Math.max(0, remainingInput - audioInputTokens);

  const p = getModelPricing(model);
  const isLong = inputTokens > 200000;
  const inputRate = isLong ? p.inputLong : p.input;
  const cachedRate = isLong ? p.cachedLong : p.cached;
  const outputRate = isLong ? p.outputLong : p.output;

  const inputCost = (uncachedInputTokens / 1e6) * inputRate;
  const cacheReadCost = (cacheReadTokens / 1e6) * cachedRate;
  const audioCost = (audioInputTokens / 1e6) * p.audioIn;
  const outputCost = (outputTokens / 1e6) * outputRate;
  const computedTotalCost = inputCost + cacheReadCost + audioCost + outputCost;
  const totalCost =
    computedTotalCost > 0 || inputTokens + outputTokens > 0
      ? computedTotalCost
      : typeof u.cost === "number" && Number.isFinite(u.cost)
        ? u.cost
        : 0;

  return {
    model,
    inputTokens,
    uncachedInputTokens,
    outputTokens,
    totalTokens,
    cacheReadTokens,
    cacheCreationTokens,
    thinkingTokens,
    audioInputTokens,
    audioOutputTokens,
    inputCost,
    cacheReadCost,
    audioCost,
    outputCost,
    totalCost,
  };
};

// Usage of a finished (non-partial) event only changes when the pricing
// inputs change, so cache it per event object and drop the whole cache when
// the model / tier / custom rates / server table move. Partial events are
// re-estimated every time (their parts keep growing).
let usageCache = new WeakMap();
let usageCacheSig = "";
let usageCacheServerPricing = null;

export const syncUsageCache = () => {
  const sig = `${prefs.model}|${prefs.pricingTier || "standard"}|${JSON.stringify(
    prefs.modelPricing || {}
  )}`;
  if (sig !== usageCacheSig || state.serverModelPricing !== usageCacheServerPricing) {
    usageCache = new WeakMap();
    usageCacheSig = sig;
    usageCacheServerPricing = state.serverModelPricing;
  }
};

export const cachedEventUsage = (ev) => {
  if (!ev || typeof ev !== "object" || ev.partial) return extractEventUsage(ev);
  if (usageCache.has(ev)) return usageCache.get(ev);
  const u = extractEventUsage(ev);
  usageCache.set(ev, u);
  return u;
};

export const computeSessionUsage = (events) => {
  syncUsageCache();
  // Dedupe by id in place (last occurrence wins) without reordering events.
  const list = Array.isArray(events) ? events : [];
  const lastIdxById = new Map();
  list.forEach((ev, idx) => {
    if (ev && typeof ev === "object" && ev.id) lastIdxById.set(ev.id, idx);
  });
  const allEvents = list.filter(
    (ev, idx) => ev && typeof ev === "object" && (!ev.id || lastIdxById.get(ev.id) === idx)
  );

  const summary = {
    llmCalls: 0,
    inputTokens: 0,
    outputTokens: 0,
    totalTokens: 0,
    cacheReadTokens: 0,
    cacheCreationTokens: 0,
    thinkingTokens: 0,
    audioInputTokens: 0,
    audioOutputTokens: 0,
    inputCost: 0,
    cacheReadCost: 0,
    audioCost: 0,
    outputCost: 0,
    totalCost: 0,
    models: new Set(),
  };

  for (const ev of allEvents) {
    const u = cachedEventUsage(ev);
    if (!u) continue;
    summary.llmCalls += 1;
    summary.inputTokens += u.inputTokens;
    summary.outputTokens += u.outputTokens;
    summary.totalTokens += u.totalTokens;
    summary.cacheReadTokens += u.cacheReadTokens;
    summary.cacheCreationTokens += u.cacheCreationTokens;
    summary.thinkingTokens += u.thinkingTokens;
    summary.audioInputTokens += u.audioInputTokens;
    summary.audioOutputTokens += u.audioOutputTokens;
    summary.inputCost += u.inputCost;
    summary.cacheReadCost += u.cacheReadCost;
    summary.audioCost += u.audioCost;
    summary.outputCost += u.outputCost;
    summary.totalCost += u.totalCost;
    if (u.model) summary.models.add(u.model);
  }

  return summary;
};
