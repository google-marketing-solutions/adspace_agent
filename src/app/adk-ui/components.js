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
// `adk-ui` (`UiResponse`) Interactive Component Renderer (Right-Hand Canvas)
// ---------------------------------------------------------------------------

// Unique id prefix for each rendered `tabs` component (tab ↔ panel wiring).
import { createIcon, showToast, style } from "../dom.js";
import { formatCellValue, safeImageSrc, variantColors } from "../format.js";
import { appendInlineMarkdown, parseMarkdownBlocksInto } from "../markdown.js";
import { renderChartInner } from "./charts.js";
import { renderTableParts } from "./tables.js";
import { closeModalOverlay } from "./modal.js";
import {
  applyPointerPatch,
  evalA2uiCondition,
  formatActionSubmission,
  formatInsightNumber,
  formatInsightValue,
  resolveActionArgs,
  resolveDynamicString,
  resolveDynamicValue,
  resolveInsightTokenColor,
  resolvePointer,
} from "./a2ui.js";

let uiTabsSeq = 0;

// `input_type` values a `text_input` component may request; anything else
// (it comes from tool output) falls back to "text". `render_form` has no
// date component of its own: it tags a text input with the field's type.
const SAFE_INPUT_TYPES = [
  "text",
  "email",
  "url",
  "tel",
  "password",
  "search",
  "number",
  "date",
  "time",
  "datetime-local",
];

// Late binding: `sendMessage` (sse.js) sits above this module (sse.js renders
// through transcript.js → adk-ui), so app.js injects it via `bindSendMessage`.
// `isBusy` is the host's "a run is in flight" predicate (the studio passes
// `() => state.isRunning`); hosts without one leave the default.
let sendMessage = null;
let isBusy = () => false;
export const bindSendMessage = (fn, { isBusy: busyFn = null } = {}) => {
  sendMessage = fn;
  isBusy = typeof busyFn === "function" ? busyFn : () => false;
};

export const alertVariantLabel = (variant) => {
  const v = String(variant || "info").toLowerCase();
  if (v === "info") return "Info";
  if (v === "warn" || v === "warning") return "Warning";
  if (v === "error" || v === "danger" || v === "outage") return "Error";
  if (v === "success" || v === "ok") return "Success";
  return v.charAt(0).toUpperCase() + v.slice(1);
};

export const buildAlertParts = (alertComp) => {
  const variantLabel = alertVariantLabel(alertComp.variant);
  const titleText = String(alertComp.title || alertComp.message || "Alert");

  const content = document.createElement("div");
  content.className = "dm-alert-content";
  const titleEl = document.createElement("h3");
  titleEl.className = "dm-alert-title";
  titleEl.textContent = titleText;
  content.appendChild(titleEl);

  if (alertComp.description && String(alertComp.description).trim()) {
    const descEl = document.createElement("p");
    descEl.className = "dm-alert-desc";
    descEl.textContent = String(alertComp.description).trim();
    content.appendChild(descEl);
  }

  const statusBtn = document.createElement("span");
  statusBtn.className = "dm-alert-status-btn";
  statusBtn.textContent = variantLabel;

  return { content, statusBtn, variantLabel, titleText };
};

// Shared pure UI sub-renderers reused across both v2 legacy components and v3
// A2UI / `adk-ui` 3.0 extension components.
export const renderTextBlock = ({ content, variant = "body", inGrid = false, inModal = false }) => {
  const v = String(variant || "body").toLowerCase();
  const raw = String(content ?? "");
  const stepMatch = v === "body" && raw.match(/^(✅|⏳|⬜)\s+([\s\S]+)$/);
  if (stepMatch) {
    const row = document.createElement("div");
    const isDone = stepMatch[1] === "✅";
    const isCurrent = stepMatch[1] === "⏳";
    style(row, {
      display: "flex",
      alignItems: "center",
      gap: "10px",
      padding: "10px 14px",
      borderRadius: "14px",
      background: "var(--dm-surface-gray)",
      border: `1px solid ${isCurrent ? "var(--dm-text-primary)" : "var(--dm-outline)"}`,
      fontSize: "14px",
      color: isDone ? "var(--dm-text-secondary)" : "var(--dm-text-primary)",
      fontWeight: isCurrent ? "500" : "400",
    });
    const mark = document.createElement("span");
    mark.textContent = stepMatch[1];
    mark.setAttribute("aria-hidden", "true");
    const txt = document.createElement("span");
    appendInlineMarkdown(txt, stepMatch[2]);
    row.append(mark, txt);
    return row;
  }
  const el = document.createElement(v === "code" ? "code" : "div");
  if (v === "code") {
    el.textContent = raw;
  } else if (v === "body" && raw.includes("\n")) {
    el.className = "adk-ui-md";
    parseMarkdownBlocksInto(el, raw);
    style(el, { padding: "0" });
  } else {
    appendInlineMarkdown(el, raw);
  }
  const map = {
    h1: {
      fontSize: "22px",
      fontWeight: "450",
      letterSpacing: "-0.012em",
      lineHeight: "1.2",
      color: "var(--dm-text-primary)",
    },
    h2: {
      fontSize: inGrid ? "28px" : "19px",
      fontWeight: "450",
      letterSpacing: inGrid ? "-0.015em" : "-0.01em",
      lineHeight: inGrid ? "1.12" : "1.22",
      color: "var(--dm-text-primary)",
    },
    h3: {
      fontSize: inModal ? "17px" : "16.5px",
      fontWeight: "450",
      letterSpacing: "-0.006em",
      lineHeight: "1.32",
      color: "var(--dm-text-primary)",
    },
    h4: {
      fontSize: "14.5px",
      fontWeight: "500",
      lineHeight: "1.4",
      color: "var(--dm-text-primary)",
    },
    h5: {
      fontSize: "13.5px",
      fontWeight: "500",
      lineHeight: "1.4",
      color: "var(--dm-text-primary)",
    },
    caption: {
      fontSize: inModal ? "13.5px" : "13px",
      color: "var(--dm-text-secondary)",
      lineHeight: "1.45",
    },
    code: {
      fontFamily: "var(--mono)",
      fontSize: inModal ? "13px" : "12.5px",
      background: "var(--dm-surface-gray-subtle)",
      border: "1px solid var(--dm-outline)",
      padding: "1px 6px",
      borderRadius: "6px",
    },
    body: {
      fontSize: inModal ? "16px" : "14.5px",
      lineHeight: "1.55",
      color: inModal ? "var(--dm-text-secondary)" : "var(--dm-text-primary)",
    },
  };
  style(el, map[v] || map.body);
  return el;
};

export const renderBadgeControl = ({ text, tone = "neutral" }) => {
  const span = document.createElement("span");
  span.textContent = String(text ?? "");
  const vc = variantColors(tone);
  style(span, {
    display: "inline-flex",
    alignItems: "center",
    padding: "4px 10px",
    borderRadius: "999px",
    fontSize: "12px",
    fontWeight: "500",
    background: vc.bg,
    color: vc.fg,
    border: `1px solid ${vc.border || "transparent"}`,
    alignSelf: "flex-start",
  });
  return span;
};

export const renderTabsControl = ({
  label = "",
  tabs = [],
  initialIndex = 0,
  vertical = false,
  renderPanelContent,
}) => {
  const wrap = document.createElement("div");
  style(wrap, {
    display: "flex",
    flexDirection: vertical ? "row" : "column",
    flexWrap: vertical ? "wrap" : "nowrap",
    gap: vertical ? "18px" : "14px",
    alignItems: "stretch",
  });
  const bar = document.createElement("div");
  bar.setAttribute("role", "tablist");
  if (vertical) bar.setAttribute("aria-orientation", "vertical");
  if (label) bar.setAttribute("aria-label", String(label));
  style(
    bar,
    vertical
      ? {
          display: "flex",
          flexDirection: "column",
          gap: "4px",
          minWidth: "150px",
          borderRight: "1px solid var(--dm-outline)",
          paddingRight: "12px",
        }
      : {
          display: "flex",
          flexWrap: "wrap",
          gap: "4px",
          borderBottom: "1px solid var(--dm-outline)",
          paddingBottom: "0",
        }
  );
  const panel = document.createElement("div");
  panel.setAttribute("role", "tabpanel");
  panel.tabIndex = 0;
  if (vertical) style(panel, { flex: "1 1 260px", minWidth: "0" });

  const tabsId = `ui-tabs-${++uiTabsSeq}`;
  let activeIdx = Math.max(0, Math.min(Math.max(0, tabs.length - 1), initialIndex));

  const selectTab = (idx, { focus = false } = {}) => {
    activeIdx = idx;
    Array.from(bar.children).forEach((btn, i) => {
      const selected = i === activeIdx;
      btn.setAttribute("aria-selected", selected ? "true" : "false");
      btn.dataset.active = selected ? "true" : "false";
      btn.tabIndex = selected ? 0 : -1;
      style(
        btn,
        vertical
          ? {
              background: selected ? "var(--dm-surface-gray)" : "transparent",
              color: selected ? "var(--dm-text-primary)" : "var(--dm-text-secondary)",
              borderRadius: "10px",
            }
          : {
              background: "transparent",
              color: selected ? "var(--dm-text-primary)" : "var(--dm-text-secondary)",
              borderBottom: selected ? "2px solid var(--dm-text-primary)" : "2px solid transparent",
            }
      );
      if (selected && focus) btn.focus();
    });
    panel.setAttribute("aria-labelledby", `${tabsId}-tab-${idx}`);
    panel.replaceChildren();
    if (tabs[idx]) {
      const content = renderPanelContent(tabs[idx], idx);
      if (Array.isArray(content)) {
        for (const node of content) panel.appendChild(node);
      } else if (content) {
        panel.appendChild(content);
      }
    }
  };

  tabs.forEach((tab, idx) => {
    const b = document.createElement("button");
    b.type = "button";
    b.className = "adk-ui-tab";
    b.id = `${tabsId}-tab-${idx}`;
    b.setAttribute("role", "tab");
    b.setAttribute("aria-controls", `${tabsId}-panel`);
    b.textContent = tab.label || tab.title || `Tab ${idx + 1}`;
    style(
      b,
      vertical
        ? {
            border: "0",
            textAlign: "left",
            padding: "8px 12px",
            fontSize: "13.5px",
            fontWeight: "500",
            cursor: "pointer",
          }
        : {
            border: "0",
            borderBottom: "2px solid transparent",
            borderRadius: "0",
            padding: "10px 16px",
            marginBottom: "-1px",
            fontSize: "14px",
            fontWeight: "500",
            cursor: "pointer",
          }
    );
    b.addEventListener("click", () => selectTab(idx));
    b.addEventListener("keydown", (ev) => {
      const n = tabs.length;
      let next = null;
      if (ev.key === "ArrowRight" || ev.key === "ArrowDown") next = (idx + 1) % n;
      else if (ev.key === "ArrowLeft" || ev.key === "ArrowUp") next = (idx - 1 + n) % n;
      else if (ev.key === "Home") next = 0;
      else if (ev.key === "End") next = n - 1;
      if (next === null) return;
      ev.preventDefault();
      selectTab(next, { focus: true });
    });
    bar.appendChild(b);
  });
  panel.id = `${tabsId}-panel`;
  wrap.append(bar, panel);
  if (tabs.length) selectTab(activeIdx);
  return wrap;
};

