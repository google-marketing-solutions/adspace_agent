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

// `adk-ui` chart component renderer (inline SVG). Pure DOM, no app state.

import { style, svgEl } from "../dom.js";
import { formatCellValue, formatCompact, toNumber } from "../format.js";

export const PALETTE = [
  "#4b90ff",
  "#6b7cff",
  "#9b72cb",
  "#d96570",
  "#38bdf8",
  "#34d399",
  "#fbbf24",
  "#f472b6",
];

// Points outside their range on a `band` chart.
const OUTSIDE_COLOR = "var(--accent-error, #d93025)";
// How many out-of-range points are listed under a `band` chart.
const MAX_LISTED_OUTSIDE = 10;

// The smallest "round" step (1, 2, 2.5, 5 or 10 times a power of ten) that is
// at least `x`.
const niceStep = (x) => {
  if (!(x > 0)) return 1;
  const pow = 10 ** Math.floor(Math.log10(x));
  const f = x / pow;
  return (f <= 1 ? 1 : f <= 2 ? 2 : f <= 2.5 ? 2.5 : f <= 5 ? 5 : 10) * pow;
};

// A date or timestamp as epoch milliseconds, or NaN. A date without a time
// ("2026-11-03") is read as local midnight: `Date` reads it as UTC midnight,
// which prints as the day before anywhere west of Greenwich.
const toTime = (value) => {
  if (typeof value === "number") return value;
  const text = String(value ?? "").trim();
  const day = /^(\d{4})-(\d{2})-(\d{2})$/.exec(text);
  if (day) return new Date(Number(day[1]), Number(day[2]) - 1, Number(day[3])).getTime();
  return new Date(text).getTime();
};

// Writes the x values of a `time` axis. `format` is the component's
// `time_format` (`date`, `time` or `iso`); without one, values that all fall
// on midnight are written as dates, values within one day as times, and
// anything else as date and time.
const timeFormatter = (times, format) => {
  const dates = times.filter(Number.isFinite).map((t) => new Date(t));
  const wholeDays = dates.every(
    (d) => d.getHours() === 0 && d.getMinutes() === 0 && d.getSeconds() === 0
  );
  const oneYear = new Set(dates.map((d) => d.getFullYear())).size <= 1;
  const oneDay = new Set(dates.map((d) => d.toDateString())).size <= 1;
  const mode = ["date", "time", "iso"].includes(format)
    ? format
    : wholeDays
      ? "date"
      : oneDay
        ? "time"
        : "both";
  const day = { month: "short", day: "numeric", ...(oneYear ? {} : { year: "numeric" }) };
  const clock = { hour: "numeric", minute: "2-digit" };
  const two = (v) => String(v).padStart(2, "0");
  return (t) => {
    const d = new Date(t);
    if (mode === "iso") {
      return wholeDays
        ? `${d.getFullYear()}-${two(d.getMonth() + 1)}-${two(d.getDate())}`
        : d.toISOString();
    }
    if (mode === "time") return d.toLocaleTimeString(undefined, clock);
    if (mode === "date") return d.toLocaleDateString(undefined, day);
    return d.toLocaleString(undefined, { ...day, ...clock });
  };
};

// An axis label, cut to `max` characters with the full text as its tooltip.
const axisLabel = (text, max, attrs) => {
  const label = svgEl("text", {
    "text-anchor": "middle",
    fill: "var(--dm-text-secondary)",
    "font-size": "11",
    ...attrs,
  });
  label.textContent = text.length > max ? `${text.slice(0, max - 1)}…` : text;
  const full = svgEl("title");
  full.textContent = text;
  label.appendChild(full);
  return label;
};

