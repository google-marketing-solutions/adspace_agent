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

// Session usage popover / pill (token totals, cost breakdown, animated counters).

import { $ } from "../dom.js";
import { formatExactInt, formatUsdDetail, formatUsdShort } from "../format.js";
import { DEFAULT_MODELS, els, prefs, state } from "../state.js";
import { animateCounter, triggerPulseClass } from "./counter.js";
import { getModelPricing } from "./pricing.js";
import { computeSessionUsage } from "./telemetry.js";

export const updateSessionUsageUi = () => {
  const s = computeSessionUsage(state.currentEvents);
  const activeModel = prefs.model || DEFAULT_MODELS.model;
  const modelsList = s.models.size > 0 ? Array.from(s.models) : [activeModel];
  const primaryModel = modelsList[modelsList.length - 1] || activeModel;
  const rates = getModelPricing(primaryModel);
  const showSession = !!prefs.showSessionUsage;
  const showCost = !!prefs.showCost;

  if ($("session-usage-anchor")) {
    $("session-usage-anchor").classList.toggle("hidden", !showSession);
    if (!showSession && els.sessionUsagePopover) {
      els.sessionUsagePopover.classList.add("hidden");
    }
  }

  if (els.sessionUsageBtn) {
    const prevBtnTokens = els.sessionUsageBtn._lastSessionTokens;
    if (
      typeof prevBtnTokens === "number" &&
      s.totalTokens > prevBtnTokens &&
      showSession
    ) {
      triggerPulseClass(els.sessionUsageBtn, "usage-pill-bump");
    }
    els.sessionUsageBtn._lastSessionTokens = s.totalTokens;
  }

  animateCounter(
    $("session-usage-tokens"),
    s.totalTokens,
    (v) => `${formatExactInt(v)} tok`
  );
  if ($("session-usage-sep")) {
    $("session-usage-sep").classList.toggle("hidden", !showCost);
  }
  if ($("session-usage-cost")) {
    $("session-usage-cost").classList.toggle("hidden", !showCost);
    animateCounter($("session-usage-cost"), s.totalCost, formatUsdShort);
  }
  if (els.sessionUsageBtn) {
    const costSuffix = showCost ? ` · Est. ${formatUsdDetail(s.totalCost)}` : "";
    els.sessionUsageBtn.title = `Session usage: ${formatExactInt(s.totalTokens)} tokens (${formatExactInt(
      s.inputTokens
    )} input, ${formatExactInt(s.outputTokens)} output)${costSuffix} across ${s.llmCalls} LLM call${
      s.llmCalls === 1 ? "" : "s"
    }`;
  }

  animateCounter($("usage-total-tokens"), s.totalTokens, formatExactInt);
  animateCounter(
    $("usage-llm-calls"),
    s.llmCalls,
    (v) => {
      const n = Math.round(v);
      return `${n} LLM call${n === 1 ? "" : "s"}`;
    }
  );
  if ($("usage-kpi-cost-card")) {
    $("usage-kpi-cost-card").classList.toggle("hidden", !showCost);
  }
  animateCounter($("usage-total-cost"), s.totalCost, formatUsdDetail);
  if ($("usage-active-model")) {
    $("usage-active-model").textContent = modelsList.join(", ");
  }

  animateCounter($("usage-input-tokens"), s.inputTokens, formatExactInt);
  if ($("usage-input-cost")) {
    $("usage-input-cost").classList.toggle("hidden", !showCost);
    animateCounter($("usage-input-cost"), s.inputCost, formatUsdDetail);
  }
  animateCounter($("usage-cache-read-tokens"), s.cacheReadTokens, formatExactInt);
  if ($("usage-cache-read-cost")) {
    $("usage-cache-read-cost").classList.toggle("hidden", !showCost);
    animateCounter($("usage-cache-read-cost"), s.cacheReadCost, formatUsdDetail);
  }

  const cacheCreateRow = $("usage-row-cache-creation");
  if (cacheCreateRow) {
    cacheCreateRow.classList.toggle("hidden", s.cacheCreationTokens === 0);
    animateCounter(
      $("usage-cache-creation-tokens"),
      s.cacheCreationTokens,
      formatExactInt
    );
  }

  animateCounter($("usage-output-tokens"), s.outputTokens, formatExactInt);
  if ($("usage-output-cost")) {
    $("usage-output-cost").classList.toggle("hidden", !showCost);
    animateCounter($("usage-output-cost"), s.outputCost, formatUsdDetail);
  }
  animateCounter($("usage-thinking-tokens"), s.thinkingTokens, formatExactInt);

  const audioRow = $("usage-row-audio");
  if (audioRow) {
    const hasAudio = s.audioInputTokens > 0 || s.audioOutputTokens > 0;
    audioRow.classList.toggle("hidden", !hasAudio);
    const totalAudio = s.audioInputTokens + s.audioOutputTokens;
    animateCounter(
      $("usage-audio-tokens"),
      totalAudio,
      (v) => {
        const r = totalAudio > 0 ? v / totalAudio : 1;
        return `${formatExactInt(s.audioInputTokens * r)} / ${formatExactInt(
          s.audioOutputTokens * r
        )}`;
      },
      { formatSig: `${s.audioInputTokens}:${s.audioOutputTokens}` }
    );
    if ($("usage-audio-cost")) {
      $("usage-audio-cost").classList.toggle("hidden", !showCost);
      animateCounter($("usage-audio-cost"), s.audioCost, formatUsdDetail);
    }
  }

  if ($("usage-rate-note")) {
    const tierLabel = rates.isCustom
      ? "Custom"
      : rates.tier === "priority"
        ? "Paid Priority"
        : rates.tier === "batch"
          ? "Paid Batch/Flex"
          : "Paid Standard";
    $("usage-rate-note").textContent = showCost
      ? `${tierLabel} (${primaryModel}): $${rates.input}/1M in · $${rates.cached}/1M cached · $${rates.output}/1M out`
      : `Model (${primaryModel}) · OpenTelemetry gen_ai.usage.* counters`;
  }

  if ($("menu-session-tokens-row")) {
    $("menu-session-tokens-row").classList.toggle("hidden", !showSession);
  }
  animateCounter(
    $("menu-session-tokens"),
    s.totalTokens,
    (v) => `${formatExactInt(v)} tok`
  );
  if ($("menu-session-cost-row")) {
    $("menu-session-cost-row").classList.toggle("hidden", !showSession || !showCost);
  }
  animateCounter($("menu-session-cost"), s.totalCost, formatUsdDetail);
};