export const renderKeyValueControl = ({ title = "", pairs = [] }) => {
  const wrap = document.createElement("div");
  style(wrap, { display: "flex", flexDirection: "column", gap: "8px" });
  if (title) {
    wrap.appendChild(renderTextBlock({ content: title, variant: "h4" }));
  }
  const dl = document.createElement("div");
  style(dl, {
    display: "grid",
    gridTemplateColumns: "minmax(140px, auto) 1fr",
    gap: "10px 20px",
    padding: "16px 18px",
    borderRadius: "18px",
    background: "var(--dm-surface-gray)",
    border: "1px solid var(--dm-outline)",
    fontSize: "13.5px",
  });
  for (const pair of pairs) {
    const k = document.createElement("div");
    k.textContent = String(pair.key ?? "");
    style(k, { color: "var(--dm-text-secondary)", fontWeight: "450" });
    const v = document.createElement("div");
    v.textContent = String(pair.value ?? "");
    style(v, { color: "var(--dm-text-primary)", fontWeight: "500" });
    dl.append(k, v);
  }
  wrap.appendChild(dl);
  return wrap;
};

export const renderProgressControl = ({ label, value = 0, tone = "neutral", indeterminate = false }) => {
  const wrap = document.createElement("div");
  style(wrap, { display: "flex", flexDirection: "column", gap: "8px" });
  const pct = Math.max(0, Math.min(100, Number(value) || 0));
  const top = document.createElement("div");
  style(top, { display: "flex", justifyContent: "space-between", fontSize: "13.5px" });
  const lbl = document.createElement("span");
  lbl.textContent = label || "Progress";
  style(lbl, { color: "var(--dm-text-primary)", fontWeight: "450" });
  const val = document.createElement("span");
  val.textContent = indeterminate ? "In progress" : `${formatInsightNumber(pct)}%`;
  style(val, { color: "var(--dm-text-secondary)", fontFamily: "var(--mono)" });
  top.append(lbl, val);

  const track = document.createElement("div");
  track.setAttribute("role", "progressbar");
  if (!indeterminate) {
    track.setAttribute("aria-valuemin", "0");
    track.setAttribute("aria-valuemax", "100");
    track.setAttribute("aria-valuenow", String(pct));
  }
  style(track, {
    height: "8px",
    borderRadius: "999px",
    background: "var(--dm-surface-gray)",
    overflow: "hidden",
  });
  const barColor = resolveInsightTokenColor(tone) || "var(--dm-text-primary)";
  const bar = document.createElement("div");
  style(
    bar,
    indeterminate
      ? {
          width: "35%",
          height: "100%",
          borderRadius: "999px",
          background: barColor,
          animation: "ui-progress-indeterminate 1.4s ease-in-out infinite",
        }
      : {
          width: `${pct}%`,
          height: "100%",
          borderRadius: "999px",
          background: barColor,
        }
  );
  track.appendChild(bar);
  wrap.append(top, track);
  return wrap;
};

