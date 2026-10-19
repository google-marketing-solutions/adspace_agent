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

// `adk-ui` `UiResponse` extraction / classification and standalone canvas card assembly.

// Lower-cased component type, matching how `renderInnerComponent` dispatches.
import { style } from "../dom.js";
import { renderChartInner } from "./charts.js";
import { renderTableParts } from "./tables.js";
import { buildAlertParts, renderInnerComponent } from "./components.js";

export const compType = (c) => String(c?.type || "").toLowerCase();

export const isConfirmCard = (comp, toolName) => {
  if (toolName === "render_confirm") return true;
  if (String(comp?.type || "").toLowerCase() !== "card") return false;
  const footer = Array.isArray(comp.footer) ? comp.footer : [];
  const content = Array.isArray(comp.content) ? comp.content : [];
  if (footer.length !== 2 || !content.length) return false;
  const allButtons = footer.every((b) => String(b?.type || "").toLowerCase() === "button");
  const allText = content.every((c) => String(c?.type || "").toLowerCase() === "text");
  const hasCancel = footer.some(
    (b) =>
      String(b?.variant || "").toLowerCase() === "ghost" ||
      /^(cancel|dismiss|no)\b/i.test(String(b?.action_id || "")) ||
      /^(cancel|dismiss|no)\b/i.test(String(b?.label || ""))
  );
  return allButtons && allText && hasCancel;
};

export const classifyUiResponse = (part, uiResp) => {
  const toolName =
    part?.functionResponse?.name || part?.function_response?.name || part?.name || "";
  const comps = Array.isArray(uiResp?.components) ? uiResp.components : [];
  const toasts = comps.filter((c) => String(c?.type || "").toLowerCase() === "toast");
  const rest = comps.filter((c) => String(c?.type || "").toLowerCase() !== "toast");

  if (toolName === "render_toast" || (toasts.length > 0 && rest.length === 0)) {
    return { kind: "toast", toolName, toasts };
  }
  if (
    toolName === "render_modal" ||
    (rest.length === 1 && String(rest[0]?.type || "").toLowerCase() === "modal")
  ) {
    return { kind: "modal", toolName, modal: rest[0], toasts };
  }
  if (rest.length === 1 && isConfirmCard(rest[0], toolName)) {
    return { kind: "confirm", toolName, confirmCard: rest[0], toasts };
  }
  const cleanResp = rest.length === comps.length ? uiResp : { ...uiResp, components: rest };
  return { kind: "surface", toolName, uiResp: cleanResp, toasts };
};

export const buildStandaloneCards = (uiResp, toolName = "") => {
  const comps = Array.isArray(uiResp.components) ? uiResp.components : [];
  const cards = [];

  if (
    (comps.length === 2 && compType(comps[0]) === "text" && compType(comps[1]) === "table") ||
    (comps.length === 1 && compType(comps[0]) === "table")
  ) {
    const title = comps.length === 2 ? comps[0].content : comps[0].title || "Data Table";
    const tableComp = comps.length === 2 ? comps[1] : comps[0];
    const { element, toolbar } = renderTableParts(tableComp);
    const { card, body } = createStandaloneCard({
      title,
      headerRight: toolbar,
      flushBody: true,
    });
    body.appendChild(element);
    cards.push(card);
    return { title, cards };
  }

  if (comps.length === 1 && compType(comps[0]) === "chart") {
    const chartComp = comps[0];
    const title = chartComp.title || "Chart";
    const { card, body } = createStandaloneCard({ title });
    body.appendChild(renderChartInner(chartComp));
    cards.push(card);
    return { title, cards };
  }

  if (comps.length === 1 && String(comps[0]?.type || "").toLowerCase() === "alert") {
    const { card, title } = createAlertCard(comps[0]);
    cards.push(card);
    return { title, cards };
  }

  let surfaceTitle = null;
  let pendingTitle = null;
  let pendingSubtitle = null;

  for (let i = 0; i < comps.length; i++) {
    const comp = comps[i];
    if (!comp || typeof comp !== "object") continue;
    const t = String(comp.type || "").toLowerCase();

    if (t === "text" && (comp.variant === "h1" || comp.variant === "h2") && !pendingTitle) {
      pendingTitle = comp.content;
      if (!surfaceTitle) surfaceTitle = comp.content;
      continue;
    }
    if (t === "text" && comp.variant === "caption" && pendingTitle && !pendingSubtitle) {
      pendingSubtitle = comp.content;
      continue;
    }

    if (
      t === "stack" &&
      Array.isArray(comp.children) &&
      comp.children.length === 2 &&
      compType(comp.children[0]) === "text" &&
      compType(comp.children[1]) === "grid"
    ) {
      const secTitle = comp.children[0].content;
      if (!surfaceTitle) surfaceTitle = secTitle;
      const { card, body } = createStandaloneCard({
        title: secTitle,
        description: pendingTitle
          ? `${pendingTitle}${pendingSubtitle ? ` · ${pendingSubtitle}` : ""}`
          : null,
      });
      pendingTitle = null;
      pendingSubtitle = null;
      body.appendChild(renderInnerComponent(comp.children[1], card, { inGrid: true }));
      cards.push(card);
      continue;
    }

    if (t === "card" || t === "modal") {
      let contentArr = Array.isArray(comp.content) ? [...comp.content] : [];
      const singleTable = contentArr.length === 1 && compType(contentArr[0]) === "table";
      const singleChart = contentArr.length === 1 && compType(contentArr[0]) === "chart";
      const isProgress =
        toolName === "render_progress" ||
        (!comp.title && contentArr.some((c) => String(c?.type || "").toLowerCase() === "progress"));

      let liftedTitle = null;
      let liftedDesc = null;
      if (isProgress && !comp.title && compType(contentArr[0]) === "text") {
        liftedTitle = contentArr.shift().content;
        if (compType(contentArr[0]) === "text" && String(contentArr[0].variant || "").toLowerCase() === "caption") {
          liftedDesc = contentArr.shift().content;
        }
      }

      let headerRight = null;
      let tableElement = null;
      if (singleTable) {
        const parts = renderTableParts(contentArr[0]);
        headerRight = parts.toolbar;
        tableElement = parts.element;
      }

      const cardTitle = comp.title || liftedTitle || pendingTitle || "Details";
      if (!surfaceTitle) surfaceTitle = cardTitle;

      const { card, body } = createStandaloneCard({
        title: cardTitle,
        description:
          comp.description ||
          liftedDesc ||
          (comp.title && pendingTitle ? pendingSubtitle : null),
        headerRight,
        flushBody: singleTable,
      });
      pendingTitle = null;
      pendingSubtitle = null;

      if (singleTable && tableElement) {
        body.appendChild(tableElement);
      } else if (singleChart) {
        // The card header shows the title; the chart only uses it for the
        // SVG's accessible name, so fall back to the card's.
        body.appendChild(
          renderChartInner({ ...contentArr[0], title: contentArr[0].title || comp.title })
        );
      } else {
        for (const child of contentArr) {
          body.appendChild(renderInnerComponent(child, card));
        }
      }

      if (Array.isArray(comp.footer) && comp.footer.length) {
        const ftr = document.createElement("footer");
        style(ftr, {
          display: "flex",
          flexWrap: "wrap",
          gap: "10px",
          justifyContent: "flex-end",
          padding: "4px 24px 22px",
          background: "transparent",
        });
        for (const child of comp.footer) {
          ftr.appendChild(renderInnerComponent(child, card));
        }
        card.appendChild(ftr);
      }

      cards.push(card);
      continue;
    }

    if (t === "alert") {
      const { card, title } = createAlertCard(comp);
      if (!surfaceTitle) surfaceTitle = title;
      pendingTitle = null;
      pendingSubtitle = null;
      cards.push(card);
      continue;
    }

    const fallbackTitle = comp.title || pendingTitle || "Component";
    if (!surfaceTitle) surfaceTitle = fallbackTitle;
    const { card, body } = createStandaloneCard({
      title: fallbackTitle,
      description: pendingSubtitle || null,
    });
    pendingTitle = null;
    pendingSubtitle = null;
    body.appendChild(renderInnerComponent(comp, card));
    cards.push(card);
  }

  return {
    title: surfaceTitle || "Dashboard",
    cards,
  };
};

