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

// Pure A2UI v0.9.1 / `DrawToolset` / `adk-ui` 3.0 wire decoder, JSON pointer
// data-model evaluator, validation helper, and surface reconstructor.
// No DOM, no app state.

export const DRAW_TOOL_NAMES = new Set([
  "render_form",
  "render_card",
  "render_table",
  "render_screen",
  "patch_surface",
  "delete_surface",
]);

export const CHART_PALETTE = [
  "#4b90ff",
  "#6b7cff",
  "#9b72cb",
  "#d96570",
  "#38bdf8",
  "#34d399",
  "#fbbf24",
  "#f472b6",
];

const INSIGHT_TOKEN_COLORS = {
  chart1: CHART_PALETTE[0],
  chart2: CHART_PALETTE[1],
  chart3: CHART_PALETTE[2],
  chart4: CHART_PALETTE[3],
  chart5: CHART_PALETTE[4],
  chart6: CHART_PALETTE[5],
  chart7: CHART_PALETTE[6],
  chart8: CHART_PALETTE[7],
  ink: "var(--dm-text-primary)",
  neutral: "var(--dm-text-secondary)",
  muted: "var(--text-muted)",
  ok: "var(--accent-success)",
  success: "var(--accent-success)",
  warning: "var(--accent-warning)",
  critical: "var(--accent-error)",
  danger: "var(--accent-error)",
  info: "var(--accent-primary)",
};

export const resolveInsightTokenColor = (token) => {
  if (typeof token !== "string") return null;
  const trimmed = token.trim();
  if (!trimmed) return null;
  const named = INSIGHT_TOKEN_COLORS[trimmed.toLowerCase()];
  if (named) return named;
  if (/^#[0-9a-f]{6}$/i.test(trimmed) || /^#[0-9a-f]{3}$/i.test(trimmed)) {
    return trimmed;
  }
  return null;
};

export const isUiToolName = (name) => {
  if (typeof name !== "string" || !name) return false;
  return DRAW_TOOL_NAMES.has(name) || name === "render_band_chart" || name.startsWith("render_");
};

export const extractToolCallName = (part) => {
  if (!part || typeof part !== "object") return "";
  if ("functionCall" in part || "function_call" in part) {
    return part.functionCall?.name || part.function_call?.name || part.name || "";
  }
  if (!("functionResponse" in part) && !("function_response" in part) && typeof part.name === "string") {
    return part.name;
  }
  return "";
};

export const extractDrawnEnvelope = (obj) => {
  if (!obj || typeof obj !== "object") return null;
  const toolName =
    obj.functionResponse?.name || obj.function_response?.name || obj.name || "";
  const candidates = [
    obj,
    obj.functionResponse?.response,
    obj.function_response?.response,
    obj.response,
    obj.result,
  ];
  for (const cand of candidates) {
    if (!cand || typeof cand !== "object") continue;
    if (cand.drawn && typeof cand.drawn === "object" && Array.isArray(cand.drawn.messages)) {
      return { drawn: cand.drawn, response: cand, toolName };
    }
    if (typeof cand.surfaceId === "string" && Array.isArray(cand.messages)) {
      return { drawn: cand, response: obj, toolName };
    }
  }
  return null;
};