// `ctx` is the render context: `inGrid` (KPI tile inside a grid) and
// `inModal` (anywhere inside the modal overlay). It is threaded explicitly
// because children are rendered while their parents are still detached, so
// DOM-based `closest()` sniffing cannot see the modal.
export const renderInnerComponent = (comp, surfaceRoot, ctx = {}) => {
  if (!comp || typeof comp !== "object") return document.createTextNode("");
  const { inGrid = false, inModal = false } = ctx || {};
  const type = String(comp.type || "").toLowerCase();

  switch (type) {
    case "text":
      return renderTextBlock({
        content: comp.content,
        variant: comp.variant,
        inGrid,
        inModal,
      });

    case "badge":
      return renderBadgeControl({ text: comp.label ?? "", tone: comp.variant });

    case "divider": {
      const hr = document.createElement("hr");
      style(hr, {
        border: "0",
        borderTop: "1px solid var(--dm-outline)",
        margin: "6px 0",
        width: "100%",
      });
      return hr;
    }

    case "stack": {
      const div = document.createElement("div");
      const horiz = String(comp.direction || "vertical").toLowerCase() === "horizontal";
      const gapPx = Math.max(1, Number(comp.gap ?? 3)) * 4;
      style(div, {
        display: "flex",
        flexDirection: horiz ? "row" : "column",
        flexWrap: horiz ? "wrap" : "nowrap",
        alignItems: horiz ? "center" : "stretch",
        gap: `${gapPx}px`,
      });
      for (const child of comp.children || []) {
        div.appendChild(renderInnerComponent(child, surfaceRoot, ctx));
      }
      return div;
    }

    case "grid": {
      const div = document.createElement("div");
      const cols = Math.max(1, Math.min(6, Number(comp.columns) || 2));
      style(div, {
        display: "grid",
        gridTemplateColumns: `repeat(auto-fit, minmax(${cols >= 3 ? "165px" : "220px"}, 1fr))`,
        gap: "14px",
        alignItems: "stretch",
      });
      for (const child of comp.children || []) {
        div.appendChild(renderInnerComponent(child, surfaceRoot, { inGrid: true, inModal }));
      }
      return div;
    }

    case "container": {
      const div = document.createElement("div");
      style(div, { display: "flex", flexDirection: "column", gap: "12px" });
      for (const child of comp.children || []) {
        div.appendChild(renderInnerComponent(child, surfaceRoot, ctx));
      }
      return div;
    }

    case "card":
    case "modal": {
      const tile = document.createElement("div");
      tile.setAttribute("data-ui-scope", "true");
      style(tile, {
        display: "flex",
        flexDirection: "column",
        justifyContent: "space-between",
        gap: "8px",
        padding: inGrid ? "18px 20px" : "20px 22px",
        borderRadius: "20px",
        background: "var(--dm-surface-gray)",
        // KPI tiles inside a grid are borderless; standalone cards keep the outline.
        border: inGrid ? "none" : "1px solid var(--dm-outline)",
      });

      if (comp.title) {
        const t = document.createElement("div");
        t.textContent = comp.title;
        style(
          t,
          inGrid
            ? {
                fontSize: "13.5px",
                fontWeight: "450",
                letterSpacing: "0.005em",
                color: "var(--dm-text-secondary)",
              }
            : {
                fontSize: "16.5px",
                fontWeight: "450",
                letterSpacing: "-0.006em",
                color: "var(--dm-text-primary)",
              }
        );
        tile.appendChild(t);
      }
      if (comp.description) {
        const d = document.createElement("div");
        d.textContent = comp.description;
        style(d, { fontSize: "13.5px", color: "var(--dm-text-secondary)", lineHeight: "1.45" });
        tile.appendChild(d);
      }
      if (Array.isArray(comp.content) && comp.content.length) {
        const body = document.createElement("div");
        style(body, { display: "flex", flexDirection: "column", gap: "8px" });
        for (const child of comp.content) {
          body.appendChild(renderInnerComponent(child, tile, ctx));
        }
        tile.appendChild(body);
      }
      if (Array.isArray(comp.footer) && comp.footer.length) {
        const ftr = document.createElement("div");
        style(ftr, {
          display: "flex",
          flexWrap: "wrap",
          gap: "8px",
          justifyContent: "flex-end",
          paddingTop: "10px",
        });
        for (const child of comp.footer) {
          ftr.appendChild(renderInnerComponent(child, tile, { inModal }));
        }
        tile.appendChild(ftr);
      }
      return tile;
    }

    case "tabs":
      return renderTabsControl({
        label: comp.label || "",
        tabs: Array.isArray(comp.tabs) ? comp.tabs : [],
        renderPanelContent: (tab) =>
          (tab?.content || []).map((child) => renderInnerComponent(child, surfaceRoot, { inModal })),
      });

    case "chart":
      return renderChartInner(comp);

    case "table": {
      const { element, toolbar } = renderTableParts(comp);
      const box = document.createElement("div");
      style(box, {
        border: "1px solid var(--dm-outline)",
        borderRadius: "18px",
        overflow: "hidden",
      });
      const bar = document.createElement("div");
      style(bar, {
        display: "flex",
        justifyContent: "flex-end",
        padding: "10px 16px",
        background: "transparent",
        borderBottom: "1px solid var(--dm-outline)",
      });
      bar.appendChild(toolbar);
      box.append(bar, element);
      return box;
    }

    case "key_value":
      return renderKeyValueControl({ pairs: comp.pairs || [] });

    case "list": {
      const list = document.createElement(comp.ordered ? "ol" : "ul");
      style(list, {
        margin: "0",
        paddingLeft: "22px",
        display: "flex",
        flexDirection: "column",
        gap: "6px",
        fontSize: "14.5px",
        lineHeight: "1.55",
        color: "var(--dm-text-primary)",
      });
      for (const item of comp.items || []) {
        const li = document.createElement("li");
        li.textContent = item;
        list.appendChild(li);
      }
      return list;
    }

    case "code_block": {
      const pre = document.createElement("pre");
      pre.textContent = comp.code ?? "";
      style(pre, {
        margin: "0",
        padding: "14px 16px",
        borderRadius: "14px",
        background: "var(--dm-surface-gray)",
        border: "1px solid var(--dm-outline)",
        color: "var(--dm-text-primary)",
        fontSize: "12.5px",
        overflowX: "auto",
        fontFamily: "var(--mono)",
      });
      return pre;
    }

    case "alert":
    case "toast": {
      const box = document.createElement("div");
      box.className = "dm-alert-inline";
      const { content, statusBtn } = buildAlertParts(comp);
      box.append(content, statusBtn);
      return box;
    }

    case "progress":
      return renderProgressControl({ label: comp.label, value: comp.value });

    case "text_input":
    case "number_input":
    case "date_input":
    case "textarea":
    case "select":
    case "multi_select":
    case "slider":
    case "switch": {
      const field = document.createElement("label");
      style(field, { display: "flex", flexDirection: "column", gap: "6px", fontSize: "13.5px" });
      const lbl = document.createElement("span");
      lbl.textContent = `${comp.label || comp.name}${comp.required ? " *" : ""}`;
      style(lbl, { fontWeight: "450", color: "var(--dm-text-secondary)" });

      let sliderValEl = null;
      if (type === "slider") {
        const lblRow = document.createElement("div");
        style(lblRow, { display: "flex", justifyContent: "space-between", alignItems: "center" });
        sliderValEl = document.createElement("code");
        sliderValEl.textContent = String(comp.default_value ?? comp.min ?? 0);
        style(sliderValEl, {
          fontFamily: "var(--mono)",
          fontSize: "12.5px",
          color: "var(--dm-text-primary)",
        });
        lblRow.append(lbl, sliderValEl);
        field.appendChild(lblRow);
      } else {
        field.appendChild(lbl);
      }

      let control;
      if (type === "textarea") {
        control = document.createElement("textarea");
        control.rows = Number(comp.rows) || 3;
        control.placeholder = comp.placeholder || "";
        control.value = comp.default_value || "";
      } else if (type === "select" || type === "multi_select") {
        control = document.createElement("select");
        if (type === "multi_select") control.multiple = true;
        const hasDefault =
          comp.default_value !== undefined &&
          comp.default_value !== null &&
          String(comp.default_value) !== "";
        if (type === "select" && (comp.placeholder || (comp.required && !hasDefault))) {
          const ph = document.createElement("option");
          ph.value = "";
          ph.textContent = comp.placeholder || "Select an option…";
          ph.disabled = true;
          ph.selected = !hasDefault;
          control.appendChild(ph);
        }
        for (const opt of comp.options || []) {
          const o = document.createElement("option");
          if (opt && typeof opt === "object") {
            o.value = String(opt.value ?? opt.label ?? "");
            o.textContent = String(opt.label ?? opt.value ?? "");
          } else {
            // Plain-string options: the string is both value and label.
            o.value = String(opt ?? "");
            o.textContent = o.value;
          }
          control.appendChild(o);
        }
        if (hasDefault) {
          if (control.multiple) {
            const defaults = (
              Array.isArray(comp.default_value) ? comp.default_value : [comp.default_value]
            ).map((d) => String(d));
            for (const o of control.options) o.selected = defaults.includes(o.value);
          } else {
            control.value = String(comp.default_value);
          }
        }
      } else if (type === "switch") {
        const row = document.createElement("div");
        style(row, { display: "flex", alignItems: "center", gap: "10px", padding: "4px 0" });
        control = document.createElement("input");
        control.type = "checkbox";
        control.checked = !!comp.default_checked;
        row.append(control, lbl);
        field.replaceChildren(row);
      } else {
        control = document.createElement("input");
        const requested = String(comp.input_type || "text").toLowerCase();
        control.type =
          type === "number_input"
            ? "number"
            : type === "date_input"
              ? "date"
              : type === "slider"
                ? "range"
                : SAFE_INPUT_TYPES.includes(requested)
                  ? requested
                  : "text";
        if (comp.placeholder) control.placeholder = comp.placeholder;
        if (comp.min !== undefined && comp.min !== null) control.min = String(comp.min);
        if (comp.max !== undefined && comp.max !== null) control.max = String(comp.max);
        if (comp.step !== undefined && comp.step !== null) control.step = String(comp.step);
        if (comp.pattern) control.pattern = String(comp.pattern);
        if (comp.default_value !== undefined && comp.default_value !== null) {
          control.value = String(comp.default_value);
        }
        if (type === "slider" && sliderValEl) {
          sliderValEl.textContent = control.value;
          control.addEventListener("input", () => {
            sliderValEl.textContent = control.value;
          });
        }
        if (type !== "slider") {
          control.addEventListener("keydown", (ev) => {
            if (ev.key === "Enter") {
              ev.preventDefault();
              const scope = control.closest('[data-ui-scope="true"]') || surfaceRoot;
              const submitBtn = scope?.querySelector('button[data-ui-primary="true"]');
              if (submitBtn && !submitBtn.disabled) submitBtn.click();
            }
          });
        }
      }

      if (comp.required) {
        control.required = true;
        control.setAttribute("aria-required", "true");
      }
      control.setAttribute("data-ui-label", comp.label || comp.name || "Field");

      if (type !== "switch") {
        style(control, {
          padding:
            type === "select"
              ? "10px 34px 10px 14px"
              : type === "slider"
                ? "4px 0"
                : "10px 14px",
          borderRadius: "12px",
          border: type === "slider" ? "0" : "1px solid var(--dm-outline)",
          backgroundColor: type === "slider" ? "transparent" : "var(--dm-surface-gray)",
          color: "var(--dm-text-primary)",
          fontSize: "14px",
        });
        field.appendChild(control);
      }

      const errEl = document.createElement("span");
      errEl.className = "ui-field-error hidden";
      style(errEl, {
        fontSize: "12px",
        fontWeight: "450",
        color: "var(--accent-error)",
      });
      field.appendChild(errEl);

      const clearFieldError = () => {
        if (type !== "switch" && type !== "slider") {
          control.style.borderColor = "var(--dm-outline)";
        }
        control.removeAttribute("aria-invalid");
        errEl.textContent = "";
        errEl.classList.add("hidden");
      };
      control.addEventListener("input", clearFieldError);
      control.addEventListener("change", clearFieldError);

      control.setAttribute("data-ui-field", comp.name || comp.label || "field");
      return field;
    }

    case "button": {
      const btn = document.createElement("button");
      btn.type = "button";
      btn.textContent = comp.label || "Submit";
      btn.disabled = !!comp.disabled;
      const v = String(comp.variant || "primary").toLowerCase();
      if (v === "primary" || v === "danger") {
        btn.setAttribute("data-ui-primary", "true");
      }
      const btnStyles =
        v === "primary"
          ? {
              background: "var(--primary-btn-bg)",
              color: "var(--primary-btn-fg)",
              border: "1px solid transparent",
            }
          : v === "danger"
            ? {
                background: "var(--accent-error)",
                color: "#fff",
                border: "1px solid var(--accent-error)",
              }
            : v === "outline"
              ? {
                  background: "transparent",
                  color: "var(--dm-text-primary)",
                  border: "1px solid var(--dm-outline)",
                  boxShadow: "0 0 0 1px var(--dm-outline)",
                }
              : v === "ghost"
                ? {
                    background: "transparent",
                    color: "var(--dm-text-secondary)",
                    border: "1px solid transparent",
                  }
                : {
                    background: inModal ? "var(--dm-surface-gray-subtle)" : "var(--dm-surface-gray)",
                    color: "var(--dm-text-primary)",
                    border: "1px solid var(--dm-outline)",
                  };
      style(btn, {
        padding: "10px 22px",
        borderRadius: "999px",
        fontSize: inModal ? "14.5px" : "14px",
        fontWeight: "500",
        letterSpacing: "-0.004em",
        cursor: btn.disabled ? "default" : "pointer",
        opacity: btn.disabled ? "0.45" : "1",
        alignSelf: "flex-start",
        transition: "opacity 0.15s ease, transform 0.12s ease",
        ...btnStyles,
      });

      btn.addEventListener("click", () => {
        // Programmatic sends must never double as the composer's stop toggle:
        // while a run is streaming, refuse instead of aborting it and dropping
        // the form payload.
        if (isBusy()) {
          showToast({ variant: "warning", message: "Wait for the agent to finish." });
          return;
        }
        const scope = btn.closest('[data-ui-scope="true"]') || surfaceRoot;
        const inputs = scope ? Array.from(scope.querySelectorAll("[data-ui-field]")) : [];
        const actionId = comp.action_id || comp.label || "click";
        const isCancelAction =
          v === "secondary" ||
          v === "ghost" ||
          /^(cancel|dismiss|close|skip|reset)\b/i.test(actionId) ||
          /^(cancel|dismiss|close|skip|reset)\b/i.test(comp.label || "");

        if (inputs.length > 0 && !isCancelAction) {
          let firstInvalid = null;
          for (const inp of inputs) {
            const fieldLabel = inp.getAttribute("data-ui-label") || inp.getAttribute("data-ui-field") || "Field";
            const errEl = inp.closest("label")?.querySelector(".ui-field-error");
            let errMsg = "";

            if (inp.required) {
              const isEmpty =
                inp.type === "checkbox"
                  ? !inp.checked
                  : inp.multiple
                    ? inp.selectedOptions.length === 0
                    : String(inp.value ?? "").trim() === "";
              if (isEmpty) {
                errMsg = `${fieldLabel} is required.`;
              }
            }
            if (!errMsg && typeof inp.checkValidity === "function" && !inp.checkValidity()) {
              errMsg = inp.validationMessage || `${fieldLabel} is invalid.`;
            }

            if (errMsg) {
              if (inp.type !== "checkbox" && inp.type !== "range") {
                inp.style.borderColor = "var(--accent-error)";
              }
              inp.setAttribute("aria-invalid", "true");
              if (errEl) {
                errEl.textContent = errMsg;
                errEl.classList.remove("hidden");
              }
              if (!firstInvalid) firstInvalid = inp;
            }
          }

          if (firstInvalid) {
            firstInvalid.focus();
            if (typeof firstInvalid.reportValidity === "function") {
              firstInvalid.reportValidity();
            }
            return;
          }
        }

        let msg;
        if (inputs.length > 0 && !isCancelAction) {
          const lines = inputs.map((inp) => {
            const name = inp.getAttribute("data-ui-field");
            const val =
              inp.type === "checkbox"
                ? String(inp.checked)
                : inp.multiple
                  ? Array.from(inp.selectedOptions)
                      .map((o) => o.value)
                      .join(", ")
                  : inp.value;
            return `  ${name}: ${val}`;
          });
          msg = `[UI Event: Form submitted]\nAction: ${actionId}\nData:\n${lines.join("\n")}`;
        } else {
          msg = `[UI Event: Button clicked]\nAction: ${actionId}`;
        }
        const orig = btn.textContent;
        btn.textContent = `${orig} ✓`;
        setTimeout(() => {
          btn.textContent = orig;
        }, 1800);
        if (btn.closest('[data-ui-modal="true"]')) {
          closeModalOverlay();
        }
        sendMessage(msg);
      });
      return btn;
    }

    case "icon": {
      const rawName = String(comp.name || "info").toLowerCase().replace(/-/g, "_");
      const iconMap = {
        check: "check_circle",
        alert_triangle: "warning",
        alert_circle: "error",
        x: "close",
      };
      const span = createIcon(iconMap[rawName] || rawName, {
        size: Math.max(14, Number(comp.size) || 18),
      });
      style(span, { color: "var(--accent-primary)" });
      return span;
    }

    case "image": {
      const img = document.createElement("img");
      img.src = safeImageSrc(comp.src);
      img.alt = comp.alt || "";
      style(img, {
        maxWidth: "100%",
        height: "auto",
        borderRadius: "10px",
        border: "1px solid var(--border-default)",
      });
      return img;
    }

    case "spinner": {
      const wrap = document.createElement("div");
      style(wrap, { display: "inline-flex", alignItems: "center", gap: "8px" });
      const ring = document.createElement("span");
      const sz = comp.size === "small" ? "14px" : comp.size === "large" ? "24px" : "18px";
      style(ring, {
        width: sz,
        height: sz,
        borderRadius: "50%",
        border: "2px solid var(--border-strong)",
        borderTopColor: "var(--accent-primary)",
        display: "inline-block",
        animation: "ui-spin 0.8s linear infinite",
      });
      wrap.appendChild(ring);
      if (comp.label) {
        const lbl = document.createElement("span");
        lbl.textContent = comp.label;
        style(lbl, { fontSize: "12px", color: "var(--text-secondary)" });
        wrap.appendChild(lbl);
      }
      return wrap;
    }

    case "skeleton": {
      const sk = document.createElement("div");
      style(sk, {
        width: comp.width || "100%",
        height: comp.height || "16px",
        borderRadius: comp.circle ? "50%" : "8px",
        background: "var(--bg-primary)",
        border: "1px solid var(--border-default)",
        animation: "ui-shimmer 1.4s ease-in-out infinite",
      });
      return sk;
    }

    default: {
      if (!type) return document.createTextNode("");
      // Say so instead of drawing nothing: a blank card hides a typo in a
      // component's `type`, or a component this renderer has no case for.
      const note = document.createElement("p");
      note.className = "dm-unsupported";
      note.textContent = `Unsupported component: ${type}`;
      style(note, { margin: "0", fontSize: "12.5px", color: "var(--text-muted)" });
      return note;
    }
  }
};

