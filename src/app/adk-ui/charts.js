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

export const renderChartInner = (comp) => {
  const wrap = document.createElement("div");
  style(wrap, { display: "flex", flexDirection: "column", gap: "10px" });

  const data = Array.isArray(comp.data) ? comp.data : [];
  const xKey = comp.x_key || (data[0] ? Object.keys(data[0])[0] : "x");
  const yKeys =
    Array.isArray(comp.y_keys) && comp.y_keys.length
      ? comp.y_keys
      : data[0]
        ? Object.keys(data[0]).filter((k) => k !== xKey)
        : [];
  const colors = Array.isArray(comp.colors) && comp.colors.length ? comp.colors : PALETTE;
  const kind = String(comp.kind || "bar").toLowerCase();

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

    const svg = svgEl("svg", { viewBox: "0 0 210 210" });
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
  const W = 680;
  const H = isSpark ? 84 : 250;
  const padL = isSpark ? 8 : 52;
  const padR = isSpark ? 8 : 16;
  const padT = isSpark ? 8 : 16;
  const padB = isSpark ? 10 : 44;
  const plotW = W - padL - padR;
  const plotH = H - padT - padB;

  let minVal = 0;
  let maxVal = 0;
  for (const row of data) {
    for (const yk of yKeys) {
      const v = toNumber(row[yk]);
      if (Number.isFinite(v)) {
        if (v < minVal) minVal = v;
        if (v > maxVal) maxVal = v;
      }
    }
  }
  if (maxVal === minVal) maxVal = minVal + 1;
  const span = maxVal - minVal;
  const yToPx = (v) => padT + plotH - ((v - minVal) / span) * plotH;

  const svg = svgEl("svg", { viewBox: `0 0 ${W} ${H}` });
  style(svg, {
    width: "100%",
    height: "auto",
    maxHeight: isSpark ? "90px" : "265px",
    overflow: "visible",
    fill: "none",
    stroke: "none",
  });

  if (!isSpark) {
    const steps = 4;
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

  const n = data.length;
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
        const rawLbl = String(row[xKey] ?? "");
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
    const xForIdx = (i) => (n === 1 ? padL + plotW / 2 : padL + (i / (n - 1)) * plotW);
    const baseY = yToPx(Math.max(0, minVal));

    yKeys.forEach((yk, sIdx) => {
      const color = colors[sIdx % colors.length];
      const pts = [];
      data.forEach((row, i) => {
        const v = toNumber(row[yk]);
        if (Number.isFinite(v)) {
          pts.push({ x: xForIdx(i), y: yToPx(v), raw: row[yk], label: row[xKey] ?? i });
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
          const dot = svgEl("circle", {
            cx: p.x,
            cy: p.y,
            r: kind === "scatter" ? "4.5" : "3.2",
            fill: color,
            stroke: "var(--surface-card)",
            "stroke-width": "1.5",
          });
          const t = svgEl("title");
          t.textContent = `${p.label} · ${yk}: ${formatCellValue(p.raw)}`;
          dot.appendChild(t);
          svg.appendChild(dot);
        }
      }
    });

    if (!isSpark) {
      const stepEvery = Math.max(1, Math.ceil(n / 8));
      data.forEach((row, i) => {
        if (i % stepEvery !== 0 && i !== n - 1) return;
        const rawLbl = String(row[xKey] ?? "");
        const shortLbl = rawLbl.length > 13 ? `${rawLbl.slice(0, 12)}…` : rawLbl;
        const xTxt = svgEl("text", {
          x: xForIdx(i),
          y: H - padB + 17,
          "text-anchor": "middle",
          fill: "var(--dm-text-secondary)",
          "font-size": "10.5",
        });
        xTxt.textContent = shortLbl;
        svg.appendChild(xTxt);
      });
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
    yKeys.forEach((yk, sIdx) => {
      const item = document.createElement("span");
      style(item, { display: "inline-flex", alignItems: "center", gap: "7px" });
      const dot = document.createElement("span");
      style(dot, {
        width: "8px",
        height: "8px",
        borderRadius: "999px",
        background: colors[sIdx % colors.length],
      });
      const txt = document.createElement("span");
      txt.textContent = yk;
      item.append(dot, txt);
      legend.appendChild(item);
    });
    wrap.appendChild(legend);
  }

  return wrap;
};