export const extractUiResponse = (obj) => {
  if (!obj || typeof obj !== "object") return null;
  if (
    Array.isArray(obj.components) &&
    obj.components.length > 0 &&
    typeof obj.components[0]?.type === "string"
  ) {
    return obj;
  }
  const candidates = [
    obj.functionResponse?.response,
    obj.function_response?.response,
    obj.response,
    obj.result,
  ];
  for (const c of candidates) {
    if (
      c &&
      typeof c === "object" &&
      Array.isArray(c.components) &&
      c.components.length > 0 &&
      typeof c.components[0]?.type === "string"
    ) {
      return c;
    }
  }
  return null;
};

export const createAlertCard = (alertComp) => {
  const card = document.createElement("section");
  card.className = "adk-ui-surface adk-ui-card dm-alert-card";
  card.setAttribute("data-ui-scope", "true");
  const { content, statusBtn, titleText } = buildAlertParts(alertComp);
  card.append(content, statusBtn);
  return { card, title: titleText };
};

export const createStandaloneCard = ({ title, description, headerRight, flushBody = false }) => {
  const card = document.createElement("section");
  card.className = "adk-ui-surface adk-ui-card";
  card.setAttribute("data-ui-scope", "true");
  style(card, {
    width: "100%",
    border: "1px solid var(--dm-outline)",
    borderRadius: "24px",
    background: "var(--surface-card)",
    boxShadow:
      "0 12px 32px -12px rgba(18, 19, 23, 0.08), 0 2px 8px -2px rgba(18, 19, 23, 0.04)",
    overflow: "hidden",
    display: "flex",
    flexDirection: "column",
  });

  if (title || description || headerRight) {
    const hdr = document.createElement("header");
    style(hdr, {
      display: "flex",
      alignItems: "flex-start",
      justifyContent: "space-between",
      gap: "16px",
      padding: flushBody ? "20px 24px 16px" : "22px 24px 6px",
      borderBottom: flushBody ? "1px solid var(--dm-outline)" : "0",
      background: "transparent",
    });

    const left = document.createElement("div");
    style(left, {
      display: "flex",
      flexDirection: "column",
      alignItems: "flex-start",
      gap: "4px",
      minWidth: "0",
    });

    if (title) {
      const strong = document.createElement("strong");
      strong.textContent = title;
      style(strong, {
        fontSize: "20px",
        fontWeight: "450",
        lineHeight: "1.22",
        letterSpacing: "-0.008em",
        color: "var(--dm-text-primary)",
        textWrap: "pretty",
      });
      left.appendChild(strong);
    }

    if (description) {
      const sub = document.createElement("span");
      sub.textContent = description;
      style(sub, {
        fontSize: "14.5px",
        fontWeight: "400",
        lineHeight: "1.45",
        color: "var(--dm-text-secondary)",
      });
      left.appendChild(sub);
    }

    hdr.appendChild(left);
    if (headerRight) hdr.appendChild(headerRight);
    card.appendChild(hdr);
  }

  const body = document.createElement("div");
  style(body, {
    padding: flushBody ? "0" : "10px 24px 24px",
    display: "flex",
    flexDirection: "column",
    gap: "16px",
  });
  card.appendChild(body);

  return { card, body };
};
