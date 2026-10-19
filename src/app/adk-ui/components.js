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
import { showToast, style } from "../dom.js";
import { safeImageSrc, variantColors } from "../format.js";
import { state } from "../state.js";
import { appendInlineMarkdown, parseMarkdownBlocksInto } from "../markdown.js";
import { renderChartInner } from "./charts.js";
import { renderTableParts } from "./tables.js";
import { closeModalOverlay } from "./modal.js";

let uiTabsSeq = 0;

// Late binding: `sendMessage` (sse.js) sits above this module (sse.js renders
// through transcript.js → adk-ui), so app.js injects it via `bindSendMessage`.
let sendMessage = null;
export const bindSendMessage = (fn) => {
  sendMessage = fn;
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

// `ctx` is the render context: `inGrid` (KPI tile inside a grid) and
// `inModal` (anywhere inside the modal overlay). It is threaded explicitly
// because children are rendered while their parents are still detached, so
// DOM-based `closest()` sniffing cannot see the modal.
export const renderInnerComponent = (comp, surfaceRoot, ctx = {}) => {
  if (!comp || typeof comp !== "object") return document.createTextNode("");
  const { inGrid = false, inModal = false } = ctx || {};
  const type = String(comp.type || "").toLowerCase();

  switch (type) {
    case "text": {
      const v = String(comp.variant || "body").toLowerCase();
      const raw = String(comp.content ?? "");
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
        el.className = "message-text";
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
    }

    case "badge": {
      const span = document.createElement("span");
      span.textContent = comp.label ?? "";
      const vc = variantColors(comp.variant);
      style(span, {
        display: "inline-flex",
        alignItems: "center",
        padding: "4px 10px",
        borderRadius: "999px",
        fontSize: "12px",
        fontWeight: "500",
        background: vc.bg,
        color: vc.fg,
        border: "1px solid transparent",
        alignSelf: "flex-start",
      });
      return span;
    }

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

    case "tabs": {
      const wrap = document.createElement("div");
      style(wrap, { display: "flex", flexDirection: "column", gap: "14px" });
      const bar = document.createElement("div");
      bar.setAttribute("role", "tablist");
      if (comp.label) bar.setAttribute("aria-label", String(comp.label));
      style(bar, {
        display: "flex",
        gap: "4px",
        borderBottom: "1px solid var(--dm-outline)",
        paddingBottom: "0",
      });
      const panel = document.createElement("div");
      panel.setAttribute("role", "tabpanel");
      panel.tabIndex = 0;
      const tabs = Array.isArray(comp.tabs) ? comp.tabs : [];
      const tabsId = `ui-tabs-${++uiTabsSeq}`;
      let activeIdx = 0;

      const selectTab = (idx, { focus = false } = {}) => {
        activeIdx = idx;
        Array.from(bar.children).forEach((btn, i) => {
          const selected = i === activeIdx;
          btn.setAttribute("aria-selected", selected ? "true" : "false");
          btn.tabIndex = selected ? 0 : -1;
          style(btn, {
            background: "transparent",
            color: selected ? "var(--dm-text-primary)" : "var(--dm-text-secondary)",
            borderBottom: selected ? "2px solid var(--dm-text-primary)" : "2px solid transparent",
          });
          if (selected && focus) btn.focus();
        });
        panel.setAttribute("aria-labelledby", `${tabsId}-tab-${idx}`);
        panel.replaceChildren();
        for (const child of tabs[idx]?.content || []) {
          panel.appendChild(renderInnerComponent(child, surfaceRoot, { inModal }));
        }
      };

      tabs.forEach((tab, idx) => {
        const b = document.createElement("button");
        b.type = "button";
        b.id = `${tabsId}-tab-${idx}`;
        b.setAttribute("role", "tab");
        b.setAttribute("aria-controls", `${tabsId}-panel`);
        b.textContent = tab.label || `Tab ${idx + 1}`;
        style(b, {
          border: "0",
          borderBottom: "2px solid transparent",
          borderRadius: "0",
          padding: "10px 16px",
          marginBottom: "-1px",
          fontSize: "14px",
          fontWeight: "500",
          cursor: "pointer",
        });
        b.addEventListener("click", () => selectTab(idx));
        // Roving tabindex: arrows move between tabs, Home/End jump to the ends.
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
      if (tabs.length) selectTab(0);
      return wrap;
    }

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

    case "key_value": {
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
      for (const pair of comp.pairs || []) {
        const k = document.createElement("div");
        k.textContent = pair.key;
        style(k, { color: "var(--dm-text-secondary)", fontWeight: "450" });
        const v = document.createElement("div");
        v.textContent = pair.value;
        style(v, { color: "var(--dm-text-primary)", fontWeight: "500" });
        dl.append(k, v);
      }
      return dl;
    }

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

    case "progress": {
      const wrap = document.createElement("div");
      style(wrap, { display: "flex", flexDirection: "column", gap: "8px" });
      const pct = Math.max(0, Math.min(100, Number(comp.value) || 0));
      const top = document.createElement("div");
      style(top, { display: "flex", justifyContent: "space-between", fontSize: "13.5px" });
      const lbl = document.createElement("span");
      lbl.textContent = comp.label || "Progress";
      style(lbl, { color: "var(--dm-text-primary)", fontWeight: "450" });
      const val = document.createElement("span");
      val.textContent = `${pct}%`;
      style(val, { color: "var(--dm-text-secondary)", fontFamily: "var(--mono)" });
      top.append(lbl, val);

      const track = document.createElement("div");
      style(track, {
        height: "8px",
        borderRadius: "999px",
        background: "var(--dm-surface-gray)",
        overflow: "hidden",
      });
      const bar = document.createElement("div");
      style(bar, {
        width: `${pct}%`,
        height: "100%",
        borderRadius: "999px",
        background: "var(--dm-text-primary)",
      });
      track.appendChild(bar);
      wrap.append(top, track);
      return wrap;
    }

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
        control.type =
          type === "number_input"
            ? "number"
            : type === "date_input"
              ? "date"
              : type === "slider"
                ? "range"
                : comp.input_type || "text";
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
        if (state.isRunning) {
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
      const span = document.createElement("span");
      span.className = "google-symbols";
      span.setAttribute("aria-hidden", "true");
      const rawName = String(comp.name || "info").toLowerCase().replace(/-/g, "_");
      const iconMap = {
        check: "check_circle",
        alert_triangle: "warning",
        alert_circle: "error",
        x: "close",
      };
      span.textContent = iconMap[rawName] || rawName;
      style(span, {
        fontSize: `${Math.max(14, Number(comp.size) || 18)}px`,
        color: "var(--accent-primary)",
      });
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

    default:
      return document.createTextNode("");
  }
};