export const renderChartInner = (comp) => {
  const wrap = document.createElement("div");
  style(wrap, { display: "flex", flexDirection: "column", gap: "10px" });

  const rows = Array.isArray(comp.data) ? comp.data : [];
  // `window`: draw only the last N rows.
  const windowSize = Number(comp.window);
  const data = Number.isInteger(windowSize) && windowSize > 0 ? rows.slice(-windowSize) : rows;
  const xKey = comp.x_key || (data[0] ? Object.keys(data[0])[0] : "x");
  const yKeys =
    Array.isArray(comp.y_keys) && comp.y_keys.length
      ? comp.y_keys
      : data[0]
        ? Object.keys(data[0]).filter((k) => k !== xKey)
        : [];
  const colors = Array.isArray(comp.colors) && comp.colors.length ? comp.colors : PALETTE;
  const kind = String(comp.kind || "bar").toLowerCase();
  // Accessible name for the `<svg>` (the visible title lives in the card header).
  const chartLabel = comp.title ? String(comp.title) : `${kind} chart`;

  if (!data.length || !yKeys.length) {
    const empty = document.createElement("div");
    empty.textContent = "No chart data available.";
    style(empty, { color: "var(--text-muted)", fontSize: "12px", padding: "8px 0" });
    wrap.appendChild(empty);
    return wrap;
  }

  if (kind === "pie") {
    const yKey = yKeys[0];
    const slices = data
      .map((row, idx) => ({
        label: String(row[xKey] ?? `Item ${idx + 1}`),
        value: Math.max(0, toNumber(row[yKey]) || 0),
        color: colors[idx % colors.length],
      }))
      .filter((s) => s.value > 0);
    const total = slices.reduce((acc, s) => acc + s.value, 0) || 1;

    const rowWrap = document.createElement("div");
    style(rowWrap, {
      display: "flex",
      flexWrap: "wrap",
      alignItems: "center",
      gap: "24px",
      padding: "4px 0",
    });

    const svg = svgEl("svg", { viewBox: "0 0 210 210", role: "img", "aria-label": chartLabel });
    style(svg, {
      width: "185px",
      height: "185px",
      flexShrink: "0",
      fill: "none",
      stroke: "none",
    });

    const cx = 105;
    const cy = 105;
    const rOuter = 88;
    const rInner = 52;
    let angle = -Math.PI / 2;

    for (const s of slices) {
      const frac = s.value / total;
      const sweep = frac * Math.PI * 2;
      if (frac >= 0.9999) {
        const c = svgEl("circle", {
          cx,
          cy,
          r: (rOuter + rInner) / 2,
          fill: "none",
          stroke: s.color,
          "stroke-width": rOuter - rInner,
        });
        const t = svgEl("title");
        t.textContent = `${s.label}: ${formatCellValue(s.value)} (100%)`;
        c.appendChild(t);
        svg.appendChild(c);
      } else {
        const end = angle + sweep;
        const large = sweep > Math.PI ? 1 : 0;
        const x1 = cx + rOuter * Math.cos(angle);
        const y1 = cy + rOuter * Math.sin(angle);
        const x2 = cx + rOuter * Math.cos(end);
        const y2 = cy + rOuter * Math.sin(end);
        const x3 = cx + rInner * Math.cos(end);
        const y3 = cy + rInner * Math.sin(end);
        const x4 = cx + rInner * Math.cos(angle);
        const y4 = cy + rInner * Math.sin(angle);
        const d = [
          `M ${x1} ${y1}`,
          `A ${rOuter} ${rOuter} 0 ${large} 1 ${x2} ${y2}`,
          `L ${x3} ${y3}`,
          `A ${rInner} ${rInner} 0 ${large} 0 ${x4} ${y4}`,
          "Z",
        ].join(" ");
        const path = svgEl("path", {
          d,
          fill: s.color,
          stroke: "var(--surface-card)",
          "stroke-width": "2",
        });
        const t = svgEl("title");
        t.textContent = `${s.label}: ${formatCellValue(s.value)} (${(frac * 100).toFixed(1)}%)`;
        path.appendChild(t);
        svg.appendChild(path);
        angle = end;
      }
    }

    const centerTop = svgEl("text", {
      x: cx,
      y: cy - 4,
      "text-anchor": "middle",
      fill: "var(--dm-text-secondary)",
      "font-size": "10.5",
    });
    centerTop.textContent = yKey;
    const centerVal = svgEl("text", {
      x: cx,
      y: cy + 14,
      "text-anchor": "middle",
      fill: "var(--dm-text-primary)",
      "font-size": "16",
      "font-weight": "500",
    });
    centerVal.textContent = formatCompact(total);
    svg.append(centerTop, centerVal);

    const legend = document.createElement("div");
    style(legend, {
      display: "flex",
      flexDirection: "column",
      gap: "8px",
      flex: "1",
      minWidth: "180px",
    });
    for (const s of slices) {
      const item = document.createElement("div");
      style(item, {
        display: "flex",
        alignItems: "center",
        justifyContent: "space-between",
        gap: "10px",
        fontSize: "13px",
      });
      const left = document.createElement("div");
      style(left, { display: "flex", alignItems: "center", gap: "8px", minWidth: "0" });
      const swatch = document.createElement("span");
      style(swatch, {
        width: "9px",
        height: "9px",
        borderRadius: "999px",
        background: s.color,
        flexShrink: "0",
      });
      const lbl = document.createElement("span");
      lbl.textContent = s.label;
      style(lbl, {
        color: "var(--dm-text-primary)",
        overflow: "hidden",
        textOverflow: "ellipsis",
        whiteSpace: "nowrap",
      });
      left.append(swatch, lbl);

      const right = document.createElement("span");
      right.textContent = `${formatCompact(s.value)} (${((s.value / total) * 100).toFixed(1)}%)`;
      style(right, {
        color: "var(--dm-text-secondary)",
        fontFamily: "var(--mono)",
        fontSize: "12px",
        flexShrink: "0",
      });
      item.append(left, right);
      legend.appendChild(item);
    }

    rowWrap.append(svg, legend);
    wrap.appendChild(rowWrap);
    return wrap;
  }

  const isSpark = kind === "sparkline";
  const xAxisLabel = !isSpark && comp.x_label ? String(comp.x_label).trim() : "";
  const yAxisLabel = !isSpark && comp.y_label ? String(comp.y_label).trim() : "";
  const W = 680;
  const H = isSpark ? 84 : 250;
  // A y-axis label takes a strip to the left of the tick values.
  const padL = isSpark ? 8 : yAxisLabel ? 70 : 52;
  const padR = isSpark ? 8 : 16;
  const padT = isSpark ? 8 : 16;
  const padB = isSpark ? 10 : 44;
  const plotW = W - padL - padR;
  const plotH = H - padT - padB;

  // `band`: one series (the first y key) against a lower and an upper bound
  // per row (`lower_key` / `upper_key`), with an optional note (`note_key`)
  // shown on the points that fall outside their range. Nothing here knows
  // what the range means; it can be an expected range, a forecast interval,
  // a pacing corridor or a benchmark.
  const isBand = kind === "band";
  const seriesKeys = isBand ? yKeys.slice(0, 1) : yKeys;
  // The row's bounds in order, or null when either is missing.
  const boundsOf = (row) => {
    const a = toNumber(row?.[comp.lower_key]);
    const b = toNumber(row?.[comp.upper_key]);
    if (!Number.isFinite(a) || !Number.isFinite(b)) return null;
    return a <= b ? { lo: a, hi: b } : { lo: b, hi: a };
  };

  // Horizontal grid intervals. Fixed for the other kinds; a band chart picks
  // its own so the lines land on round values.
  let gridSteps = 4;
  let minVal = isBand ? Infinity : 0;
  let maxVal = isBand ? -Infinity : 0;
  for (const row of data) {
    for (const yk of seriesKeys) {
      const v = toNumber(row[yk]);
      if (Number.isFinite(v)) {
        if (v < minVal) minVal = v;
        if (v > maxVal) maxVal = v;
      }
    }
    const bounds = isBand ? boundsOf(row) : null;
    if (bounds) {
      if (bounds.lo < minVal) minVal = bounds.lo;
      if (bounds.hi > maxVal) maxVal = bounds.hi;
    }
  }
  if (isBand) {
    // Fit the axis to the series and its range instead of anchoring it at
    // zero: what matters is where the values sit against the band.
    if (!Number.isFinite(minVal)) {
      minVal = 0;
      maxVal = 1;
    }
    const pad = (maxVal - minVal) * 0.08 || Math.abs(maxVal) * 0.08 || 1;
    const lo = minVal >= 0 ? Math.max(0, minVal - pad) : minVal - pad;
    const hi = maxVal + pad;
    // Snap the axis to round steps, aiming for about five intervals.
    let step = niceStep((hi - lo) / 5);
    while (Math.ceil(hi / step) - Math.floor(lo / step) > 6) step = niceStep(step * 1.01);
    minVal = Math.floor(lo / step) * step;
    maxVal = Math.ceil(hi / step) * step;
    gridSteps = Math.max(1, Math.round((maxVal - minVal) / step));
  }
  if (maxVal === minVal) maxVal = minVal + 1;
  const span = maxVal - minVal;
  const yToPx = (v) => padT + plotH - ((v - minVal) / span) * plotH;

  const svg = svgEl("svg", { viewBox: `0 0 ${W} ${H}`, role: "img", "aria-label": chartLabel });
  style(svg, {
    width: "100%",
    height: "auto",
    maxHeight: isSpark ? "90px" : "265px",
    overflow: "visible",
    fill: "none",
    stroke: "none",
  });

  if (!isSpark) {
    const steps = gridSteps;
    for (let i = 0; i <= steps; i++) {
      const val = minVal + (span * i) / steps;
      const y = yToPx(val);
      const line = svgEl("line", {
        x1: padL,
        y1: y,
        x2: W - padR,
        y2: y,
        stroke: "var(--dm-outline)",
        "stroke-width": "1",
      });
      const label = svgEl("text", {
        x: padL - 8,
        y: y + 3.5,
        "text-anchor": "end",
        fill: "var(--dm-text-secondary)",
        "font-size": "10.5",
      });
      label.textContent = formatCompact(val);
      svg.append(line, label);
    }
  }

  if (yAxisLabel) {
    const cy = padT + plotH / 2;
    svg.appendChild(axisLabel(yAxisLabel, 34, { x: 12, y: cy, transform: `rotate(-90 12 ${cy})` }));
  }
  if (xAxisLabel) {
    svg.appendChild(axisLabel(xAxisLabel, 90, { x: padL + plotW / 2, y: H - 5 }));
  }

  const n = data.length;
  // `x_type`: `time` and `numeric` x values are read as such; the default is
  // a category, shown as given.
  const xType = String(comp.x_type || "category").toLowerCase();
  const xValues =
    xType === "time"
      ? data.map((row) => toTime(row[xKey]))
      : xType === "numeric"
        ? data.map((row) => toNumber(row[xKey]))
        : null;
  const formatTime = xType === "time" ? timeFormatter(xValues, comp.time_format) : null;
  // The text of an x tick: dates and times in the axis format, the rest as given.
  const xText = (row, i) =>
    formatTime && Number.isFinite(xValues[i]) ? formatTime(xValues[i]) : String(row[xKey] ?? "");
  // Points of a `band` chart that fall outside their range, in row order.
  const outside = [];
  if (kind === "bar") {
    const groupStep = plotW / Math.max(1, n);
    const groupW = groupStep * 0.72;
    const barW = Math.max(4, groupW / Math.max(1, yKeys.length) - 2);
    const zeroY = yToPx(Math.max(0, minVal));
    // Show at most ~10 x-axis labels regardless of row count.
    const stepEvery = Math.max(1, Math.ceil(n / 10));

    data.forEach((row, i) => {
      const gx = padL + i * groupStep + (groupStep - groupW) / 2;
      yKeys.forEach((yk, sIdx) => {
        const raw = row[yk];
        const v = toNumber(raw);
        if (!Number.isFinite(v)) return;
        const yVal = yToPx(v);
        const top = Math.min(yVal, zeroY);
        const h = Math.max(2, Math.abs(zeroY - yVal));
        const bx = gx + sIdx * (barW + 2);
        const rect = svgEl("rect", {
          x: bx,
          y: top,
          width: barW,
          height: h,
          rx: "6",
          fill: colors[sIdx % colors.length],
          stroke: "none",
        });
        const title = svgEl("title");
        title.textContent = `${row[xKey] ?? i} · ${yk}: ${formatCellValue(raw)}`;
        rect.appendChild(title);
        svg.appendChild(rect);

        if (n <= 12 && yKeys.length === 1) {
          const valTxt = svgEl("text", {
            x: bx + barW / 2,
            y: Math.max(10, top - 5),
            "text-anchor": "middle",
            fill: "var(--dm-text-secondary)",
            "font-size": "10",
          });
          valTxt.textContent = formatCompact(v);
          svg.appendChild(valTxt);
        }
      });

      if (i % stepEvery === 0) {
        const rawLbl = xText(row, i);
        const shortLbl = rawLbl.length > 13 ? `${rawLbl.slice(0, 12)}…` : rawLbl;
        const xTxt = svgEl("text", {
          x: padL + (i + 0.5) * groupStep,
          y: H - padB + 17,
          "text-anchor": "middle",
          fill: "var(--dm-text-secondary)",
          "font-size": "10.5",
        });
        xTxt.textContent = shortLbl;
        const t = svgEl("title");
        t.textContent = rawLbl;
        xTxt.appendChild(t);
        svg.appendChild(xTxt);
      }
    });
  } else {
    // Numeric and time x values are placed by value, so an uneven gap shows
    // as one. Categories, and values that cannot all be read, are spaced
    // evenly in row order.
    const xMin = xValues ? Math.min(...xValues) : NaN;
    const xMax = xValues ? Math.max(...xValues) : NaN;
    const byValue = Number.isFinite(xMin) && Number.isFinite(xMax) && xMax > xMin;
    const xForIdx = (i) =>
      byValue
        ? padL + ((xValues[i] - xMin) / (xMax - xMin)) * plotW
        : n === 1
          ? padL + plotW / 2
          : padL + (i / (n - 1)) * plotW;
    const baseY = yToPx(Math.max(0, minVal));

    if (isBand) {
      // Shade the range wherever both bounds are known; a row without bounds
      // (the start of a rolling window, say) splits the band.
      const bandColor = colors[0];
      let run = [];
      const flush = () => {
        if (run.length > 1) {
          const upper = run.map((r) => `${r.x},${r.hi}`);
          const lower = run.map((r) => `${r.x},${r.lo}`);
          svg.appendChild(
            svgEl("polygon", {
              points: [...upper, ...lower.slice().reverse()].join(" "),
              fill: bandColor,
              "fill-opacity": "0.14",
              stroke: "none",
            })
          );
          for (const edge of [upper, lower]) {
            svg.appendChild(
              svgEl("polyline", {
                points: edge.join(" "),
                fill: "none",
                stroke: bandColor,
                "stroke-opacity": "0.45",
                "stroke-width": "1",
                "stroke-dasharray": "3 3",
              })
            );
          }
        }
        run = [];
      };
      data.forEach((row, i) => {
        const bounds = boundsOf(row);
        if (!bounds) {
          flush();
          return;
        }
        run.push({ x: xForIdx(i), lo: yToPx(bounds.lo), hi: yToPx(bounds.hi) });
      });
      flush();
    }

    seriesKeys.forEach((yk, sIdx) => {
      const color = colors[sIdx % colors.length];
      const pts = [];
      data.forEach((row, i) => {
        const v = toNumber(row[yk]);
        if (Number.isFinite(v)) {
          pts.push({ x: xForIdx(i), y: yToPx(v), raw: row[yk], label: row[xKey] ?? i, value: v, row });
        }
      });
      if (!pts.length) return;

      if (kind === "area" && pts.length > 1) {
        const areaPts = [
          `${pts[0].x},${baseY}`,
          ...pts.map((p) => `${p.x},${p.y}`),
          `${pts[pts.length - 1].x},${baseY}`,
        ].join(" ");
        svg.appendChild(
          svgEl("polygon", {
            points: areaPts,
            fill: color,
            "fill-opacity": "0.14",
            stroke: "none",
          })
        );
      }

      if (kind !== "scatter" && pts.length > 1) {
        svg.appendChild(
          svgEl("polyline", {
            points: pts.map((p) => `${p.x},${p.y}`).join(" "),
            fill: "none",
            stroke: color,
            "stroke-width": isSpark ? "2" : "2.2",
            "stroke-linecap": "round",
            "stroke-linejoin": "round",
          })
        );
      }

      if (!isSpark) {
        for (const p of pts) {
          const bounds = isBand ? boundsOf(p.row) : null;
          const side = !bounds ? null : p.value > bounds.hi ? "above" : p.value < bounds.lo ? "below" : null;
          const note = side && comp.note_key ? String(p.row[comp.note_key] ?? "").trim() : "";
          if (side) {
            outside.push({
              label: p.label,
              raw: p.raw,
              side,
              bound: side === "above" ? bounds.hi : bounds.lo,
              note,
            });
          }
          const dot = svgEl("circle", {
            cx: p.x,
            cy: p.y,
            r: side ? "5" : isBand ? "2.4" : kind === "scatter" ? "4.5" : "3.2",
            fill: side ? OUTSIDE_COLOR : color,
            stroke: "var(--surface-card)",
            "stroke-width": "1.5",
          });
          const t = svgEl("title");
          t.textContent = `${p.label} · ${yk}: ${formatCellValue(p.raw)}`;
          if (bounds) {
            t.textContent += ` · range ${formatCellValue(bounds.lo)} – ${formatCellValue(bounds.hi)}`;
            if (side) t.textContent += ` · ${side} range`;
            if (note) t.textContent += ` · ${note}`;
          }
          dot.appendChild(t);
          svg.appendChild(dot);
        }
      }
    });

    if (!isSpark) {
      const stepEvery = Math.max(1, Math.ceil(n / 8));
      let labelled;
      if (byValue) {
        // Placed by value, points can bunch up: label one only where it
        // clears the label before it. Dates and times are written in full,
        // so the room a label needs follows from the longest one.
        const widest = formatTime ? Math.max(...data.map((row, i) => xText(row, i).length)) : 0;
        const minGap = Math.max(plotW / 9, widest * 6 + 14);
        let lastX = -Infinity;
        labelled = data
          .map((_, i) => i)
          .sort((a, b) => xValues[a] - xValues[b])
          .filter((i) => {
            if (xForIdx(i) - lastX < minGap) return false;
            lastX = xForIdx(i);
            return true;
          });
      } else {
        labelled = data.map((_, i) => i).filter((i) => i % stepEvery === 0 || i === n - 1);
      }
      for (const i of labelled) {
        const rawLbl = xText(data[i], i);
        const shortLbl =
          rawLbl.length > 13 && !(byValue && formatTime) ? `${rawLbl.slice(0, 12)}…` : rawLbl;
        const xTxt = svgEl("text", {
          x: xForIdx(i),
          y: H - padB + 17,
          "text-anchor": "middle",
          fill: "var(--dm-text-secondary)",
          "font-size": "10.5",
        });
        xTxt.textContent = shortLbl;
        svg.appendChild(xTxt);
      }
    }
  }

  wrap.appendChild(svg);

  if (comp.show_legend !== false && !isSpark && yKeys.length > 0) {
    const legend = document.createElement("div");
    style(legend, {
      display: "flex",
      flexWrap: "wrap",
      gap: "16px",
      justifyContent: "center",
      fontSize: "12.5px",
      color: "var(--dm-text-secondary)",
    });
    const legendItem = (label, swatchStyle) => {
      const item = document.createElement("span");
      style(item, { display: "inline-flex", alignItems: "center", gap: "7px" });
      const dot = document.createElement("span");
      style(dot, { width: "8px", height: "8px", borderRadius: "999px", ...swatchStyle });
      const txt = document.createElement("span");
      txt.textContent = label;
      item.append(dot, txt);
      legend.appendChild(item);
    };
    seriesKeys.forEach((yk, sIdx) => {
      legendItem(yk, { background: colors[sIdx % colors.length] });
    });
    if (isBand) {
      legendItem(String(comp.band_label || "Expected range"), {
        width: "14px",
        borderRadius: "3px",
        background: `color-mix(in srgb, ${colors[0]} 24%, transparent)`,
        border: `1px dashed color-mix(in srgb, ${colors[0]} 55%, transparent)`,
      });
      if (outside.length) legendItem("Outside range", { background: OUTSIDE_COLOR });
    }
    wrap.appendChild(legend);
  }

  // Hover alone would hide the notes on touch screens and from assistive
  // technology, so the out-of-range points are also listed under the chart.
  if (isBand && outside.length) {
    const list = document.createElement("ul");
    list.className = "dm-band-outside";
    style(list, {
      listStyle: "none",
      margin: "4px 0 0",
      padding: "0",
      display: "flex",
      flexDirection: "column",
      gap: "10px",
      fontSize: "13px",
      color: "var(--dm-text-primary)",
    });
    for (const point of outside.slice(0, MAX_LISTED_OUTSIDE)) {
      const li = document.createElement("li");
      style(li, { display: "flex", gap: "10px", alignItems: "baseline" });
      const marker = document.createElement("span");
      style(marker, {
        width: "8px",
        height: "8px",
        borderRadius: "999px",
        background: OUTSIDE_COLOR,
        flexShrink: "0",
        transform: "translateY(-1px)",
      });
      const body = document.createElement("div");
      style(body, { display: "flex", flexDirection: "column", gap: "2px", minWidth: "0" });
      const head = document.createElement("span");
      const when = document.createElement("strong");
      when.textContent = String(point.label);
      style(when, { fontWeight: "500" });
      const what = document.createElement("span");
      what.textContent = ` ${formatCellValue(point.raw)}, ${point.side} the range (${
        point.side === "above" ? "upper" : "lower"
      } bound ${formatCellValue(point.bound)})`;
      style(what, { color: "var(--dm-text-secondary)" });
      head.append(when, what);
      body.appendChild(head);
      if (point.note) {
        const noteEl = document.createElement("span");
        noteEl.textContent = point.note;
        body.appendChild(noteEl);
      }
      li.append(marker, body);
      list.appendChild(li);
    }
    if (outside.length > MAX_LISTED_OUTSIDE) {
      const more = document.createElement("li");
      more.textContent = `+${outside.length - MAX_LISTED_OUTSIDE} more outside the range`;
      style(more, { color: "var(--dm-text-secondary)" });
      list.appendChild(more);
    }
    wrap.appendChild(list);
  }

  return wrap;
};