export const extractLegacyComponentsResponse = (obj) => {
  if (!obj || typeof obj !== "object") return null;
  const candidates = [
    obj,
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

export const cloneJson = (val) => {
  if (val === null || val === undefined || typeof val !== "object") return val;
  if (typeof structuredClone === "function") return structuredClone(val);
  return JSON.parse(JSON.stringify(val));
};

export const decodePointerTokens = (pointer) => {
  const raw = String(pointer ?? "");
  const trimmed = raw.startsWith("/") ? raw.slice(1) : raw;
  if (!trimmed) return [];
  return trimmed.split("/").map((t) => t.replace(/~1/g, "/").replace(/~0/g, "~"));
};

export const resolvePointer = (dataModel, scopeData, rawPath) => {
  const path = String(rawPath ?? "");
  if (path === "" || path === "/") return dataModel;
  const start = path.startsWith("/") ? dataModel : scopeData ?? dataModel;
  if (start === null || start === undefined || typeof start !== "object") return undefined;
  const tokens = decodePointerTokens(path);
  let cur = start;
  for (const token of tokens) {
    if (cur === null || cur === undefined || typeof cur !== "object") return undefined;
    if (Array.isArray(cur)) {
      const idx = Number(token);
      if (!Number.isInteger(idx) || idx < 0 || idx >= cur.length) return undefined;
      cur = cur[idx];
    } else if (Object.prototype.hasOwnProperty.call(cur, token)) {
      cur = cur[token];
    } else {
      return undefined;
    }
  }
  return cur;
};

// Applies a JSON pointer patch (`/` replaces root; `null`/`undefined` deletes
// target) in-place on `targetModel` (returns the new root model).
export const applyPointerPatch = (targetModel, path, value) => {
  const deleteMode = value === undefined || value === null;
  const p = path === undefined || path === null || path === "" ? "/" : String(path);
  if (p === "/") {
    return deleteMode ? {} : cloneJson(value);
  }
  if (!p.startsWith("/")) return targetModel;

  const tokens = decodePointerTokens(p);
  if (!tokens.length) return targetModel;
  const last = tokens.pop();

  let root = targetModel;
  if (!deleteMode && (root === null || root === undefined || typeof root !== "object")) {
    root = {};
  }
  if (root === null || root === undefined || typeof root !== "object") return root;

  let node = root;
  for (const token of tokens) {
    if (Array.isArray(node)) {
      const idx = Number(token);
      if (!Number.isInteger(idx) || idx < 0 || idx >= node.length) return root;
      node = node[idx];
    } else if (node && typeof node === "object") {
      if (!(token in node) || typeof node[token] !== "object" || node[token] === null) {
        if (deleteMode) return root;
        node[token] = {};
      }
      node = node[token];
    } else {
      return root;
    }
  }

  if (Array.isArray(node)) {
    const idx = Number(last);
    if (!Number.isInteger(idx) || idx < 0) return root;
    if (deleteMode) {
      if (idx < node.length) node.splice(idx, 1);
    } else if (idx < node.length) {
      node[idx] = cloneJson(value);
    } else if (idx === node.length) {
      node.push(cloneJson(value));
    }
  } else if (node && typeof node === "object") {
    if (deleteMode) {
      delete node[last];
    } else {
      node[last] = cloneJson(value);
    }
  }
  return root;
};

export const formatBoundValue = (v) => {
  if (v === null || v === undefined) return "";
  if (Array.isArray(v)) return v.join(", ");
  if (typeof v === "object") return JSON.stringify(v);
  return String(v);
};

export const resolveDynamicValue = (val, dataModel, scopeData = null, depth = 0) => {
  if (depth > 12 || val === null || val === undefined) return val;
  if (typeof val === "object" && !Array.isArray(val)) {
    if (typeof val.path === "string") {
      return resolvePointer(dataModel, scopeData, val.path);
    }
    if (typeof val.call === "string") {
      return evalA2uiFunction(val, dataModel, scopeData, depth + 1);
    }
  }
  return val;
};

export const resolveDynamicString = (val, dataModel, scopeData = null, depth = 0) =>
  formatBoundValue(resolveDynamicValue(val, dataModel, scopeData, depth));

const interpolateFormatString = (template, dataModel, scopeData) => {
  let out = "";
  let rest = String(template ?? "");
  while (rest.length > 0) {
    const start = rest.indexOf("${");
    if (start === -1) {
      out += rest;
      break;
    }
    if (start > 0 && rest[start - 1] === "\\") {
      out += rest.slice(0, start - 1) + "${";
      rest = rest.slice(start + 2);
      continue;
    }
    out += rest.slice(0, start);
    const after = rest.slice(start + 2);
    const end = after.indexOf("}");
    if (end === -1) {
      out += "${" + after;
      break;
    }
    const expr = after.slice(0, end).trim();
    if (expr && !expr.includes("${") && !expr.includes("(")) {
      out += formatBoundValue(resolvePointer(dataModel, scopeData, expr));
    }
    rest = after.slice(end + 1);
  }
  return out;
};

const isBlankString = (v) => v === null || v === undefined || (typeof v === "string" && v.trim() === "");

export const evalA2uiFunction = (fnCall, dataModel, scopeData = null, depth = 0) => {
  if (!fnCall || typeof fnCall !== "object" || depth > 12) return "";
  const call = String(fnCall.call || "");
  const args = fnCall.args && typeof fnCall.args === "object" ? fnCall.args : {};
  const resolveArg = (k) => resolveDynamicValue(args[k], dataModel, scopeData, depth + 1);

  switch (call) {
    case "formatString": {
      const tmpl = resolveDynamicString(args.value, dataModel, scopeData, depth + 1);
      return interpolateFormatString(tmpl, dataModel, scopeData);
    }
    case "pluralize": {
      const countRaw = resolveArg("value");
      const count = typeof countRaw === "number" ? countRaw : Number(countRaw);
      let branchKey = "other";
      if (Number.isFinite(count)) {
        if (count === 0 && args.zero !== undefined) branchKey = "zero";
        else if (count === 1 && args.one !== undefined) branchKey = "one";
        else if (count === 2 && args.two !== undefined) branchKey = "two";
      }
      const chosen =
        args[branchKey] ?? args.other ?? args.one ?? args.zero ?? args.two ?? args.few ?? args.many;
      return resolveDynamicString(chosen, dataModel, scopeData, depth + 1);
    }
    case "required": {
      const v = resolveArg("value");
      if (v === null || v === undefined) return false;
      if (typeof v === "string") return v.trim().length > 0;
      if (Array.isArray(v)) return v.length > 0;
      if (typeof v === "boolean") return v;
      return true;
    }
    case "email": {
      const raw = formatBoundValue(resolveArg("value"));
      if (!raw) return true;
      if (/\s/.test(raw)) return false;
      const at = raw.indexOf("@");
      if (at <= 0) return false;
      const domain = raw.slice(at + 1);
      return (
        domain.includes(".") &&
        !domain.startsWith(".") &&
        !domain.endsWith(".") &&
        !domain.includes("@")
      );
    }
    case "regex": {
      const raw = formatBoundValue(resolveArg("value"));
      const pattern = args.pattern;
      if (!raw || typeof pattern !== "string") return true;
      try {
        return new RegExp(`^(?:${pattern})$`).test(raw);
      } catch {
        return true;
      }
    }
    case "length": {
      const raw = formatBoundValue(resolveArg("value"));
      const len = Array.from(raw).length;
      if (typeof args.min === "number" && len < args.min) return false;
      if (typeof args.max === "number" && len > args.max) return false;
      return true;
    }
    case "numeric": {
      const v = resolveArg("value");
      if (isBlankString(v)) return true;
      const num = typeof v === "number" ? v : Number(String(v).trim());
      if (!Number.isFinite(num)) return false;
      if (typeof args.min === "number" && num < args.min) return false;
      if (typeof args.max === "number" && num > args.max) return false;
      return true;
    }
    case "and":
      return (Array.isArray(args.values) ? args.values : []).every((item) =>
        evalA2uiCondition(item, dataModel, scopeData, depth + 1)
      );
    case "or":
      return (Array.isArray(args.values) ? args.values : []).some((item) =>
        evalA2uiCondition(item, dataModel, scopeData, depth + 1)
      );
    case "not":
      return !evalA2uiCondition(args.value, dataModel, scopeData, depth + 1);
    default:
      return true;
  }
};

export const evalA2uiCondition = (condition, dataModel, scopeData = null, depth = 0) => {
  if (depth > 12) return true;
  if (typeof condition === "boolean") return condition;
  if (condition && typeof condition === "object") {
    if (typeof condition.call === "string") {
      return Boolean(evalA2uiFunction(condition, dataModel, scopeData, depth + 1));
    }
    if (typeof condition.path === "string") {
      return Boolean(resolvePointer(dataModel, scopeData, condition.path));
    }
  }
  return Boolean(condition);
};

export const foldA2uiMessages = (messages, prevState = null) => {
  let state = prevState
    ? {
        surfaceId: prevState.surfaceId,
        catalogId: prevState.catalogId,
        componentsById: new Map(prevState.componentsById),
        componentOrder: [...prevState.componentOrder],
        dataModel: cloneJson(prevState.dataModel ?? {}),
        deleted: Boolean(prevState.deleted),
      }
    : {
        surfaceId: "",
        catalogId: "",
        componentsById: new Map(),
        componentOrder: [],
        dataModel: {},
        deleted: false,
      };

  for (const msg of Array.isArray(messages) ? messages : []) {
    if (!msg || typeof msg !== "object") continue;
    if (msg.createSurface && typeof msg.createSurface === "object") {
      state = {
        surfaceId: msg.createSurface.surfaceId || state.surfaceId,
        catalogId: msg.createSurface.catalogId || state.catalogId,
        componentsById: new Map(),
        componentOrder: [],
        dataModel: {},
        deleted: false,
      };
    } else if (msg.updateComponents && typeof msg.updateComponents === "object") {
      if (msg.updateComponents.surfaceId) state.surfaceId = msg.updateComponents.surfaceId;
      for (const comp of Array.isArray(msg.updateComponents.components)
        ? msg.updateComponents.components
        : []) {
        if (!comp || typeof comp !== "object" || !comp.id) continue;
        if (!state.componentsById.has(comp.id)) {
          state.componentOrder.push(comp.id);
        }
        state.componentsById.set(comp.id, cloneJson(comp));
      }
    } else if (msg.updateDataModel && typeof msg.updateDataModel === "object") {
      if (msg.updateDataModel.surfaceId) state.surfaceId = msg.updateDataModel.surfaceId;
      state.dataModel = applyPointerPatch(
        state.dataModel,
        msg.updateDataModel.path,
        msg.updateDataModel.value
      );
    } else if (msg.deleteSurface && typeof msg.deleteSurface === "object") {
      if (msg.deleteSurface.surfaceId) state.surfaceId = msg.deleteSurface.surfaceId;
      state.componentsById = new Map();
      state.componentOrder = [];
      state.dataModel = {};
      state.deleted = true;
    }
  }
  return state;
};

// Reconstructs live surface records across `events` in chronological order.
// - `DrawToolset` surfaces fold across `createSurface` / `updateComponents` /
//   `updateDataModel` / `deleteSurface` keyed by `drawn.surfaceId`, updating in
//   place and removing deleted surfaces cleanly.
// - Legacy `{ components }` (`render_band_chart` and historical v2 events) are
//   addressed by `surface-${ev.id || idx}-${slotIndex}` as before.
export const reconstructSurfacesFromEvents = (events) => {
  const orderedIds = [];
  const entriesById = new Map();
  const partToSurfaceId = new Map();

  (Array.isArray(events) ? events : []).forEach((ev, evIdx) => {
    const parts = Array.isArray(ev?.content?.parts) ? ev.content.parts : [];
    let legacySlot = 0;

    parts.forEach((part, partIdx) => {
      const partKey = `${evIdx}:${partIdx}`;
      const drawnHit = extractDrawnEnvelope(part);
      if (drawnHit) {
        const { drawn, response, toolName } = drawnHit;
        const surfaceId = drawn.surfaceId || `surface-${ev?.id || evIdx}-${partIdx}`;
        const prevEntry = entriesById.get(surfaceId) || null;
        const folded = foldA2uiMessages(drawn.messages, prevEntry?.folded || null);
        const deleted = Boolean(drawn.deleted || folded.deleted);
        if (drawn.catalogId && !folded.catalogId) folded.catalogId = drawn.catalogId;
        if (!folded.surfaceId) folded.surfaceId = surfaceId;

        if (!entriesById.has(surfaceId)) {
          orderedIds.push(surfaceId);
        }
        const entry = {
          id: surfaceId,
          kind: "a2ui",
          toolName: toolName || prevEntry?.toolName || "",
          version: Number(drawn.version) || (prevEntry ? prevEntry.version + 1 : 1),
          title: drawn.title || prevEntry?.title || null,
          drawnKind: drawn.kind || prevEntry?.drawnKind || "screen",
          actions: Array.isArray(drawn.actions) ? drawn.actions : prevEntry?.actions || [],
          folded,
          deleted,
          raw: response,
          ev,
          renderSig: JSON.stringify({
            id: surfaceId,
            version: drawn.version ?? null,
            deleted,
            title: drawn.title ?? null,
            catalogId: folded.catalogId,
            components: Array.from(folded.componentsById.values()),
            dataModel: folded.dataModel,
            actions: drawn.actions ?? [],
          }),
        };
        entriesById.set(surfaceId, entry);
        partToSurfaceId.set(partKey, surfaceId);
        return;
      }

      const legacy = extractLegacyComponentsResponse(part);
      if (legacy) {
        const slotIndex = legacySlot++;
        const surfaceId = `surface-${ev?.id || evIdx}-${slotIndex}`;
        if (!entriesById.has(surfaceId)) {
          orderedIds.push(surfaceId);
        }
        entriesById.set(surfaceId, {
          id: surfaceId,
          kind: "legacy",
          toolName:
            part?.functionResponse?.name || part?.function_response?.name || part?.name || "",
          legacyResp: legacy,
          deleted: false,
          raw: legacy,
          ev,
          renderSig: JSON.stringify(legacy),
        });
        partToSurfaceId.set(partKey, surfaceId);
      }
    });
  });

  const liveEntries = orderedIds
    .map((id) => entriesById.get(id))
    .filter((e) => e && !e.deleted);

  return { liveEntries, entriesById, partToSurfaceId };
};

export const detectTableSurface = (folded, meta = {}) => {
  if (!folded || !folded.componentsById) return null;
  const byId = folded.componentsById;

  // Locate a `List` component bound to a template child component over a data array.
  let listComp = null;
  for (const comp of byId.values()) {
    if (
      comp.component === "List" &&
      comp.children &&
      !Array.isArray(comp.children) &&
      typeof comp.children.componentId === "string" &&
      typeof comp.children.path === "string"
    ) {
      listComp = comp;
      break;
    }
  }
  if (!listComp && meta.drawnKind !== "table") return null;

  const rawRows = listComp
    ? resolvePointer(folded.dataModel, folded.dataModel, listComp.children.path)
    : folded.dataModel?.rows;
  if (!Array.isArray(rawRows)) return null;

  const titleComp = byId.get("title");
  const titleText =
    meta.title ||
    (titleComp ? resolveDynamicString(titleComp.text, folded.dataModel) : "") ||
    "Table";

  // Extract column headings from a `header` Row component if present.
  const headerComp = byId.get("header");
  const headerLabels = [];
  if (headerComp && headerComp.component === "Row" && Array.isArray(headerComp.children)) {
    for (const hid of headerComp.children) {
      const cell = byId.get(hid);
      headerLabels.push(cell ? resolveDynamicString(cell.text, folded.dataModel) : String(hid));
    }
  }

  // Inspect template Row (if any) to map each cell's relative data path.
  const tplComp = listComp ? byId.get(listComp.children.componentId) : null;
  const cellPaths = [];
  if (tplComp && tplComp.component === "Row" && Array.isArray(tplComp.children)) {
    for (const cid of tplComp.children) {
      const cell = byId.get(cid);
      const p = cell?.text && typeof cell.text === "object" ? cell.text.path : null;
      cellPaths.push(typeof p === "string" ? p : null);
    }
  }

  if (meta.drawnKind !== "table" && (!headerLabels.length || !tplComp)) {
    return null;
  }

  const colCount = Math.max(
    headerLabels.length,
    cellPaths.length,
    Array.isArray(rawRows[0]) ? rawRows[0].length : 0
  );
  if (colCount === 0 && rawRows.length && typeof rawRows[0] === "object" && !Array.isArray(rawRows[0])) {
    const keys = Object.keys(rawRows[0]);
    return {
      title: titleText,
      columns: keys.map((k) => ({ header: k, accessor_key: k, sortable: true })),
      data: rawRows,
    };
  }

  const columns = [];
  for (let i = 0; i < colCount; i++) {
    columns.push({
      header: headerLabels[i] ?? `Col ${i + 1}`,
      accessor_key: `c${i}`,
      sortable: true,
    });
  }

  const data = rawRows.map((row) => {
    const obj = {};
    for (let i = 0; i < colCount; i++) {
      const path = cellPaths[i];
      if (Array.isArray(row)) {
        const idx = path !== null && /^\d+$/.test(path) ? Number(path) : i;
        obj[`c${i}`] = row[idx];
      } else if (row && typeof row === "object") {
        obj[`c${i}`] = path !== null ? resolvePointer(folded.dataModel, row, path) : row[headerLabels[i]];
      } else {
        obj[`c${i}`] = i === 0 ? row : "";
      }
    }
    return obj;
  });

  return { title: titleText, columns, data };
};

export const formatInsightNumber = (value) => {
  const num = Number(value);
  if (!Number.isFinite(num)) return String(value ?? "");
  const rounded = Math.round(num * 100) / 100;
  return rounded.toLocaleString(undefined, { maximumFractionDigits: 2 });
};

export const formatInsightValue = (value, { format = null, currency = null, unit = null } = {}) => {
  let core;
  if (value === null || value === undefined) {
    core = "";
  } else if (typeof value === "number") {
    const formatted = formatInsightNumber(value);
    if (format === "percent") core = `${formatted}%`;
    else if (format === "currency" && currency) core = `${currency} ${formatted}`;
    else core = formatted;
  } else if (typeof value === "boolean") {
    core = value ? "Yes" : "No";
  } else if (typeof value === "object") {
    core = JSON.stringify(value);
  } else {
    core = String(value);
  }
  const u = typeof unit === "string" ? unit.trim() : "";
  return u && core ? `${core} ${u}` : core;
};

export const resolveActionArgs = (action, dataModel) => {
  const rawArgs = action?.args && typeof action.args === "object" ? action.args : {};
  const resolved = {};
  for (const [k, v] of Object.entries(rawArgs)) {
    if (v && typeof v === "object" && !Array.isArray(v) && typeof v.path === "string" && Object.keys(v).length === 1) {
      resolved[k] = resolvePointer(dataModel, dataModel, v.path);
    } else {
      resolved[k] = cloneJson(v);
    }
  }
  return resolved;
};

export const formatActionSubmission = ({ surfaceId = "", action = null, actionName = "click", resolvedArgs = null, fallbackData = null, hadInputs = false }) => {
  const actionId = action?.id || actionName || "click";
  const actionLabel = action?.label && action.label !== actionId ? `${actionId} (${action.label})` : actionId;
  const header = hadInputs || (resolvedArgs && Object.keys(resolvedArgs).length > 0)
    ? "[UI Event: Form submitted]"
    : "[UI Event: Button clicked]";
  const lines = [header];
  if (surfaceId) lines.push(`Surface: ${surfaceId}`);
  lines.push(`Action: ${actionLabel}`);
  if (action?.tool) lines.push(`Tool: ${action.tool}`);

  const dataObj =
    resolvedArgs && Object.keys(resolvedArgs).length > 0
      ? resolvedArgs
      : fallbackData && Object.keys(fallbackData).length > 0
        ? fallbackData
        : null;
  if (dataObj) {
    lines.push("Data:");
    for (const [k, v] of Object.entries(dataObj)) {
      const rendered =
        v === null || v === undefined
          ? ""
          : Array.isArray(v) || typeof v === "object"
            ? JSON.stringify(v)
            : String(v);
      lines.push(`  ${k}: ${rendered}`);
    }
  }
  return lines.join("\n");
};