// ---------------------------------------------------------------------------
// A2UI v0.9.1 + `adk-ui` 3.0 (`forms` 1.0, `insight` 1.0, `studio` 1.0) Renderer
// ---------------------------------------------------------------------------

const FLEX_JUSTIFY = {
  start: "flex-start",
  end: "flex-end",
  center: "center",
  spaceBetween: "space-between",
  spaceAround: "space-around",
  spaceEvenly: "space-evenly",
  stretch: "stretch",
};

const FLEX_ALIGN = {
  start: "flex-start",
  center: "center",
  end: "flex-end",
  stretch: "stretch",
};

const applyA2uiButtonStyles = (btn, variant, { inModal = false, disabled = false } = {}) => {
  const v = String(variant || "default").toLowerCase();
  if (v === "primary" || v === "danger") {
    btn.setAttribute("data-ui-primary", "true");
  }
  const btnStyles =
    v === "primary"
      ? {
          background: "var(--primary-btn-bg)",
          color: "var(--primary-btn-fg)",
          border: "1px solid transparent",
        }
      : v === "danger"
        ? {
            background: "var(--accent-error)",
            color: "#fff",
            border: "1px solid var(--accent-error)",
          }
        : v === "borderless" || v === "ghost"
          ? {
              background: "transparent",
              color: "var(--dm-text-primary)",
              border: "1px solid transparent",
            }
          : {
              background: inModal ? "var(--dm-surface-gray-subtle)" : "var(--dm-surface-gray)",
              color: "var(--dm-text-primary)",
              border: "1px solid var(--dm-outline)",
            };
  style(btn, {
    padding: "10px 22px",
    borderRadius: "999px",
    fontSize: inModal ? "14.5px" : "14px",
    fontWeight: "500",
    letterSpacing: "-0.004em",
    cursor: disabled ? "default" : "pointer",
    opacity: disabled ? "0.45" : "1",
    alignSelf: "flex-start",
    transition: "opacity 0.15s ease, transform 0.12s ease",
    ...btnStyles,
  });
};

const createA2uiFieldWrapper = (labelText, { isRequired = false, headerRightEl = null } = {}) => {
  const field = document.createElement("label");
  style(field, { display: "flex", flexDirection: "column", gap: "6px", fontSize: "13.5px" });
  const lbl = document.createElement("span");
  lbl.textContent = `${labelText}${isRequired ? " *" : ""}`;
  style(lbl, { fontWeight: "450", color: "var(--dm-text-secondary)" });

  if (headerRightEl) {
    const row = document.createElement("div");
    style(row, { display: "flex", justifyContent: "space-between", alignItems: "center" });
    row.append(lbl, headerRightEl);
    field.appendChild(row);
  } else if (labelText) {
    field.appendChild(lbl);
  }

  const errEl = document.createElement("span");
  errEl.className = "ui-field-error hidden";
  style(errEl, {
    fontSize: "12px",
    fontWeight: "450",
    color: "var(--accent-error)",
  });
  return { field, lbl, errEl };
};

