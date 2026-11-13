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

export const inferArtifactKind = (filename, mimeType = "") => {
  const lower = String(filename || "").toLowerCase();
  const mime = String(mimeType || "").toLowerCase();
  if (
    mime.startsWith("image/") ||
    /\.(?:png|jpe?g|webp|gif|svg|avif|bmp)$/i.test(lower)
  ) {
    return { kind: "image", icon: "image", defaultTitle: "Generated Image" };
  }
  if (
    mime.startsWith("video/") ||
    /\.(?:mp4|webm|mov|m4v|ogv)$/i.test(lower)
  ) {
    return { kind: "video", icon: "movie", defaultTitle: "Generated Video" };
  }
  if (
    mime.startsWith("audio/") ||
    /\.(?:mp3|wav|ogg|m4a|flac|aac)$/i.test(lower)
  ) {
    return { kind: "audio", icon: "audio_file", defaultTitle: "Audio Artifact" };
  }
  return { kind: "file", icon: "description", defaultTitle: filename || "Artifact" };
};

export const extractPartArtifact = (part) => {
  if (!part || typeof part !== "object") return null;
  const fn = part.functionResponse || part.function_response || null;
  const toolName = fn?.name || part.name || "";
  const resp = fn?.response ?? part.response ?? null;
  if (!resp || typeof resp !== "object") return null;

  if (
    typeof resp.filename === "string" &&
    resp.filename.trim() !== "" &&
    (!resp.status || String(resp.status).toUpperCase() === "SUCCESS")
  ) {
    const filename = resp.filename.trim();
    const inferred = inferArtifactKind(filename, resp.mime_type || resp.mimeType);
    const title =
      toolName === "generate_image"
        ? "Generated Image"
        : toolName === "generate_video"
          ? "Generated Video"
          : inferred.defaultTitle;
    return {
      filename,
      kind: inferred.kind,
      icon: inferred.icon,
      title,
      toolName,
      message: typeof resp.message === "string" ? resp.message : null,
      mimeType: resp.mime_type || resp.mimeType || null,
    };
  }

  if (typeof resp.saved_to_artifact === "string" && resp.saved_to_artifact.trim() !== "") {
    const filename = resp.saved_to_artifact.trim();
    const inferred = inferArtifactKind(filename, resp.mime_type || resp.mimeType);
    return {
      filename,
      kind: inferred.kind,
      icon: inferred.icon,
      title: filename,
      toolName,
      mimeType: resp.mime_type || resp.mimeType || null,
      bytes: typeof resp.bytes === "number" ? resp.bytes : null,
      preview: typeof resp.preview === "string" ? resp.preview : null,
    };
  }

  return null;
};

const createArtifactHeaderAction = ({ href, label, iconName, download = null }) => {
  const a = document.createElement("a");
  a.href = href;
  if (download) {
    a.download = download;
  } else {
    a.target = "_blank";
    a.rel = "noreferrer";
  }
  a.className = "quiet-button artifact-action-link";
  style(a, {
    display: "inline-flex",
    alignItems: "center",
    gap: "6px",
    padding: "6px 13px",
    borderRadius: "999px",
    border: "1px solid var(--dm-outline)",
    background: "var(--dm-surface-gray)",
    color: "var(--dm-text-primary)",
    fontSize: "12.5px",
    fontWeight: "500",
    textDecoration: "none",
    whiteSpace: "nowrap",
  });
  const icon = document.createElement("span");
  icon.className = "google-symbols";
  icon.setAttribute("aria-hidden", "true");
  icon.textContent = iconName;
  style(icon, { fontSize: "15px" });
  const text = document.createElement("span");
  text.textContent = label;
  a.append(icon, text);
  return a;
};

