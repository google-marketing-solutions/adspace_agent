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

// Model pricing: server-served Paid Tier rates, the local fallback table, custom overrides and the pricing settings controls.

import { $ } from "../dom.js";
import { DEFAULT_MODELS, prefs, state } from "../state.js";

export const toFiniteRate = (v) => {
  const n = Number(v);
  return Number.isFinite(n) && n >= 0 ? n : null;
};

export const ingestServerPricing = (models) => {
  const table = models && typeof models === "object" ? models.pricing : null;
  if (!table || typeof table !== "object") return false;
  const next = {};
  for (const [name, p] of Object.entries(table)) {
    if (!p || typeof p !== "object") continue;
    const input = toFiniteRate(p.input_per_million);
    const cached = toFiniteRate(p.cached_input_per_million);
    const output = toFiniteRate(p.output_per_million);
    if (input === null || cached === null || output === null) continue;
    next[String(name).trim().toLowerCase()] = {
      input,
      inputLong: toFiniteRate(p.input_long_per_million) ?? input,
      cached,
      cachedLong: toFiniteRate(p.cached_input_long_per_million) ?? cached,
      output,
      outputLong: toFiniteRate(p.output_long_per_million) ?? output,
      audioIn: toFiniteRate(p.audio_input_per_million) ?? input,
    };
  }
  state.serverModelPricing = next;
  return true;
};

// Base rates for a model. Prefers the table served by the backend (see
// `ingestServerPricing`); the heuristics below are a fallback that mirrors
// `config::model_pricing` in src/config.rs rule-for-rule (keep both in sync).
// Official Paid Tier pricing per 1M tokens (USD) from https://ai.google.dev/gemini-api/docs/pricing
export const getFallbackModelPricing = (m) => {
  if (m.includes("gemini-4") || m.includes("argon")) {
    // Gemini 4 Argon: $2.00 in, $0.10 cached, $10.00 out
    return {
      input: 2.0,
      inputLong: 2.0,
      cached: 0.1,
      cachedLong: 0.1,
      output: 10.0,
      outputLong: 10.0,
      audioIn: 2.0,
    };
  }
  if (m.includes("gemini-3.5-flash-lite")) {
    // Gemini 3.5 Flash-Lite: $0.30 in, $0.03 cached, $2.50 out
    return {
      input: 0.3,
      inputLong: 0.3,
      cached: 0.03,
      cachedLong: 0.03,
      output: 2.5,
      outputLong: 2.5,
      audioIn: 0.3,
    };
  }
  if (m.includes("gemini-3.1-flash-lite")) {
    // Gemini 3.1 Flash-Lite: $0.25 in ($0.50 audio), $0.025 cached, $1.50 out
    return {
      input: 0.25,
      inputLong: 0.25,
      cached: 0.025,
      cachedLong: 0.025,
      output: 1.5,
      outputLong: 1.5,
      audioIn: 0.5,
    };
  }
  if (m.includes("flash-lite") || m.includes("gemini-2.0-flash")) {
    // Gemini 2.5 Flash-Lite / 2.0 Flash: $0.10 in ($0.30 audio), $0.01 cached, $0.40 out
    return {
      input: 0.1,
      inputLong: 0.1,
      cached: 0.01,
      cachedLong: 0.01,
      output: 0.4,
      outputLong: 0.4,
      audioIn: 0.3,
    };
  }
  if ((m.includes("gemini-3.8") || m.includes("gemini-3.6")) && m.includes("flash")) {
    // Gemini 3.8 Flash & 3.6 Flash: $0.75 in, $0.075 cached, $3.75 out
    return {
      input: 0.75,
      inputLong: 0.75,
      cached: 0.075,
      cachedLong: 0.075,
      output: 3.75,
      outputLong: 3.75,
      audioIn: 0.75,
    };
  }
  if (m.includes("gemini-3") && m.includes("flash")) {
    // Gemini 3 Flash Preview: $0.50 in ($1.00 audio), $0.05 cached, $3.00 out
    return {
      input: 0.5,
      inputLong: 0.5,
      cached: 0.05,
      cachedLong: 0.05,
      output: 3.0,
      outputLong: 3.0,
      audioIn: 1.0,
    };
  }
  if (m.includes("flash")) {
    // Gemini 2.5 Flash: $0.30 in ($1.00 audio), $0.03 cached, $2.50 out
    return {
      input: 0.3,
      inputLong: 0.3,
      cached: 0.03,
      cachedLong: 0.03,
      output: 2.5,
      outputLong: 2.5,
      audioIn: 1.0,
    };
  }
  if (m.includes("gemini-3")) {
    // Gemini 3.1 Pro Preview: $2.00/$4.00 in, $0.20/$0.40 cached, $12.00/$18.00 out
    return {
      input: 2.0,
      inputLong: 4.0,
      cached: 0.2,
      cachedLong: 0.4,
      output: 12.0,
      outputLong: 18.0,
      audioIn: 2.0,
    };
  }
  // Gemini 2.5 Pro: $1.25/$2.50 in, $0.125/$0.25 cached, $10.00/$15.00 out
  return {
    input: 1.25,
    inputLong: 2.5,
    cached: 0.125,
    cachedLong: 0.25,
    output: 10.0,
    outputLong: 15.0,
    audioIn: 1.25,
  };
};