const styleInputBox = (el, kind = "text") => {
  style(el, {
    padding:
      kind === "select" ? "10px 34px 10px 14px" : kind === "slider" ? "4px 0" : "10px 14px",
    borderRadius: "12px",
    border: kind === "slider" ? "0" : "1px solid var(--dm-outline)",
    backgroundColor: kind === "slider" ? "transparent" : "var(--dm-surface-gray)",
    color: "var(--dm-text-primary)",
    fontSize: "14px",
  });
};

const bindA2uiField = (runtime, { comp, control, focusTarget = null, errEl, path, label, getValue, styledBorder = true }) => {
  const clearError = () => {
    if (styledBorder && control) control.style.borderColor = "var(--dm-outline)";
    (focusTarget || control)?.removeAttribute("aria-invalid");
    errEl.textContent = "";
    errEl.classList.add("hidden");
  };
  const syncModel = () => {
    if (path && path.startsWith("/")) {
      runtime.dataModel = applyPointerPatch(runtime.dataModel, path, getValue());
    }
    clearError();
  };
  const setError = (msg) => {
    if (styledBorder && control) control.style.borderColor = "var(--accent-error)";
    (focusTarget || control)?.setAttribute("aria-invalid", "true");
    errEl.textContent = msg;
    errEl.classList.remove("hidden");
  };
  runtime.inputs.push({
    comp,
    control,
    focusTarget: focusTarget || control,
    path,
    label,
    getValue,
    syncModel,
    setError,
    clearError,
  });
  return { syncModel, clearError };
};

const resolveChildrenSpec = (childrenSpec, dataModel, scopeData) => {
  if (Array.isArray(childrenSpec)) {
    return childrenSpec.map((id) => ({ id, scopeData }));
  }
  if (
    childrenSpec &&
    typeof childrenSpec === "object" &&
    typeof childrenSpec.componentId === "string" &&
    typeof childrenSpec.path === "string"
  ) {
    const arr = resolvePointer(dataModel, scopeData, childrenSpec.path);
    if (Array.isArray(arr)) {
      return arr.map((item) => ({ id: childrenSpec.componentId, scopeData: item }));
    }
  }
  return [];
};

const buildLayoutContainer = (layout, columns = 2) => {
  const el = document.createElement("div");
  if (layout === "grid") {
    const cols = Math.max(1, Math.min(4, Number(columns) || 2));
    style(el, {
      display: "grid",
      gridTemplateColumns: `repeat(auto-fit, minmax(${cols >= 3 ? "165px" : "220px"}, 1fr))`,
      gap: "14px",
      alignItems: "stretch",
    });
  } else if (layout === "split") {
    style(el, {
      display: "grid",
      gridTemplateColumns: "repeat(auto-fit, minmax(260px, 1fr))",
      gap: "16px",
      alignItems: "start",
    });
  } else {
    style(el, { display: "flex", flexDirection: "column", gap: "14px" });
  }
  return el;
};

const renderA2uiChart = (comp, dataModel, scopeData, { skipTitle = false } = {}) => {
  const rawTitle = comp.title ? resolveDynamicString(comp.title, dataModel, scopeData) : "";
  const rawData = resolveDynamicValue(comp.data, dataModel, scopeData);
  const rows = Array.isArray(rawData) ? rawData : [];
  const xSpec = comp.x && typeof comp.x === "object" ? comp.x : null;
  const adapted = {
    ...comp,
    title: rawTitle || "Chart",
    data: rows,
    x: xSpec
      ? {
          ...xSpec,
          label: xSpec.label ? resolveDynamicString(xSpec.label, dataModel, scopeData) : "",
        }
      : undefined,
    series: Array.isArray(comp.series)
      ? comp.series.map((s) => ({
          ...s,
          label: s?.label ? resolveDynamicString(s.label, dataModel, scopeData) : s?.key || "",
        }))
      : comp.series,
    yLabel: comp.yLabel ? resolveDynamicString(comp.yLabel, dataModel, scopeData) : "",
  };
  const chartEl = renderChartInner(adapted);
  if (skipTitle || !rawTitle) return chartEl;
  const wrap = document.createElement("div");
  style(wrap, { display: "flex", flexDirection: "column", gap: "8px" });
  wrap.append(renderTextBlock({ content: rawTitle, variant: "h4" }), chartEl);
  return wrap;
};

const renderA2uiSparkline = (comp, dataModel, scopeData) => {
  const rawData = resolveDynamicValue(comp.data, dataModel, scopeData);
  const items = Array.isArray(rawData) ? rawData : [];
  const key = comp.key;
  const rows = items
    .map((item, idx) => {
      const val =
        key && item && typeof item === "object" ? Number(item[key]) : Number(item);
      return Number.isFinite(val) ? { __i: idx, __v: val } : null;
    })
    .filter(Boolean);
  const label = comp.label ? resolveDynamicString(comp.label, dataModel, scopeData) : "";
  const toneColor = resolveInsightTokenColor(comp.tone) || resolveInsightTokenColor("chart1");
  const wrap = document.createElement("div");
  style(wrap, { display: "flex", flexDirection: "column", gap: "6px" });
  if (label) wrap.appendChild(renderTextBlock({ content: label, variant: "caption" }));
  wrap.appendChild(
    renderChartInner({
      kind: "sparkline",
      title: label || "Sparkline",
      data: rows,
      x_key: "__i",
      y_keys: ["__v"],
      colors: [toneColor],
      window: comp.window,
      show_legend: false,
    })
  );
  return wrap;
};

const STATUS_BADGE_MAP = {
  ok: { label: "OK", tone: "success" },
  warning: { label: "Warning", tone: "warning" },
  critical: { label: "Critical", tone: "danger" },
  info: { label: "Info", tone: "info" },
  neutral: { label: "Neutral", tone: "neutral" },
};

const renderA2uiMetric = (comp, dataModel, scopeData, { inGrid = false } = {}) => {
  const label = resolveDynamicString(comp.label, dataModel, scopeData);
  const rawValue = resolveDynamicValue(comp.value, dataModel, scopeData);
  const unit = comp.unit !== undefined ? resolveDynamicString(comp.unit, dataModel, scopeData) : null;
  const formatted = formatInsightValue(rawValue, {
    format: comp.format || null,
    currency: comp.currency || null,
    unit,
  });

  const tile = document.createElement("div");
  tile.setAttribute("data-ui-scope", "true");
  style(tile, {
    display: "flex",
    flexDirection: "column",
    justifyContent: "space-between",
    gap: "8px",
    padding: inGrid ? "18px 20px" : "20px 22px",
    borderRadius: "20px",
    background: "var(--dm-surface-gray)",
    border: inGrid ? "none" : "1px solid var(--dm-outline)",
  });

  const topRow = document.createElement("div");
  style(topRow, { display: "flex", alignItems: "center", justifyContent: "space-between", gap: "8px" });
  const lblEl = document.createElement("span");
  lblEl.textContent = label;
  style(lblEl, {
    fontSize: "13.5px",
    fontWeight: "450",
    letterSpacing: "0.005em",
    color: "var(--dm-text-secondary)",
  });
  topRow.appendChild(lblEl);

  const statusMeta = comp.status ? STATUS_BADGE_MAP[String(comp.status).toLowerCase()] : null;
  if (statusMeta) {
    topRow.appendChild(renderBadgeControl({ text: statusMeta.label, tone: statusMeta.tone }));
  }
  tile.appendChild(topRow);

  const valEl = document.createElement("div");
  valEl.textContent = formatted || "—";
  style(valEl, {
    fontSize: "28px",
    fontWeight: "450",
    letterSpacing: "-0.015em",
    lineHeight: "1.12",
    color: "var(--dm-text-primary)",
    fontFamily: "var(--sans)",
  });
  tile.appendChild(valEl);

  if (comp.delta && typeof comp.delta === "object") {
    const rawDelta = resolveDynamicValue(comp.delta.value, dataModel, scopeData);
    const numDelta = Number(rawDelta);
    if (Number.isFinite(numDelta)) {
      const goodWhen = comp.delta.goodWhen;
      const better =
        (goodWhen === "up" && numDelta > 0) || (goodWhen === "down" && numDelta < 0);
      const worse =
        (goodWhen === "up" && numDelta < 0) || (goodWhen === "down" && numDelta > 0);
      const deltaColor = better
        ? "var(--accent-success)"
        : worse
          ? "var(--accent-error)"
          : "var(--dm-text-secondary)";
      const sign = numDelta > 0 ? "+" : numDelta < 0 ? "-" : "";
      const arrow = numDelta > 0 ? "↑ " : numDelta < 0 ? "↓ " : "";
      const deltaEl = document.createElement("div");
      deltaEl.textContent = `${arrow}${sign}${formatInsightNumber(Math.abs(numDelta))}`;
      style(deltaEl, {
        fontSize: "13px",
        fontWeight: "500",
        color: deltaColor,
        fontFamily: "var(--mono)",
      });
      tile.appendChild(deltaEl);
    }
  }

  if (comp.caption !== undefined) {
    const capText = resolveDynamicString(comp.caption, dataModel, scopeData);
    if (capText.trim()) {
      tile.appendChild(renderTextBlock({ content: capText, variant: "caption" }));
    }
  }

  if (comp.trend !== undefined) {
    const rawTrend = resolveDynamicValue(comp.trend, dataModel, scopeData);
    if (Array.isArray(rawTrend) && rawTrend.length > 0) {
      tile.appendChild(renderA2uiSparkline({ data: rawTrend, tone: comp.status }, dataModel, scopeData));
    }
  }

  return tile;
};

