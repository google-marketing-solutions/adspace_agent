// AdSpace Agent — Google authorization helper & live `adk-ui` renderer for `/ui/`.
//
// Injected into `/ui/` from the same origin (`script-src 'self'`).
// 1. Listens to `/oauth2/events` (SSE) so the instant a tool call needs Google
//    OAuth consent, a centered popup window opens automatically while the tool
//    awaits completion on the server.
// 2. Watches `details.tool-call` blocks in `/ui/` for `adk-ui` (`UiResponse`)
//    component trees (`render_chart`, `render_table`, `render_layout`,
//    `render_card`, `render_form`, `render_confirm`, `render_modal`,
//    `render_alert`, `render_progress`, `render_toast`) and renders interactive
//    charts, sortable tables, KPI dashboards, and two-way forms directly in the
//    chat stream.
(() => {
  "use strict";
  const AUTH_PREFIX = "https://accounts.google.com/o/oauth2/";
  const CHANNEL = "adspace:oauth";
  const EVENTS_PATH = "/oauth2/events";
  const ACK_PATH = "/oauth2/ack";
  const RESUME_MARKER = "Google access has been granted for this chat";
  const SVG_NS = "http://www.w3.org/2000/svg";
  const PALETTE = [
    "#4285f4",
    "#34a853",
    "#fbbc04",
    "#ea4335",
    "#a142f4",
    "#24c1e0",
    "#f538a0",
    "#fa903e",
  ];

  const seenLinks = new WeakSet();
  const seenDetails = new WeakSet();
  const handledStates = new Set();
  let popup = null;
  let modal = null;

  // CSP forbids inline `style="..."` attributes; CSSOM assignments are allowed.
  const style = (el, props) => {
    for (const [k, v] of Object.entries(props)) el.style[k] = v;
  };

  const svgEl = (tag, attrs = {}) => {
    const el = document.createElementNS(SVG_NS, tag);
    for (const [k, v] of Object.entries(attrs)) {
      if (v !== undefined && v !== null) el.setAttribute(k, String(v));
    }
    return el;
  };

  const hideModal = () => {
    if (modal) modal.remove();
    modal = null;
  };

  const openPopup = (url) => {
    const w = 520;
    const h = 720;
    const left = Math.max(0, (window.screenX || 0) + ((window.outerWidth || w) - w) / 2);
    const top = Math.max(0, (window.screenY || 0) + ((window.outerHeight || h) - h) / 2);
    const win = window.open(
      url,
      "adspace_google_auth",
      `popup=yes,width=${w},height=${h},left=${left},top=${top}`
    );
    if (win) {
      popup = win;
      try {
        win.focus();
      } catch {
        /* ignore */
      }
    }
    return win;
  };

  const shortScopes = (scopes) =>
    (Array.isArray(scopes) ? scopes : [])
      .map((s) => String(s).slice(String(s).lastIndexOf("/") + 1))
      .filter(Boolean)
      .join(", ");

  // Centered modal shown ONLY if both `window.open` was blocked by the browser
  // AND the server could not open the consent screen locally (e.g. Cloud Run).
  const showFallbackModal = (authUrl, scopes) => {
    hideModal();
    modal = document.createElement("div");
    modal.setAttribute("role", "dialog");
    modal.setAttribute("aria-modal", "true");
    modal.setAttribute("aria-label", "Google authorization required");
    style(modal, {
      position: "fixed",
      inset: "0",
      zIndex: "2147483000",
      display: "flex",
      alignItems: "center",
      justifyContent: "center",
      background: "rgba(0, 0, 0, 0.55)",
      font: "14px/1.45 system-ui, sans-serif",
    });

    const box = document.createElement("div");
    style(box, {
      maxWidth: "380px",
      width: "90%",
      padding: "20px 22px",
      borderRadius: "14px",
      background: "#1f1f1f",
      color: "#f5f5f5",
      boxShadow: "0 12px 36px rgba(0, 0, 0, 0.45)",
    });

    const title = document.createElement("div");
    title.textContent = "Google authorization required";
    style(title, { fontSize: "16px", fontWeight: "600", marginBottom: "6px" });

    const detail = document.createElement("div");
    const names = shortScopes(scopes);
    detail.textContent = `Click below to open the Google consent popup${names ? ` (${names})` : ""}. Your request will continue automatically as soon as you grant access.`;
    style(detail, { opacity: "0.88", marginBottom: "16px" });

    const row = document.createElement("div");
    style(row, { display: "flex", gap: "10px", justifyContent: "flex-end" });

    const cancel = document.createElement("button");
    cancel.type = "button";
    cancel.textContent = "Cancel";
    style(cancel, {
      border: "0",
      background: "transparent",
      color: "inherit",
      opacity: "0.75",
      cursor: "pointer",
      padding: "8px 12px",
      font: "inherit",
    });
    cancel.addEventListener("click", hideModal);

    const button = document.createElement("button");
    button.type = "button";
    button.textContent = "Continue with Google";
    style(button, {
      border: "0",
      borderRadius: "8px",
      padding: "8px 16px",
      cursor: "pointer",
      background: "#8ab4f8",
      color: "#202124",
      fontWeight: "600",
      font: "inherit",
    });
    button.addEventListener("click", () => {
      if (openPopup(authUrl)) hideModal();
    });

    row.append(cancel, button);
    box.append(title, detail, row);
    modal.appendChild(box);
    document.body.appendChild(modal);
  };

  const handleAuthRequired = async (data) => {
    if (!data || !data.auth_url || !data.state) return;
    if (handledStates.has(data.state)) return;
    handledStates.add(data.state);

    const win = openPopup(data.auth_url);
    const opened = !!win;
    try {
      const resp = await fetch(ACK_PATH, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ state: data.state, opened }),
      });
      if (!opened && resp.ok) {
        const body = await resp.json();
        if (!body.fallback_opened) {
          showFallbackModal(data.auth_url, data.scopes);
        }
      } else if (!opened) {
        showFallbackModal(data.auth_url, data.scopes);
      }
    } catch {
      if (!opened) showFallbackModal(data.auth_url, data.scopes);
    }
  };

  const handleCompletion = (status) => {
    hideModal();
    if (status === "ok") {
      if (popup && !popup.closed) {
        try {
          popup.close();
        } catch {
          /* ignore */
        }
      }
      popup = null;
      const refresh = Array.from(document.querySelectorAll("button")).find((b) =>
        /^\s*refresh\s*$/i.test(b.textContent || "")
      );
      if (refresh) refresh.click();
    }
  };

  if ("EventSource" in window) {
    const es = new EventSource(EVENTS_PATH);
    es.addEventListener("message", (event) => {
      try {
        const data = JSON.parse(event.data);
        if (data.type === "auth_required") {
          handleAuthRequired(data);
        } else if (data.type === "auth_completed") {
          handleCompletion(data.status);
        }
      } catch {
        /* ignore malformed event */
      }
    });
  }

  const onReport = (data) => {
    if (!data || data.type !== CHANNEL) return;
    handleCompletion(data.status);
  };
  if ("BroadcastChannel" in window) {
    new BroadcastChannel(CHANNEL).addEventListener("message", (event) => onReport(event.data));
  }
  window.addEventListener("message", (event) => {
    if (event.origin === location.origin) onReport(event.data);
  });

  // ---------------------------------------------------------------------------
  // Live `adk-ui` (`UiResponse`) Component Renderer for `/ui/`
  // ---------------------------------------------------------------------------

  const sendMessageToComposer = (text) => {
    const ta =
      document.querySelector('textarea[aria-label="Message agent"]') ||
      document.querySelector(".composer-wrap textarea") ||
      document.querySelector("textarea");
    if (!ta) return false;
    const setter = Object.getOwnPropertyDescriptor(
      window.HTMLTextAreaElement.prototype,
      "value"
    )?.set;
    if (setter) setter.call(ta, text);
    else ta.value = text;
    ta.dispatchEvent(new Event("input", { bubbles: true }));
    setTimeout(() => {
      const sendBtn =
        document.querySelector("button.send-button:not(.stop-button)") ||
        document.querySelector('button[aria-label="Send message"]');
      if (sendBtn && !sendBtn.disabled) {
        sendBtn.click();
      } else {
        ta.focus();
      }
    }, 45);
    return true;
  };

  const toNumber = (val) => {
    if (typeof val === "number" && Number.isFinite(val)) return val;
    if (typeof val === "string") {
      const cleaned = val.replace(/[$,%\s,]/g, "");
      const n = Number(cleaned);
      if (cleaned !== "" && Number.isFinite(n)) return n;
    }
    return NaN;
  };

  const formatCompact = (n) => {
    if (!Number.isFinite(n)) return String(n ?? "");
    const abs = Math.abs(n);
    if (abs >= 1e9) return `${(n / 1e9).toFixed(1).replace(/\.0$/, "")}B`;
    if (abs >= 1e6) return `${(n / 1e6).toFixed(1).replace(/\.0$/, "")}M`;
    if (abs >= 1e3) return `${(n / 1e3).toFixed(1).replace(/\.0$/, "")}K`;
    if (Number.isInteger(n)) return String(n);
    return n.toFixed(2).replace(/\.?0+$/, "");
  };

  const formatCellValue = (val) => {
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

  const getThemeTokens = (themePref) => {
    const docTheme = document.documentElement.getAttribute("data-theme");
    const isDark =
      themePref === "dark" ||
      (themePref !== "light" &&
        (docTheme === "dark" ||
          (!docTheme &&
            window.matchMedia &&
            window.matchMedia("(prefers-color-scheme: dark)").matches)));
    return isDark
      ? {
          bg: "#16181d",
          cardBg: "#1e2128",
          subtleBg: "#252932",
          border: "rgba(255,255,255,0.11)",
          text: "#f1f3f4",
          muted: "#9aa0a6",
          gridLine: "rgba(255,255,255,0.08)",
          stripe: "rgba(255,255,255,0.03)",
          inputBg: "#121418",
          shadow: "0 6px 20px rgba(0,0,0,0.32)",
        }
      : {
          bg: "#ffffff",
          cardBg: "#ffffff",
          subtleBg: "#f8f9fa",
          border: "#e0e3e7",
          text: "#202124",
          muted: "#5f6368",
          gridLine: "#eceff1",
          stripe: "#f8fafd",
          inputBg: "#ffffff",
          shadow: "0 4px 14px rgba(60,64,67,0.08)",
        };
  };

  const variantColors = (variant) => {
    switch (String(variant || "default").toLowerCase()) {
      case "success":
        return { bg: "rgba(52,168,83,0.15)", fg: "#2e7d32", border: "rgba(52,168,83,0.4)" };
      case "warning":
        return { bg: "rgba(251,188,4,0.16)", fg: "#b06000", border: "rgba(251,188,4,0.45)" };
      case "error":
      case "danger":
        return { bg: "rgba(234,67,53,0.15)", fg: "#c5221f", border: "rgba(234,67,53,0.4)" };
      case "info":
      case "primary":
        return { bg: "rgba(66,133,244,0.15)", fg: "#1967d2", border: "rgba(66,133,244,0.4)" };
      default:
        return { bg: "rgba(128,134,139,0.14)", fg: "inherit", border: "rgba(128,134,139,0.35)" };
    }
  };

  // Renders SVG charts (`bar`, `line`, `area`, `pie`, `scatter`, `sparkline`).
  const renderChart = (comp, tokens) => {
    const wrap = document.createElement("div");
    style(wrap, {
      display: "flex",
      flexDirection: "column",
      gap: "10px",
      padding: "14px 16px",
      borderRadius: "12px",
      background: tokens.cardBg,
      border: `1px solid ${tokens.border}`,
    });

    if (comp.title) {
      const title = document.createElement("div");
      title.textContent = comp.title;
      style(title, { fontSize: "15px", fontWeight: "600", color: tokens.text });
      wrap.appendChild(title);
    }

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
      style(empty, { color: tokens.muted, fontSize: "13px", padding: "12px 0" });
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
        padding: "6px 0",
      });

      const svg = svgEl("svg", { viewBox: "0 0 220 220" });
      style(svg, { width: "190px", height: "190px", flexShrink: "0" });

      const cx = 110;
      const cy = 110;
      const rOuter = 92;
      const rInner = 52;
      let angle = -Math.PI / 2;

      for (const s of slices) {
        const frac = s.value / total;
        const sweep = frac * Math.PI * 2;
        if (frac >= 0.9999) {
          const c = svgEl("circle", { cx, cy, r: (rOuter + rInner) / 2, fill: "none", stroke: s.color, "stroke-width": rOuter - rInner });
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
          const path = svgEl("path", { d, fill: s.color, stroke: tokens.cardBg, "stroke-width": "2" });
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
        fill: tokens.muted,
        "font-size": "11",
      });
      centerTop.textContent = yKey;
      const centerVal = svgEl("text", {
        x: cx,
        y: cy + 16,
        "text-anchor": "middle",
        fill: tokens.text,
        "font-size": "16",
        "font-weight": "700",
      });
      centerVal.textContent = formatCompact(total);
      svg.append(centerTop, centerVal);

      const legend = document.createElement("div");
      style(legend, {
        display: "flex",
        flexDirection: "column",
        gap: "7px",
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
          width: "10px",
          height: "10px",
          borderRadius: "3px",
          background: s.color,
          flexShrink: "0",
        });
        const lbl = document.createElement("span");
        lbl.textContent = s.label;
        style(lbl, {
          color: tokens.text,
          overflow: "hidden",
          textOverflow: "ellipsis",
          whiteSpace: "nowrap",
        });
        left.append(swatch, lbl);

        const right = document.createElement("span");
        right.textContent = `${formatCompact(s.value)} (${((s.value / total) * 100).toFixed(1)}%)`;
        style(right, { color: tokens.muted, fontVariantNumeric: "tabular-nums", flexShrink: "0" });
        item.append(left, right);
        legend.appendChild(item);
      }

      rowWrap.append(svg, legend);
      wrap.appendChild(rowWrap);
      return wrap;
    }

    // Cartesian SVG chart (`bar`, `line`, `area`, `scatter`, `sparkline`)
    const isSpark = kind === "sparkline";
    const W = 680;
    const H = isSpark ? 90 : 270;
    const padL = isSpark ? 10 : 56;
    const padR = isSpark ? 10 : 20;
    const padT = isSpark ? 10 : 18;
    const padB = isSpark ? 12 : 52;
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
    style(svg, { width: "100%", height: "auto", maxHeight: isSpark ? "95px" : "290px", overflow: "visible" });

    // Grid lines + Y-axis labels
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
          stroke: tokens.gridLine,
          "stroke-width": "1",
        });
        const label = svgEl("text", {
          x: padL - 8,
          y: y + 4,
          "text-anchor": "end",
          fill: tokens.muted,
          "font-size": "11",
        });
        label.textContent = formatCompact(val);
        svg.append(line, label);
      }
    }

    const n = data.length;
    if (kind === "bar") {
      const groupStep = plotW / Math.max(1, n);
      const groupW = groupStep * 0.74;
      const barW = Math.max(4, groupW / Math.max(1, yKeys.length) - 2);
      const zeroY = yToPx(Math.max(0, minVal));

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
            rx: "4",
            fill: colors[sIdx % colors.length],
          });
          const title = svgEl("title");
          title.textContent = `${row[xKey] ?? i} · ${yk}: ${formatCellValue(raw)}`;
          rect.appendChild(title);
          svg.appendChild(rect);

          if (n <= 12 && yKeys.length === 1) {
            const valTxt = svgEl("text", {
              x: bx + barW / 2,
              y: Math.max(11, top - 5),
              "text-anchor": "middle",
              fill: tokens.muted,
              "font-size": "10",
            });
            valTxt.textContent = formatCompact(v);
            svg.appendChild(valTxt);
          }
        });

        // X-axis label
        const stepEvery = Math.max(1, Math.ceil(n / 10));
        if (i % stepEvery === 0) {
          const rawLbl = String(row[xKey] ?? "");
          const shortLbl = rawLbl.length > 14 ? `${rawLbl.slice(0, 13)}…` : rawLbl;
          const xTxt = svgEl("text", {
            x: padL + (i + 0.5) * groupStep,
            y: H - padB + 18,
            "text-anchor": "middle",
            fill: tokens.muted,
            "font-size": "11",
          });
          xTxt.textContent = shortLbl;
          const t = svgEl("title");
          t.textContent = rawLbl;
          xTxt.appendChild(t);
          svg.appendChild(xTxt);
        }
      });
    } else {
      // `line`, `area`, `scatter`, `sparkline`
      const xForIdx = (i) =>
        n === 1 ? padL + plotW / 2 : padL + (i / (n - 1)) * plotW;
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
              "fill-opacity": "0.18",
            })
          );
        }

        if (kind !== "scatter" && pts.length > 1) {
          svg.appendChild(
            svgEl("polyline", {
              points: pts.map((p) => `${p.x},${p.y}`).join(" "),
              fill: "none",
              stroke: color,
              "stroke-width": isSpark ? "2" : "2.5",
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
              r: kind === "scatter" ? "5" : "3.5",
              fill: color,
              stroke: tokens.cardBg,
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
          const shortLbl = rawLbl.length > 14 ? `${rawLbl.slice(0, 13)}…` : rawLbl;
          const xTxt = svgEl("text", {
            x: xForIdx(i),
            y: H - padB + 18,
            "text-anchor": "middle",
            fill: tokens.muted,
            "font-size": "11",
          });
          xTxt.textContent = shortLbl;
          svg.appendChild(xTxt);
        });
      }
    }

    wrap.appendChild(svg);

    // Legend
    if (comp.show_legend !== false && !isSpark && yKeys.length > 0) {
      const legend = document.createElement("div");
      style(legend, {
        display: "flex",
        flexWrap: "wrap",
        gap: "14px",
        justifyContent: "center",
        fontSize: "12px",
        color: tokens.muted,
      });
      yKeys.forEach((yk, sIdx) => {
        const item = document.createElement("span");
        style(item, { display: "inline-flex", alignItems: "center", gap: "6px" });
        const dot = document.createElement("span");
        style(dot, {
          width: "10px",
          height: "10px",
          borderRadius: "3px",
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

  // Renders interactive sortable/filterable tables (`table`).
  const renderTable = (comp, tokens) => {
    const wrap = document.createElement("div");
    style(wrap, {
      borderRadius: "12px",
      border: `1px solid ${tokens.border}`,
      background: tokens.cardBg,
      overflow: "hidden",
    });

    const rawRows = Array.isArray(comp.data) ? [...comp.data] : [];
    const columns =
      Array.isArray(comp.columns) && comp.columns.length
        ? comp.columns
        : rawRows[0]
          ? Object.keys(rawRows[0]).map((k) => ({ header: k, accessor_key: k, sortable: true }))
          : [];

    let filterQuery = "";
    let sortKey = null;
    let sortAsc = true;
    let page = 0;
    const pageSize = comp.page_size && comp.page_size > 0 ? Number(comp.page_size) : 0;

    if (rawRows.length > 4) {
      const toolbar = document.createElement("div");
      style(toolbar, {
        display: "flex",
        justifyContent: "space-between",
        alignItems: "center",
        padding: "8px 12px",
        borderBottom: `1px solid ${tokens.border}`,
        background: tokens.subtleBg,
      });
      const countLbl = document.createElement("span");
      countLbl.textContent = `${rawRows.length} rows`;
      style(countLbl, { fontSize: "12px", color: tokens.muted });

      const search = document.createElement("input");
      search.type = "search";
      search.placeholder = "Filter rows…";
      style(search, {
        padding: "5px 10px",
        borderRadius: "7px",
        border: `1px solid ${tokens.border}`,
        background: tokens.inputBg,
        color: tokens.text,
        fontSize: "12px",
        width: "180px",
      });
      search.addEventListener("input", () => {
        filterQuery = search.value.trim().toLowerCase();
        page = 0;
        rebuildBody();
      });
      toolbar.append(countLbl, search);
      wrap.appendChild(toolbar);
    }

    const scroller = document.createElement("div");
    style(scroller, { overflowX: "auto" });

    const table = document.createElement("table");
    style(table, {
      width: "100%",
      borderCollapse: "collapse",
      fontSize: "13px",
      color: tokens.text,
    });

    const thead = document.createElement("thead");
    const headRow = document.createElement("tr");
    style(headRow, { background: tokens.subtleBg, borderBottom: `1px solid ${tokens.border}` });

    const tbody = document.createElement("tbody");
    const pager = document.createElement("div");

    const rebuildHeader = () => {
      headRow.replaceChildren();
      for (const col of columns) {
        const th = document.createElement("th");
        const canSort = comp.sortable !== false && col.sortable !== false;
        const indicator =
          sortKey === col.accessor_key ? (sortAsc ? " ↑" : " ↓") : canSort ? " ↕" : "";
        th.textContent = `${col.header}${indicator}`;
        style(th, {
          textAlign: "left",
          padding: "9px 12px",
          fontWeight: "600",
          fontSize: "12px",
          color: tokens.muted,
          whiteSpace: "nowrap",
          cursor: canSort ? "pointer" : "default",
          userSelect: "none",
        });
        if (canSort) {
          th.addEventListener("click", () => {
            if (sortKey === col.accessor_key) sortAsc = !sortAsc;
            else {
              sortKey = col.accessor_key;
              sortAsc = true;
            }
            rebuildHeader();
            rebuildBody();
          });
        }
        headRow.appendChild(th);
      }
    };

    const rebuildBody = () => {
      tbody.replaceChildren();
      let rows = rawRows.filter((r) => {
        if (!filterQuery) return true;
        return columns.some((c) =>
          String(r[c.accessor_key] ?? "")
            .toLowerCase()
            .includes(filterQuery)
        );
      });

      if (sortKey) {
        rows.sort((a, b) => {
          const va = a[sortKey];
          const vb = b[sortKey];
          const na = toNumber(va);
          const nb = toNumber(vb);
          const cmp =
            Number.isFinite(na) && Number.isFinite(nb)
              ? na - nb
              : String(va ?? "").localeCompare(String(vb ?? ""));
          return sortAsc ? cmp : -cmp;
        });
      }

      const totalRows = rows.length;
      if (pageSize > 0) {
        const start = page * pageSize;
        rows = rows.slice(start, start + pageSize);
      }

      rows.forEach((row, idx) => {
        const tr = document.createElement("tr");
        style(tr, {
          borderBottom: `1px solid ${tokens.border}`,
          background: comp.striped && idx % 2 === 1 ? tokens.stripe : "transparent",
        });
        for (const col of columns) {
          const td = document.createElement("td");
          const raw = row[col.accessor_key];
          const text = formatCellValue(raw);
          const isNumeric =
            typeof raw === "number" ||
            (typeof raw === "string" && /^[$+-]?[\d,.]+%?$/.test(raw.trim()));
          td.textContent = text;
          style(td, {
            padding: "9px 12px",
            textAlign: isNumeric ? "right" : "left",
            fontVariantNumeric: isNumeric ? "tabular-nums" : "normal",
            whiteSpace: isNumeric ? "nowrap" : "normal",
          });
          tr.appendChild(td);
        }
        tbody.appendChild(tr);
      });

      if (pageSize > 0 && totalRows > pageSize) {
        pager.replaceChildren();
        const totalPages = Math.ceil(totalRows / pageSize);
        style(pager, {
          display: "flex",
          justifyContent: "space-between",
          alignItems: "center",
          padding: "8px 12px",
          fontSize: "12px",
          color: tokens.muted,
          background: tokens.subtleBg,
        });
        const info = document.createElement("span");
        info.textContent = `Page ${page + 1} of ${totalPages} (${totalRows} rows)`;
        const btnWrap = document.createElement("div");
        style(btnWrap, { display: "flex", gap: "6px" });
        const mkBtn = (lbl, disabled, onClick) => {
          const b = document.createElement("button");
          b.type = "button";
          b.textContent = lbl;
          b.disabled = disabled;
          style(b, {
            padding: "4px 10px",
            borderRadius: "6px",
            border: `1px solid ${tokens.border}`,
            background: tokens.cardBg,
            color: tokens.text,
            cursor: disabled ? "default" : "pointer",
            opacity: disabled ? "0.45" : "1",
            fontSize: "12px",
          });
          b.addEventListener("click", onClick);
          return b;
        };
        btnWrap.append(
          mkBtn("Prev", page <= 0, () => {
            page--;
            rebuildBody();
          }),
          mkBtn("Next", page >= totalPages - 1, () => {
            page++;
            rebuildBody();
          })
        );
        pager.append(info, btnWrap);
      } else {
        style(pager, { display: "none" });
      }
    };

    rebuildHeader();
    rebuildBody();
    thead.appendChild(headRow);
    table.append(thead, tbody);
    scroller.appendChild(table);
    wrap.append(scroller, pager);
    return wrap;
  };

  // Recursively renders any `adk_ui::Component` node.
  const renderComponent = (comp, tokens, surfaceRoot) => {
    if (!comp || typeof comp !== "object") return document.createTextNode("");
    const type = String(comp.type || "").toLowerCase();

    switch (type) {
      case "text": {
        const v = String(comp.variant || "body").toLowerCase();
        const el = document.createElement(v === "code" ? "code" : "div");
        el.textContent = comp.content ?? "";
        const map = {
          h1: { fontSize: "20px", fontWeight: "700", lineHeight: "1.3", color: tokens.text },
          h2: { fontSize: "17px", fontWeight: "700", lineHeight: "1.35", color: tokens.text },
          h3: { fontSize: "15px", fontWeight: "600", lineHeight: "1.4", color: tokens.text },
          h4: { fontSize: "13.5px", fontWeight: "600", lineHeight: "1.4", color: tokens.text },
          caption: { fontSize: "12px", color: tokens.muted, lineHeight: "1.4" },
          code: {
            fontFamily: "ui-monospace, SFMono-Regular, Menlo, monospace",
            fontSize: "12.5px",
            background: tokens.subtleBg,
            padding: "2px 6px",
            borderRadius: "5px",
          },
          body: { fontSize: "13.5px", lineHeight: "1.5", color: tokens.text },
        };
        style(el, map[v] || map.body);
        return el;
      }

      case "badge": {
        const span = document.createElement("span");
        span.textContent = comp.label ?? "";
        const vc = variantColors(comp.variant);
        style(span, {
          display: "inline-block",
          padding: "2px 9px",
          borderRadius: "999px",
          fontSize: "11.5px",
          fontWeight: "600",
          background: vc.bg,
          color: vc.fg,
          border: `1px solid ${vc.border}`,
          alignSelf: "flex-start",
        });
        return span;
      }

      case "divider": {
        const hr = document.createElement("hr");
        style(hr, {
          border: "0",
          borderTop: `1px solid ${tokens.border}`,
          margin: "8px 0",
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
          div.appendChild(renderComponent(child, tokens, surfaceRoot));
        }
        return div;
      }

      case "grid": {
        const div = document.createElement("div");
        const cols = Math.max(1, Math.min(6, Number(comp.columns) || 2));
        const gapPx = Math.max(2, Number(comp.gap ?? 3)) * 4;
        style(div, {
          display: "grid",
          gridTemplateColumns: `repeat(auto-fit, minmax(${cols >= 3 ? "165px" : "240px"}, 1fr))`,
          gap: `${gapPx}px`,
          alignItems: "stretch",
        });
        for (const child of comp.children || []) {
          div.appendChild(renderComponent(child, tokens, surfaceRoot));
        }
        return div;
      }

      case "container": {
        const div = document.createElement("div");
        const padPx = Math.max(0, Number(comp.padding ?? 3)) * 4;
        style(div, {
          padding: `${padPx}px`,
          display: "flex",
          flexDirection: "column",
          gap: "12px",
        });
        for (const child of comp.children || []) {
          div.appendChild(renderComponent(child, tokens, surfaceRoot));
        }
        return div;
      }

      case "card":
      case "modal": {
        const card = document.createElement("div");
        card.setAttribute("data-ui-scope", "true");
        style(card, {
          display: "flex",
          flexDirection: "column",
          justifyContent: "space-between",
          gap: "12px",
          padding: "16px",
          borderRadius: "12px",
          background: tokens.cardBg,
          border: `1px solid ${tokens.border}`,
          boxShadow: tokens.shadow,
        });

        if (comp.title || comp.description) {
          const hdr = document.createElement("div");
          style(hdr, { display: "flex", flexDirection: "column", gap: "4px" });
          if (comp.title) {
            const t = document.createElement("div");
            t.textContent = comp.title;
            style(t, { fontSize: "15px", fontWeight: "600", color: tokens.text });
            hdr.appendChild(t);
          }
          if (comp.description) {
            const d = document.createElement("div");
            d.textContent = comp.description;
            style(d, { fontSize: "12.5px", color: tokens.muted });
            hdr.appendChild(d);
          }
          card.appendChild(hdr);
        }

        if (Array.isArray(comp.content) && comp.content.length) {
          const body = document.createElement("div");
          style(body, { display: "flex", flexDirection: "column", gap: "12px" });
          for (const child of comp.content) {
            body.appendChild(renderComponent(child, tokens, card));
          }
          card.appendChild(body);
        }

        if (Array.isArray(comp.footer) && comp.footer.length) {
          const ftr = document.createElement("div");
          style(ftr, {
            display: "flex",
            flexWrap: "wrap",
            gap: "8px",
            justifyContent: "flex-end",
            paddingTop: "10px",
            borderTop: `1px solid ${tokens.border}`,
          });
          for (const child of comp.footer) {
            ftr.appendChild(renderComponent(child, tokens, card));
          }
          card.appendChild(ftr);
        }
        return card;
      }

      case "tabs": {
        const wrap = document.createElement("div");
        style(wrap, { display: "flex", flexDirection: "column", gap: "12px" });
        const bar = document.createElement("div");
        style(bar, {
          display: "flex",
          gap: "6px",
          borderBottom: `1px solid ${tokens.border}`,
          paddingBottom: "6px",
        });
        const panel = document.createElement("div");
        const tabs = Array.isArray(comp.tabs) ? comp.tabs : [];
        let activeIdx = 0;

        const selectTab = (idx) => {
          activeIdx = idx;
          Array.from(bar.children).forEach((btn, i) => {
            style(btn, {
              background: i === activeIdx ? "#4285f4" : tokens.subtleBg,
              color: i === activeIdx ? "#ffffff" : tokens.text,
            });
          });
          panel.replaceChildren();
          for (const child of tabs[idx]?.content || []) {
            panel.appendChild(renderComponent(child, tokens, surfaceRoot));
          }
        };

        tabs.forEach((tab, idx) => {
          const b = document.createElement("button");
          b.type = "button";
          b.textContent = tab.label || `Tab ${idx + 1}`;
          style(b, {
            border: "0",
            borderRadius: "7px",
            padding: "6px 12px",
            fontSize: "12.5px",
            fontWeight: "600",
            cursor: "pointer",
          });
          b.addEventListener("click", () => selectTab(idx));
          bar.appendChild(b);
        });
        wrap.append(bar, panel);
        if (tabs.length) selectTab(0);
        return wrap;
      }

      case "chart":
        return renderChart(comp, tokens);

      case "table":
        return renderTable(comp, tokens);

      case "key_value": {
        const dl = document.createElement("div");
        style(dl, {
          display: "grid",
          gridTemplateColumns: "minmax(120px, auto) 1fr",
          gap: "8px 16px",
          padding: "12px 14px",
          borderRadius: "10px",
          background: tokens.subtleBg,
          border: `1px solid ${tokens.border}`,
          fontSize: "13px",
        });
        for (const pair of comp.pairs || []) {
          const k = document.createElement("div");
          k.textContent = pair.key;
          style(k, { color: tokens.muted, fontWeight: "500" });
          const v = document.createElement("div");
          v.textContent = pair.value;
          style(v, { color: tokens.text, fontWeight: "600" });
          dl.append(k, v);
        }
        return dl;
      }

      case "list": {
        const list = document.createElement(comp.ordered ? "ol" : "ul");
        style(list, {
          margin: "0",
          paddingLeft: "20px",
          display: "flex",
          flexDirection: "column",
          gap: "5px",
          fontSize: "13.5px",
          color: tokens.text,
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
          padding: "12px 14px",
          borderRadius: "9px",
          background: "#111318",
          color: "#e8eaed",
          fontSize: "12.5px",
          overflowX: "auto",
          fontFamily: "ui-monospace, SFMono-Regular, Menlo, monospace",
        });
        return pre;
      }

      case "alert":
      case "toast": {
        const box = document.createElement("div");
        const vc = variantColors(comp.variant);
        style(box, {
          padding: "12px 14px",
          borderRadius: "10px",
          background: vc.bg,
          border: `1px solid ${vc.border}`,
          display: "flex",
          flexDirection: "column",
          gap: "3px",
        });
        const title = document.createElement("div");
        title.textContent = comp.title || comp.message || "";
        style(title, { fontWeight: "600", fontSize: "13.5px", color: tokens.text });
        box.appendChild(title);
        if (comp.description) {
          const desc = document.createElement("div");
          desc.textContent = comp.description;
          style(desc, { fontSize: "12.5px", color: tokens.muted });
          box.appendChild(desc);
        }
        return box;
      }

      case "progress": {
        const wrap = document.createElement("div");
        style(wrap, { display: "flex", flexDirection: "column", gap: "6px" });
        const pct = Math.max(0, Math.min(100, Number(comp.value) || 0));
        const top = document.createElement("div");
        style(top, { display: "flex", justifyContent: "space-between", fontSize: "12.5px" });
        const lbl = document.createElement("span");
        lbl.textContent = comp.label || "Progress";
        style(lbl, { color: tokens.text, fontWeight: "500" });
        const val = document.createElement("span");
        val.textContent = `${pct}%`;
        style(val, { color: tokens.muted, fontVariantNumeric: "tabular-nums" });
        top.append(lbl, val);

        const track = document.createElement("div");
        style(track, {
          height: "8px",
          borderRadius: "999px",
          background: tokens.subtleBg,
          overflow: "hidden",
        });
        const bar = document.createElement("div");
        style(bar, {
          width: `${pct}%`,
          height: "100%",
          borderRadius: "999px",
          background: "#4285f4",
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
        style(field, { display: "flex", flexDirection: "column", gap: "5px", fontSize: "12.5px" });
        const lbl = document.createElement("span");
        lbl.textContent = `${comp.label || comp.name}${comp.required ? " *" : ""}`;
        style(lbl, { fontWeight: "600", color: tokens.text });
        field.appendChild(lbl);

        let control;
        if (type === "textarea") {
          control = document.createElement("textarea");
          control.rows = Number(comp.rows) || 3;
          control.placeholder = comp.placeholder || "";
          control.value = comp.default_value || "";
        } else if (type === "select" || type === "multi_select") {
          control = document.createElement("select");
          if (type === "multi_select") control.multiple = true;
          for (const opt of comp.options || []) {
            const o = document.createElement("option");
            o.value = opt.value;
            o.textContent = opt.label;
            control.appendChild(o);
          }
        } else if (type === "switch") {
          const row = document.createElement("div");
          style(row, { display: "flex", alignItems: "center", gap: "8px" });
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
          if (comp.default_value !== undefined && comp.default_value !== null) {
            control.value = String(comp.default_value);
          }
        }

        if (type !== "switch") {
          style(control, {
            padding: "8px 10px",
            borderRadius: "8px",
            border: `1px solid ${tokens.border}`,
            background: tokens.inputBg,
            color: tokens.text,
            font: "inherit",
          });
          field.appendChild(control);
        }

        control.setAttribute("data-ui-field", comp.name || comp.label || "field");
        return field;
      }

      case "button": {
        const btn = document.createElement("button");
        btn.type = "button";
        btn.textContent = comp.label || "Submit";
        btn.disabled = !!comp.disabled;
        const v = String(comp.variant || "primary").toLowerCase();
        const btnStyles =
          v === "primary"
            ? { background: "#4285f4", color: "#ffffff", border: "1px solid #4285f4" }
            : v === "danger"
              ? { background: "#ea4335", color: "#ffffff", border: "1px solid #ea4335" }
              : {
                  background: tokens.subtleBg,
                  color: tokens.text,
                  border: `1px solid ${tokens.border}`,
                };
        style(btn, {
          padding: "8px 15px",
          borderRadius: "8px",
          fontSize: "13px",
          fontWeight: "600",
          cursor: btn.disabled ? "default" : "pointer",
          alignSelf: "flex-start",
          ...btnStyles,
        });

        btn.addEventListener("click", () => {
          const scope = btn.closest('[data-ui-scope="true"]') || surfaceRoot;
          const inputs = scope ? Array.from(scope.querySelectorAll("[data-ui-field]")) : [];
          const actionId = comp.action_id || comp.label || "click";
          let msg;
          if (inputs.length > 0) {
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
          if (sendMessageToComposer(msg)) {
            const orig = btn.textContent;
            btn.textContent = `${orig} ✓`;
            setTimeout(() => {
              btn.textContent = orig;
            }, 1800);
          }
        });
        return btn;
      }

      default:
        return document.createTextNode("");
    }
  };

  // Extracts an `adk_ui::UiResponse` (`{ components: [...] }`) from a tool JSON object.
  const extractUiResponse = (obj) => {
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

  const wireToolDetails = (details) => {
    if (seenDetails.has(details)) return;
    const pre = details.querySelector("pre");
    if (!pre || !pre.textContent) return;

    let parsed;
    try {
      parsed = JSON.parse(pre.textContent);
    } catch {
      return;
    }
    seenDetails.add(details);

    // Hide/compact raw `Tool · render_*` call blocks so the rendered UI stands out.
    const callName =
      parsed?.name || parsed?.functionCall?.name || parsed?.function_call?.name || "";
    if (typeof callName === "string" && callName.startsWith("render_")) {
      style(details, { opacity: "0.55", fontSize: "11.5px", marginTop: "4px" });
      return;
    }

    const uiResp = extractUiResponse(parsed);
    if (!uiResp) return;

    const tokens = getThemeTokens(uiResp.theme);
    const surface = document.createElement("div");
    surface.className = "adk-ui-surface";
    surface.setAttribute("data-ui-scope", "true");
    style(surface, {
      display: "flex",
      flexDirection: "column",
      gap: "12px",
      margin: "10px 0 6px 0",
      font: "13.5px/1.45 system-ui, -apple-system, sans-serif",
      color: tokens.text,
    });

    for (const comp of uiResp.components) {
      surface.appendChild(renderComponent(comp, tokens, surface));
    }

    details.parentNode?.insertBefore(surface, details);
    details.open = false;
    const summary = details.querySelector("summary");
    if (summary) summary.textContent = "UI specification (JSON)";
    style(details, { opacity: "0.55", fontSize: "11.5px" });
  };

  // If an OAuth link is ever clicked directly in the chat transcript, open it
  // in the centered popup instead of navigating away.
  const isAuthLink = (el) =>
    el instanceof HTMLAnchorElement &&
    typeof el.href === "string" &&
    el.href.startsWith(AUTH_PREFIX);

  const wireLink = (anchor) => {
    if (seenLinks.has(anchor)) return;
    seenLinks.add(anchor);
    anchor.addEventListener("click", (event) => {
      event.preventDefault();
      openPopup(anchor.href);
    });
  };

  const scan = (root) => {
    if (!root || root.nodeType !== Node.ELEMENT_NODE) return;
    if (isAuthLink(root)) wireLink(root);
    if (root.matches && root.matches("details.tool-call")) wireToolDetails(root);
    if (root.querySelectorAll) {
      root.querySelectorAll("a[href]").forEach((a) => {
        if (isAuthLink(a)) wireLink(a);
      });
      root.querySelectorAll("details.tool-call").forEach(wireToolDetails);
    }
  };

  const observer = new MutationObserver((mutations) => {
    for (const mutation of mutations) {
      for (const node of mutation.addedNodes) {
        if (node.nodeType !== Node.ELEMENT_NODE) continue;
        scan(node);
        if (modal && node.textContent && node.textContent.includes(RESUME_MARKER)) {
          hideModal();
        }
      }
    }
  });
  observer.observe(document.documentElement, { childList: true, subtree: true });
  scan(document.body);
})();