export const getDefaultModelPricing = (model, tier = prefs.pricingTier) => {
  const m = String(model || prefs.model || DEFAULT_MODELS.model)
    .trim()
    .toLowerCase();
  const base = state.serverModelPricing[m] || getFallbackModelPricing(m);

  const mult = tier === "priority" ? 1.8 : tier === "batch" ? 0.5 : 1.0;
  const scale = (v) => Number((v * mult).toFixed(6));
  return {
    input: scale(base.input),
    inputLong: scale(base.inputLong),
    cached: scale(base.cached),
    cachedLong: scale(base.cachedLong),
    output: scale(base.output),
    outputLong: scale(base.outputLong),
    audioIn: scale(base.audioIn),
  };
};

export const getModelPricingKey = (model, tier = prefs.pricingTier) => {
  const m = String(model || prefs.model || DEFAULT_MODELS.model)
    .trim()
    .toLowerCase();
  const t = String(tier || "standard").toLowerCase();
  return `${t}:${m}`;
};

export const getModelPricing = (model) => {
  const tier = prefs.pricingTier || "standard";
  const defaults = getDefaultModelPricing(model, tier);
  const key = getModelPricingKey(model, tier);
  const custom =
    prefs.modelPricing && typeof prefs.modelPricing === "object"
      ? prefs.modelPricing[key]
      : null;
  if (!custom || typeof custom !== "object") {
    return { ...defaults, isCustom: false, tier };
  }

  const input =
    typeof custom.input === "number" && Number.isFinite(custom.input) && custom.input >= 0
      ? custom.input
      : defaults.input;
  const cached =
    typeof custom.cached === "number" && Number.isFinite(custom.cached) && custom.cached >= 0
      ? custom.cached
      : defaults.cached;
  const output =
    typeof custom.output === "number" && Number.isFinite(custom.output) && custom.output >= 0
      ? custom.output
      : defaults.output;

  const inLongRatio = defaults.input > 0 ? defaults.inputLong / defaults.input : 1;
  const cachedLongRatio = defaults.cached > 0 ? defaults.cachedLong / defaults.cached : 1;
  const outLongRatio = defaults.output > 0 ? defaults.outputLong / defaults.output : 1;
  const audioRatio = defaults.input > 0 ? defaults.audioIn / defaults.input : 1;

  return {
    input,
    inputLong: Number((input * inLongRatio).toFixed(6)),
    cached,
    cachedLong: Number((cached * cachedLongRatio).toFixed(6)),
    output,
    outputLong: Number((output * outLongRatio).toFixed(6)),
    audioIn: Number((input * audioRatio).toFixed(6)),
    isCustom: true,
    tier,
  };
};

export const syncPricingControls = () => {
  const activeModel = prefs.model || DEFAULT_MODELS.model;
  const rates = getModelPricing(activeModel);
  if ($("setting-pricing-model-label")) {
    $("setting-pricing-model-label").textContent = activeModel;
  }
  if ($("setting-pricing-tier")) {
    $("setting-pricing-tier").value = prefs.pricingTier || "standard";
  }
  const inEl = $("setting-price-input");
  if (inEl && document.activeElement !== inEl) {
    inEl.value = String(rates.input);
  }
  const cachedEl = $("setting-price-cached");
  if (cachedEl && document.activeElement !== cachedEl) {
    cachedEl.value = String(rates.cached);
  }
  const outEl = $("setting-price-output");
  if (outEl && document.activeElement !== outEl) {
    outEl.value = String(rates.output);
  }
};