const renderA2uiScene3d = (comp, dataModel, scopeData) => {
  const wrap = document.createElement("div");
  style(wrap, {
    display: "flex",
    flexDirection: "column",
    gap: "10px",
    padding: "18px 20px",
    borderRadius: "20px",
    background: "var(--dm-surface-gray)",
    border: "1px solid var(--dm-outline)",
  });
  const title = comp.title ? resolveDynamicString(comp.title, dataModel, scopeData) : "3D Scene";
  const desc = comp.description ? resolveDynamicString(comp.description, dataModel, scopeData) : "";
  wrap.appendChild(renderTextBlock({ content: title, variant: "h4" }));
  if (desc) wrap.appendChild(renderTextBlock({ content: desc, variant: "caption" }));

  const objects = Array.isArray(comp.objects) ? comp.objects : [];
  if (objects.length) {
    const list = document.createElement("div");
    style(list, { display: "flex", flexDirection: "column", gap: "6px" });
    for (const obj of objects) {
      if (!obj || typeof obj !== "object") continue;
      const row = document.createElement("div");
      style(row, { display: "flex", alignItems: "center", justifyContent: "space-between", gap: "10px", fontSize: "13px" });
      const left = document.createElement("span");
      style(left, { display: "inline-flex", alignItems: "center", gap: "8px", color: "var(--dm-text-primary)" });
      const hex = resolveInsightTokenColor(obj.color);
      if (hex) {
        const dot = document.createElement("span");
        style(dot, { width: "10px", height: "10px", borderRadius: "50%", background: hex, flexShrink: "0" });
        left.appendChild(dot);
      }
      const objLabel = obj.label ? resolveDynamicString(obj.label, dataModel, scopeData) : obj.id;
      left.appendChild(document.createTextNode(`${objLabel || obj.id} (${obj.shape || "box"})`));
      const coords = Array.isArray(obj.position) ? `[${obj.position.join(", ")}]` : "";
      const right = document.createElement("code");
      right.textContent = coords;
      style(right, { fontFamily: "var(--mono)", fontSize: "12px", color: "var(--dm-text-secondary)" });
      row.append(left, right);
      list.appendChild(row);
    }
    wrap.appendChild(list);
  }
  return wrap;
};