export const buildArtifactSurfaceCard = (art, url) => {
  const toolbar = document.createElement("div");
  style(toolbar, {
    display: "flex",
    alignItems: "center",
    gap: "8px",
    flexWrap: "wrap",
    flexShrink: "0",
  });
  if (url) {
    toolbar.append(
      createArtifactHeaderAction({
        href: url,
        label: "Open",
        iconName: "open_in_new",
      }),
      createArtifactHeaderAction({
        href: url,
        label: "Download",
        iconName: "download",
        download: art.filename,
      })
    );
  }

  const descParts = [];
  if (art.filename && art.filename !== art.title) descParts.push(art.filename);
  if (art.mimeType) descParts.push(art.mimeType);
  if (typeof art.bytes === "number" && Number.isFinite(art.bytes)) {
    descParts.push(`${(art.bytes / 1024).toFixed(1)} KB`);
  }

  const { card, body } = createStandaloneCard({
    title: art.title || art.filename || "Artifact",
    description: descParts.join(" · ") || null,
    headerRight: toolbar.childNodes.length ? toolbar : null,
  });

  if (art.kind === "image" && url) {
    const frame = document.createElement("div");
    frame.className = "artifact-media-frame";
    style(frame, {
      position: "relative",
      width: "100%",
      borderRadius: "16px",
      overflow: "hidden",
      border: "1px solid var(--dm-outline)",
      background: "var(--dm-surface-gray)",
      display: "flex",
      alignItems: "center",
      justifyContent: "center",
      minHeight: "180px",
    });

    const link = document.createElement("a");
    link.href = url;
    link.target = "_blank";
    link.rel = "noreferrer";
    link.title = `Open ${art.filename} in new tab`;
    style(link, {
      display: "flex",
      alignItems: "center",
      justifyContent: "center",
      width: "100%",
    });

    const img = document.createElement("img");
    img.src = url;
    img.alt = art.filename || art.title || "Generated image";
    img.loading = "lazy";
    img.className = "artifact-media-img";
    style(img, {
      display: "block",
      maxWidth: "100%",
      maxHeight: "640px",
      width: "auto",
      height: "auto",
      objectFit: "contain",
    });

    link.appendChild(img);
    frame.appendChild(link);
    body.appendChild(frame);
  } else if (art.kind === "video" && url) {
    const frame = document.createElement("div");
    frame.className = "artifact-media-frame";
    style(frame, {
      position: "relative",
      width: "100%",
      borderRadius: "16px",
      overflow: "hidden",
      border: "1px solid var(--dm-outline)",
      background: "#000",
      display: "flex",
      alignItems: "center",
      justifyContent: "center",
    });

    const video = document.createElement("video");
    video.src = url;
    video.controls = true;
    video.playsInline = true;
    video.preload = "metadata";
    video.className = "artifact-media-video";
    style(video, {
      display: "block",
      width: "100%",
      maxHeight: "640px",
      objectFit: "contain",
    });

    frame.appendChild(video);
    body.appendChild(frame);
  } else if (art.kind === "audio" && url) {
    const audio = document.createElement("audio");
    audio.src = url;
    audio.controls = true;
    audio.preload = "metadata";
    style(audio, { width: "100%" });
    body.appendChild(audio);
  } else {
    if (art.preview) {
      const pre = document.createElement("pre");
      pre.className = "artifact-file-preview";
      pre.textContent = art.preview;
      style(pre, {
        margin: "0",
        padding: "14px",
        maxHeight: "320px",
        overflow: "auto",
        borderRadius: "14px",
        border: "1px solid var(--dm-outline)",
        background: "var(--dm-surface-gray)",
        color: "var(--dm-text-primary)",
        font: "12px/1.5 var(--mono)",
      });
      body.appendChild(pre);
    }
    if (url) {
      const fileRow = document.createElement("a");
      fileRow.href = url;
      fileRow.target = "_blank";
      fileRow.rel = "noreferrer";
      style(fileRow, {
        display: "flex",
        alignItems: "center",
        justifyContent: "space-between",
        gap: "12px",
        padding: "12px 16px",
        borderRadius: "14px",
        border: "1px solid var(--dm-outline)",
        background: "var(--dm-surface-gray)",
        color: "var(--dm-text-primary)",
        textDecoration: "none",
        fontSize: "13.5px",
        fontWeight: "500",
      });
      const left = document.createElement("span");
      style(left, {
        display: "inline-flex",
        alignItems: "center",
        gap: "8px",
        minWidth: "0",
      });
      const icon = document.createElement("span");
      icon.className = "google-symbols";
      icon.setAttribute("aria-hidden", "true");
      icon.textContent = art.icon || "description";
      const nameSpan = document.createElement("code");
      nameSpan.textContent = art.filename;
      style(nameSpan, {
        fontFamily: "var(--mono)",
        fontSize: "12.5px",
        overflow: "hidden",
        textOverflow: "ellipsis",
        whiteSpace: "nowrap",
      });
      left.append(icon, nameSpan);
      const right = document.createElement("span");
      right.textContent = "Open artifact ↗";
      style(right, {
        fontSize: "12.5px",
        color: "var(--dm-text-secondary)",
        flexShrink: "0",
      });
      fileRow.append(left, right);
      body.appendChild(fileRow);
    }
  }

  return {
    title: art.title || art.filename || "Artifact",
    cards: [card],
  };
};
