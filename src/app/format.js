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

// Pure formatting helpers (numbers, cell values, variants, initials). No DOM, no app state.

export const toNumber = (val) => {
  if (typeof val === "number" && Number.isFinite(val)) return val;
  if (typeof val === "string") {
    const cleaned = val.replace(/[$,%\s]/g, "");
    const n = Number(cleaned);
    if (cleaned !== "" && Number.isFinite(n)) return n;
  }
  return NaN;
};

// Agent-controlled image sources are limited to web URLs (same-origin
// relative paths included) and inline images; anything else renders empty.
export const safeImageSrc = (src) => {
  const raw = String(src || "").trim();
  if (!raw) return "";
  if (/^data:image\//i.test(raw)) return raw;
  try {
    const u = new URL(raw, location.href);
    return u.protocol === "https:" || u.protocol === "http:" ? u.href : "";
  } catch {
    return "";
  }
};

export const formatCompact = (n) => {
  if (!Number.isFinite(n)) return String(n ?? "");
  const abs = Math.abs(n);
  if (abs >= 1e9) return `${(n / 1e9).toFixed(1).replace(/\.0$/, "")}B`;
  if (abs >= 1e6) return `${(n / 1e6).toFixed(1).replace(/\.0$/, "")}M`;
  if (abs >= 1e3) return `${(n / 1e3).toFixed(1).replace(/\.0$/, "")}K`;
  if (Number.isInteger(n)) return String(n);
  return n.toFixed(2).replace(/\.?0+$/, "");
};

export const formatCellValue = (val) => {
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

export const variantColors = (variant) => {
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

export const variantIconName = (variant) => {
  switch (String(variant || "info").toLowerCase()) {
    case "success":
    case "ok":
      return "check_circle";
    case "warning":
    case "warn":
      return "warning";
    case "error":
    case "danger":
    case "outage":
      return "error";
    default:
      return "info";
  }
};

export const initialsOf = (nameOrEmail) => {
  const str = String(nameOrEmail || "U").trim();
  const parts = str.split(/\s+/);
  if (parts.length >= 2) {
    return `${parts[0][0]}${parts[parts.length - 1][0]}`.toUpperCase();
  }
  return str.slice(0, 2).toUpperCase();
};

export const formatExactInt = (n) => Math.max(0, Math.round(Number(n) || 0)).toLocaleString("en-US");

export const formatUsdShort = (cost) => {
  const val = Math.max(0, Number(cost) || 0);
  if (val === 0) return "$0.00";
  if (val < 0.0001) return "<$0.0001";
  if (val < 10) return `~$${val.toFixed(4)}`;
  return `~$${val.toFixed(2)}`;
};

export const formatUsdDetail = (cost) => {
  const val = Math.max(0, Number(cost) || 0);
  if (val === 0) return "$0.0000";
  if (val < 0.0001) return `$${val.toFixed(6)}`;
  if (val < 1) return `$${val.toFixed(4)}`;
  return `$${val.toFixed(2)}`;
};