export const renderA2uiNode = (idOrComp, runtime, ctx = {}) => {
  if (!runtime || !runtime.componentsById) return document.createTextNode("");
  const { inGrid = false, inModal = false, scopeData = runtime.scopeData ?? null, visited = runtime.visited || new Set() } = ctx;
  const dataModel = runtime.dataModel ?? {};

  let comp = null;
  let compId = null;
  if (typeof idOrComp === "string") {
    compId = idOrComp;
    if (visited.has(compId)) return document.createTextNode("");
    comp = runtime.componentsById.get(compId);
  } else if (idOrComp && typeof idOrComp === "object") {
    comp = idOrComp;
    compId = comp.id || null;
    if (compId && visited.has(compId)) return document.createTextNode("");
  }
  if (!comp || typeof comp !== "object") return document.createTextNode("");

  const nextVisited = new Set(visited);
  if (compId) nextVisited.add(compId);
  const childCtx = { inGrid, inModal, scopeData, visited: nextVisited };

  const applyWeight = (el, childSpec) => {
    const childComp = typeof childSpec === "string" ? runtime.componentsById.get(childSpec) : childSpec;
    if (el && el.style && typeof childComp?.weight === "number") {
      el.style.flex = String(childComp.weight);
    }
    return el;
  };

  switch (comp.component) {
    case "Text": {
      const content = resolveDynamicString(comp.text, dataModel, scopeData);
      return renderTextBlock({ content, variant: comp.variant || "body", inGrid, inModal });
    }

    case "Divider": {
      if (comp.axis === "vertical") {
        const sep = document.createElement("div");
        sep.setAttribute("role", "separator");
        sep.setAttribute("aria-orientation", "vertical");
        style(sep, {
          width: "1px",
          alignSelf: "stretch",
          minHeight: "18px",
          background: "var(--dm-outline)",
        });
        return sep;
      }
      const hr = document.createElement("hr");
      style(hr, {
        border: "0",
        borderTop: "1px solid var(--dm-outline)",
        margin: "6px 0",
        width: "100%",
      });
      return hr;
    }

    case "Row":
    case "Column": {
      const isRow = comp.component === "Row";
      const box = document.createElement("div");
      style(box, {
        display: "flex",
        flexDirection: isRow ? "row" : "column",
        flexWrap: isRow ? "wrap" : "nowrap",
        justifyContent: FLEX_JUSTIFY[comp.justify] || "flex-start",
        alignItems: FLEX_ALIGN[comp.align] || (isRow && comp.justify === "spaceBetween" ? "baseline" : "stretch"),
        gap: "12px",
      });
      for (const child of resolveChildrenSpec(comp.children, dataModel, scopeData)) {
        const node = renderA2uiNode(child.id, runtime, { ...childCtx, inGrid: false, scopeData: child.scopeData });
        box.appendChild(applyWeight(node, child.id));
      }
      return box;
    }

    case "List": {
      const horiz = comp.direction === "horizontal";
      const list = document.createElement("div");
      style(list, {
        display: "flex",
        flexDirection: horiz ? "row" : "column",
        flexWrap: horiz ? "wrap" : "nowrap",
        alignItems: FLEX_ALIGN[comp.align] || "stretch",
        gap: "10px",
      });
      for (const child of resolveChildrenSpec(comp.children, dataModel, scopeData)) {
        const node = renderA2uiNode(child.id, runtime, { ...childCtx, inGrid: false, scopeData: child.scopeData });
        list.appendChild(node);
      }
      return list;
    }

    case "Card": {
      const tile = document.createElement("div");
      tile.setAttribute("data-ui-scope", "true");
      style(tile, {
        display: "flex",
        flexDirection: "column",
        justifyContent: "space-between",
        gap: "10px",
        padding: inGrid ? "18px 20px" : "20px 22px",
        borderRadius: "20px",
        background: "var(--dm-surface-gray)",
        border: inGrid ? "none" : "1px solid var(--dm-outline)",
      });
      if (comp.child) {
        tile.appendChild(renderA2uiNode(comp.child, runtime, { ...childCtx, inGrid: false }));
      }
      return tile;
    }

    case "Tabs": {
      const tabs = (Array.isArray(comp.tabs) ? comp.tabs : []).map((t) => ({
        title: resolveDynamicString(t?.title, dataModel, scopeData),
        child: t?.child,
      }));
      return renderTabsControl({
        tabs,
        renderPanelContent: (tab) =>
          tab.child ? renderA2uiNode(tab.child, runtime, { ...childCtx, inGrid: false }) : null,
      });
    }

    case "TextField": {
      const label = resolveDynamicString(comp.label, dataModel, scopeData);
      const variant = comp.variant || "shortText";
      const checks = Array.isArray(comp.checks) ? comp.checks : [];
      const isRequired = checks.some((c) => c?.condition?.call === "required");
      const hasEmail = checks.some((c) => c?.condition?.call === "email");
      const valSpec = comp.value !== undefined ? comp.value : comp.text;
      const path = valSpec && typeof valSpec === "object" ? valSpec.path : null;
      const initialVal = resolveDynamicValue(valSpec, dataModel, scopeData);

      const { field, errEl } = createA2uiFieldWrapper(label, { isRequired });
      let control;
      if (variant === "longText") {
        control = document.createElement("textarea");
        control.rows = 3;
        control.value = initialVal !== null && initialVal !== undefined ? String(initialVal) : "";
      } else {
        control = document.createElement("input");
        control.type = variant === "number" ? "number" : hasEmail ? "email" : "text";
        if (variant === "number") control.step = "any";
        if (comp.validationRegexp) control.pattern = String(comp.validationRegexp);
        control.value = initialVal !== null && initialVal !== undefined ? String(initialVal) : "";
        control.addEventListener("keydown", (ev) => {
          if (ev.key === "Enter") {
            ev.preventDefault();
            const scope = control.closest('[data-ui-scope="true"]');
            const submitBtn = scope?.querySelector('button[data-ui-primary="true"]');
            if (submitBtn && !submitBtn.disabled) submitBtn.click();
          }
        });
      }
      if (isRequired) {
        control.required = true;
        control.setAttribute("aria-required", "true");
      }
      control.setAttribute("data-ui-label", label || comp.id || "Field");
      if (path) control.setAttribute("data-ui-field", path.replace(/^\//, ""));
      styleInputBox(control, variant);
      field.append(control, errEl);

      const getValue = () => {
        if (variant === "number") {
          const trimmed = control.value.trim();
          if (!trimmed) return "";
          const n = Number(trimmed);
          return Number.isFinite(n) ? n : control.value;
        }
        return control.value;
      };
      const { syncModel } = bindA2uiField(runtime, {
        comp,
        control,
        errEl,
        path,
        label: label || comp.id || "Field",
        getValue,
      });
      control.addEventListener("input", syncModel);
      control.addEventListener("change", syncModel);
      return field;
    }

    case "CheckBox": {
      const label = resolveDynamicString(comp.label, dataModel, scopeData);
      const checks = Array.isArray(comp.checks) ? comp.checks : [];
      const isRequired = checks.some((c) => c?.condition?.call === "required");
      const path = comp.value && typeof comp.value === "object" ? comp.value.path : null;
      const initialVal = resolveDynamicValue(comp.value, dataModel, scopeData);

      const field = document.createElement("label");
      style(field, { display: "flex", flexDirection: "column", gap: "4px", fontSize: "13.5px" });
      const row = document.createElement("div");
      style(row, { display: "flex", alignItems: "center", gap: "10px", padding: "4px 0" });
      const control = document.createElement("input");
      control.type = "checkbox";
      control.checked = Boolean(initialVal);
      if (isRequired) {
        control.required = true;
        control.setAttribute("aria-required", "true");
      }
      const lbl = document.createElement("span");
      lbl.textContent = `${label}${isRequired ? " *" : ""}`;
      style(lbl, { fontWeight: "450", color: "var(--dm-text-secondary)" });
      row.append(control, lbl);

      const errEl = document.createElement("span");
      errEl.className = "ui-field-error hidden";
      style(errEl, { fontSize: "12px", fontWeight: "450", color: "var(--accent-error)" });
      field.append(row, errEl);

      const { syncModel } = bindA2uiField(runtime, {
        comp,
        control,
        errEl,
        path,
        label: label || comp.id || "Checkbox",
        getValue: () => control.checked,
        styledBorder: false,
      });
      control.addEventListener("change", syncModel);
      return field;
    }

    case "Slider": {
      const label = resolveDynamicString(comp.label, dataModel, scopeData);
      const min = Number.isFinite(Number(comp.min)) ? Number(comp.min) : 0;
      const max = Number.isFinite(Number(comp.max)) ? Number(comp.max) : 100;
      const path = comp.value && typeof comp.value === "object" ? comp.value.path : null;
      const rawInit = resolveDynamicValue(comp.value, dataModel, scopeData);
      const initNum = Number.isFinite(Number(rawInit)) ? Number(rawInit) : min;

      const readout = document.createElement("code");
      readout.textContent = String(initNum);
      style(readout, { fontFamily: "var(--mono)", fontSize: "12.5px", color: "var(--dm-text-primary)" });
      const { field, errEl } = createA2uiFieldWrapper(label, { headerRightEl: readout });

      const control = document.createElement("input");
      control.type = "range";
      control.min = String(min);
      control.max = String(max);
      control.value = String(initNum);
      styleInputBox(control, "slider");
      field.append(control, errEl);

      const { syncModel } = bindA2uiField(runtime, {
        comp,
        control,
        errEl,
        path,
        label: label || comp.id || "Slider",
        getValue: () => Number(control.value),
        styledBorder: false,
      });
      control.addEventListener("input", () => {
        readout.textContent = control.value;
        syncModel();
      });
      return field;
    }

    case "DateTimeInput": {
      const label = resolveDynamicString(comp.label, dataModel, scopeData);
      const checks = Array.isArray(comp.checks) ? comp.checks : [];
      const isRequired = checks.some((c) => c?.condition?.call === "required");
      const enableDate = Boolean(comp.enableDate);
      const enableTime = Boolean(comp.enableTime);
      const inputType =
        enableDate && enableTime ? "datetime-local" : !enableDate && enableTime ? "time" : "date";
      const path = comp.value && typeof comp.value === "object" ? comp.value.path : null;
      const initialVal = resolveDynamicString(comp.value, dataModel, scopeData);

      const { field, errEl } = createA2uiFieldWrapper(label, { isRequired });
      const control = document.createElement("input");
      control.type = inputType;
      if (comp.min !== undefined) {
        const minStr = resolveDynamicString(comp.min, dataModel, scopeData);
        if (minStr) control.min = minStr;
      }
      if (comp.max !== undefined) {
        const maxStr = resolveDynamicString(comp.max, dataModel, scopeData);
        if (maxStr) control.max = maxStr;
      }
      control.value = initialVal;
      if (isRequired) {
        control.required = true;
        control.setAttribute("aria-required", "true");
      }
      styleInputBox(control, inputType);
      field.append(control, errEl);

      const { syncModel } = bindA2uiField(runtime, {
        comp,
        control,
        errEl,
        path,
        label: label || comp.id || "Date/Time",
        getValue: () => control.value,
      });
      control.addEventListener("input", syncModel);
      control.addEventListener("change", syncModel);
      return field;
    }

    case "ChoicePicker": {
      const label = resolveDynamicString(comp.label, dataModel, scopeData);
      const checks = Array.isArray(comp.checks) ? comp.checks : [];
      const isRequired = checks.some((c) => c?.condition?.call === "required");
      const isMulti = comp.variant === "multipleSelection";
      const isChips = comp.displayStyle === "chips";
      const path = comp.value && typeof comp.value === "object" ? comp.value.path : null;
      const rawSelected = resolveDynamicValue(comp.value, dataModel, scopeData);
      const selected = new Set(
        Array.isArray(rawSelected)
          ? rawSelected.map(String)
          : typeof rawSelected === "string" && rawSelected
            ? [rawSelected]
            : []
      );
      const options = (Array.isArray(comp.options) ? comp.options : []).map((opt) => ({
        label: resolveDynamicString(opt?.label, dataModel, scopeData) || String(opt?.value ?? ""),
        value: String(opt?.value ?? ""),
      }));

      const { field, errEl } = createA2uiFieldWrapper(label, { isRequired });
      let focusTarget = null;
      let control = null;
      let syncModel = () => {};

      if (!isMulti && !isChips && !comp.filterable) {
        control = document.createElement("select");
        const ph = document.createElement("option");
        ph.value = "";
        ph.textContent = "Select an option…";
        ph.disabled = isRequired;
        ph.selected = selected.size === 0;
        control.appendChild(ph);
        for (const opt of options) {
          const o = document.createElement("option");
          o.value = opt.value;
          o.textContent = opt.label;
          o.selected = selected.has(opt.value);
          control.appendChild(o);
        }
        if (isRequired) {
          control.required = true;
          control.setAttribute("aria-required", "true");
        }
        styleInputBox(control, "select");
        field.append(control, errEl);
        const bound = bindA2uiField(runtime, {
          comp,
          control,
          errEl,
          path,
          label: label || comp.id || "Choice",
          getValue: () => (control.value ? [control.value] : []),
        });
        syncModel = bound.syncModel;
        control.addEventListener("change", syncModel);
      } else {
        const box = document.createElement("div");
        style(box, { display: "flex", flexDirection: "column", gap: "8px" });
        const listEl = document.createElement("div");
        style(
          listEl,
          isChips
            ? { display: "flex", flexWrap: "wrap", gap: "8px" }
            : {
                display: "flex",
                flexDirection: "column",
                gap: "6px",
                padding: "10px 12px",
                borderRadius: "12px",
                border: "1px solid var(--dm-outline)",
                background: "var(--dm-surface-gray)",
              }
        );

        const renderOptions = (filter = "") => {
          listEl.replaceChildren();
          const q = filter.trim().toLowerCase();
          for (const opt of options) {
            if (q && !opt.label.toLowerCase().includes(q) && !opt.value.toLowerCase().includes(q)) {
              continue;
            }
            const active = selected.has(opt.value);
            if (isChips) {
              const chip = document.createElement("button");
              chip.type = "button";
              chip.textContent = opt.label;
              chip.setAttribute("aria-pressed", active ? "true" : "false");
              style(chip, {
                padding: "7px 14px",
                borderRadius: "999px",
                fontSize: "13px",
                fontWeight: "500",
                cursor: "pointer",
                border: active ? "1px solid var(--dm-text-primary)" : "1px solid var(--dm-outline)",
                background: active ? "var(--primary-btn-bg)" : "var(--dm-surface-gray)",
                color: active ? "var(--primary-btn-fg)" : "var(--dm-text-primary)",
              });
              chip.addEventListener("click", () => {
                if (isMulti) {
                  if (selected.has(opt.value)) selected.delete(opt.value);
                  else selected.add(opt.value);
                } else {
                  selected.clear();
                  selected.add(opt.value);
                }
                renderOptions(q);
                syncModel();
              });
              if (!focusTarget) focusTarget = chip;
              listEl.appendChild(chip);
            } else {
              const row = document.createElement("label");
              style(row, { display: "flex", alignItems: "center", gap: "8px", cursor: "pointer", fontSize: "13.5px" });
              const inp = document.createElement("input");
              inp.type = isMulti ? "checkbox" : "radio";
              if (!isMulti) inp.name = `choice-${comp.id || path || "group"}`;
              inp.value = opt.value;
              inp.checked = active;
              inp.addEventListener("change", () => {
                if (isMulti) {
                  if (inp.checked) selected.add(opt.value);
                  else selected.delete(opt.value);
                } else if (inp.checked) {
                  selected.clear();
                  selected.add(opt.value);
                }
                syncModel();
              });
              if (!focusTarget) focusTarget = inp;
              const txt = document.createElement("span");
              txt.textContent = opt.label;
              style(txt, { color: "var(--dm-text-primary)" });
              row.append(inp, txt);
              listEl.appendChild(row);
            }
          }
        };

        if (comp.filterable) {
          const search = document.createElement("input");
          search.type = "search";
          search.placeholder = "Filter options…";
          styleInputBox(search, "text");
          search.addEventListener("input", () => renderOptions(search.value));
          box.appendChild(search);
        }
        renderOptions("");
        box.appendChild(listEl);
        field.append(box, errEl);

        const bound = bindA2uiField(runtime, {
          comp,
          control: listEl,
          focusTarget,
          errEl,
          path,
          label: label || comp.id || "Choice",
          getValue: () => Array.from(selected),
          styledBorder: !isChips,
        });
        syncModel = bound.syncModel;
      }
      return field;
    }

    case "Button": {
      const btn = document.createElement("button");
      btn.type = "button";
      const eventName = comp.action?.event?.name || "";
      const declaredAction = (runtime.actions || []).find((a) => a.id === eventName) || null;
      const childComp = comp.child ? runtime.componentsById.get(comp.child) : null;
      const childLabel =
        childComp?.component === "Text"
          ? resolveDynamicString(childComp.text, dataModel, scopeData)
          : declaredAction?.label || "Submit";
      btn.textContent = childLabel || "Submit";
      applyA2uiButtonStyles(btn, comp.variant, { inModal });

      btn.addEventListener("click", () => {
        if (isBusy()) {
          showToast({ variant: "warning", message: "Wait for the agent to finish." });
          return;
        }
        const isCancel =
          /^(cancel|dismiss|close|skip|reset)\b/i.test(eventName) ||
          /^(cancel|dismiss|close|skip|reset)\b/i.test(childLabel || "");

        for (const inp of runtime.inputs) {
          inp.syncModel();
        }

        if (runtime.inputs.length > 0 && !isCancel) {
          let firstInvalid = null;
          for (const inp of runtime.inputs) {
            let errMsg = "";
            for (const chk of Array.isArray(inp.comp.checks) ? inp.comp.checks : []) {
              if (!evalA2uiCondition(chk.condition, runtime.dataModel, runtime.dataModel)) {
                errMsg = chk.message || `${inp.label} is invalid.`;
                break;
              }
            }
            if (!errMsg && typeof inp.control?.checkValidity === "function" && !inp.control.checkValidity()) {
              errMsg = inp.control.validationMessage || `${inp.label} is invalid.`;
            }
            if (errMsg) {
              inp.setError(errMsg);
              if (!firstInvalid) firstInvalid = inp;
            }
          }
          if (firstInvalid) {
            firstInvalid.focusTarget?.focus();
            if (typeof firstInvalid.control?.reportValidity === "function") {
              firstInvalid.control.reportValidity();
            }
            return;
          }
        }

        let resolvedArgs = {};
        if (declaredAction) {
          resolvedArgs = resolveActionArgs(declaredAction, runtime.dataModel);
        } else if (comp.action?.event?.context && typeof comp.action.event.context === "object") {
          for (const [k, v] of Object.entries(comp.action.event.context)) {
            resolvedArgs[k] = resolveDynamicValue(v, runtime.dataModel, scopeData);
          }
        }
        const fallbackData = {};
        if (!declaredAction && runtime.inputs.length > 0 && !isCancel) {
          for (const inp of runtime.inputs) {
            const key = inp.path ? inp.path.replace(/^\//, "") : inp.label;
            fallbackData[key] = inp.getValue();
          }
        }
        const msg = formatActionSubmission({
          surfaceId: runtime.surfaceId,
          action: declaredAction,
          actionName: eventName || childLabel || "click",
          resolvedArgs,
          fallbackData,
          hadInputs: runtime.inputs.length > 0 && !isCancel,
        });
        const orig = btn.textContent;
        btn.textContent = `${orig} ✓`;
        setTimeout(() => {
          btn.textContent = orig;
        }, 1800);
        if (btn.closest('[data-ui-modal="true"]')) {
          closeModalOverlay();
        }
        sendMessage(msg);
      });
      return btn;
    }

    // `insight` 1.0 components
    case "Chart":
      return renderA2uiChart(comp, dataModel, scopeData, { skipTitle: Boolean(ctx.skipTitle) });

    case "Sparkline":
      return renderA2uiSparkline(comp, dataModel, scopeData);

    case "Metric":
      return renderA2uiMetric(comp, dataModel, scopeData, { inGrid });

    case "KeyValue": {
      const title = comp.title ? resolveDynamicString(comp.title, dataModel, scopeData) : "";
      let pairs = [];
      if (Array.isArray(comp.items)) {
        pairs = comp.items.map((item) => ({
          key: resolveDynamicString(item?.key, dataModel, scopeData),
          value: formatInsightValue(resolveDynamicValue(item?.value, dataModel, scopeData)),
        }));
      } else if (comp.data !== undefined && comp.keyField && comp.valueField) {
        const rows = resolveDynamicValue(comp.data, dataModel, scopeData);
        if (Array.isArray(rows)) {
          pairs = rows.map((row) => ({
            key: formatInsightValue(row?.[comp.keyField]),
            value: formatInsightValue(row?.[comp.valueField]),
          }));
        }
      }
      return renderKeyValueControl({ title, pairs });
    }

    case "Progress":
      return renderProgressControl({
        label: resolveDynamicString(comp.label, dataModel, scopeData),
        value: resolveDynamicValue(comp.value, dataModel, scopeData),
        tone: comp.tone || "neutral",
        indeterminate: Boolean(comp.indeterminate),
      });

    case "Badge":
      return renderBadgeControl({
        text: resolveDynamicString(comp.text, dataModel, scopeData),
        tone: comp.tone || "neutral",
      });

    // `studio` 1.0 components
    case "App": {
      const wrap = document.createElement("div");
      style(wrap, { display: "flex", flexDirection: "column", gap: "14px" });
      const title = comp.title ? resolveDynamicString(comp.title, dataModel, scopeData) : "";
      if (title && !ctx.skipTitle) {
        wrap.appendChild(renderTextBlock({ content: title, variant: "h3", inModal }));
      }
      const pages = (Array.isArray(comp.pages) ? comp.pages : []).map((p) => ({
        id: p?.id,
        title: resolveDynamicString(p?.title, dataModel, scopeData),
        child: p?.child,
      }));
      let initIdx = 0;
      if (comp.initialPage) {
        const idx = pages.findIndex((p) => p.id === comp.initialPage);
        if (idx >= 0) initIdx = idx;
      }
      wrap.appendChild(
        renderTabsControl({
          label: title || "Pages",
          tabs: pages,
          initialIndex: initIdx,
          vertical: comp.navigation === "side",
          renderPanelContent: (page) =>
            page.child ? renderA2uiNode(page.child, runtime, { ...childCtx, inGrid: false }) : null,
        })
      );
      return wrap;
    }

    case "Page": {
      const wrap = document.createElement("div");
      style(wrap, { display: "flex", flexDirection: "column", gap: "14px" });
      const title = comp.title ? resolveDynamicString(comp.title, dataModel, scopeData) : "";
      const desc = comp.description
        ? resolveDynamicString(comp.description, dataModel, scopeData)
        : "";
      if (title) wrap.appendChild(renderTextBlock({ content: title, variant: "h3", inModal }));
      if (desc) wrap.appendChild(renderTextBlock({ content: desc, variant: "caption", inModal }));
      const layout = comp.layout || "stack";
      const body = buildLayoutContainer(layout, 2);
      for (const cid of Array.isArray(comp.children) ? comp.children : []) {
        body.appendChild(renderA2uiNode(cid, runtime, { ...childCtx, inGrid: layout === "grid" }));
      }
      wrap.appendChild(body);
      return wrap;
    }

    case "Region": {
      const isHero = comp.variant === "hero";
      const vc = comp.tone ? variantColors(comp.tone) : null;
      const wrap = document.createElement("section");
      style(wrap, {
        display: "flex",
        flexDirection: "column",
        gap: "12px",
        padding: isHero || comp.tone ? "18px 20px" : "0",
        borderRadius: isHero || comp.tone ? "20px" : "0",
        background: isHero ? (vc ? vc.bg : "var(--dm-surface-gray)") : comp.tone ? "var(--dm-surface-gray)" : "transparent",
        border: isHero || comp.tone ? `1px solid ${vc?.border || "var(--dm-outline)"}` : "0",
      });
      const title = comp.title ? resolveDynamicString(comp.title, dataModel, scopeData) : "";
      if (title) wrap.appendChild(renderTextBlock({ content: title, variant: "h4", inModal }));
      const layout = comp.layout || "stack";
      const body = buildLayoutContainer(layout, comp.columns || 2);
      for (const cid of Array.isArray(comp.children) ? comp.children : []) {
        body.appendChild(renderA2uiNode(cid, runtime, { ...childCtx, inGrid: layout === "grid" }));
      }
      wrap.appendChild(body);
      return wrap;
    }

    case "Scene3D":
      return renderA2uiScene3d(comp, dataModel, scopeData);

    default: {
      const note = document.createElement("p");
      note.className = "dm-unsupported";
      note.textContent = `Unsupported component: ${comp.component || "unknown"}`;
      style(note, { margin: "0", fontSize: "12.5px", color: "var(--text-muted)" });
      return note;
    }
  }
};
