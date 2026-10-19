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

// `adk-ui` table component renderer (sortable / filterable / paginated). Pure DOM, no app state.

import { style } from "../dom.js";
import { formatCellValue, toNumber } from "../format.js";

export const renderTableParts = (comp) => {
  const wrap = document.createElement("div");
  style(wrap, { display: "flex", flexDirection: "column" });

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

  const toolbar = document.createElement("div");
  style(toolbar, { display: "flex", alignItems: "center", gap: "10px" });
  const countChip = document.createElement("span");
  countChip.setAttribute("aria-live", "polite");
  style(countChip, {
    fontSize: "12.5px",
    fontWeight: "450",
    color: "var(--dm-text-secondary)",
  });
  toolbar.appendChild(countChip);

  if (rawRows.length > 4) {
    const search = document.createElement("input");
    search.type = "search";
    search.placeholder = "Filter…";
    search.setAttribute("aria-label", "Filter table rows");
    style(search, {
      padding: "6px 14px",
      borderRadius: "999px",
      border: "1px solid var(--dm-outline)",
      background: "var(--dm-surface-gray)",
      color: "var(--dm-text-primary)",
      fontSize: "13px",
      width: "180px",
    });
    // Debounced: filtering re-sorts and rebuilds the whole body.
    let filterTimer = 0;
    search.addEventListener("input", () => {
      clearTimeout(filterTimer);
      filterTimer = setTimeout(() => {
        filterQuery = search.value.trim().toLowerCase();
        page = 0;
        rebuildBody();
      }, 100);
    });
    toolbar.appendChild(search);
  }

  const scroller = document.createElement("div");
  style(scroller, { overflowX: "auto" });

  const table = document.createElement("table");
  style(table, {
    width: "100%",
    borderCollapse: "collapse",
    fontSize: "13.5px",
    color: "var(--dm-text-primary)",
  });

  const thead = document.createElement("thead");
  const headRow = document.createElement("tr");
  style(headRow, {
    background: "transparent",
    borderBottom: "1px solid var(--dm-outline)",
  });

  const tbody = document.createElement("tbody");
  const pager = document.createElement("div");

  const rebuildHeader = () => {
    headRow.replaceChildren();
    for (const col of columns) {
      const th = document.createElement("th");
      th.scope = "col";
      const canSort = comp.sortable !== false && col.sortable !== false;
      const isSorted = sortKey === col.accessor_key;
      const indicator = isSorted ? (sortAsc ? " ↑" : " ↓") : canSort ? " ↕" : "";
      // `aria-sort` only belongs on headers that can actually be sorted.
      if (canSort) {
        th.setAttribute("aria-sort", isSorted ? (sortAsc ? "ascending" : "descending") : "none");
      }
      style(th, {
        textAlign: "left",
        padding: canSort ? "0" : "12px 18px",
        fontWeight: "500",
        fontSize: "13px",
        letterSpacing: "0.005em",
        color: "var(--dm-text-primary)",
        whiteSpace: "nowrap",
        userSelect: "none",
      });
      if (canSort) {
        // A real button so the sort is reachable and operable from the keyboard.
        const sortBtn = document.createElement("button");
        sortBtn.type = "button";
        sortBtn.textContent = `${col.header}${indicator}`;
        sortBtn.setAttribute(
          "aria-label",
          `Sort by ${col.header}${isSorted ? (sortAsc ? ", descending" : ", ascending") : ""}`
        );
        style(sortBtn, {
          all: "unset",
          display: "block",
          width: "100%",
          boxSizing: "border-box",
          padding: "12px 18px",
          font: "inherit",
          color: "inherit",
          cursor: "pointer",
        });
        sortBtn.addEventListener("click", () => {
          if (sortKey === col.accessor_key) sortAsc = !sortAsc;
          else {
            sortKey = col.accessor_key;
            sortAsc = true;
          }
          rebuildHeader();
          rebuildBody();
          // The header was rebuilt; keep keyboard focus on the same column.
          headRow.children[columns.indexOf(col)]?.querySelector("button")?.focus();
        });
        th.appendChild(sortBtn);
      } else {
        th.textContent = `${col.header}${indicator}`;
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
    countChip.textContent =
      filterQuery && totalRows !== rawRows.length
        ? `${totalRows} of ${rawRows.length} rows`
        : `${totalRows} rows`;
    if (pageSize > 0) {
      const start = page * pageSize;
      rows = rows.slice(start, start + pageSize);
    }

    rows.forEach((row, idx) => {
      const tr = document.createElement("tr");
      const isLast = idx === rows.length - 1 && !(pageSize > 0 && totalRows > pageSize);
      style(tr, {
        borderBottom: isLast ? "0" : "1px solid var(--dm-outline)",
        background: comp.striped && idx % 2 === 1 ? "var(--dm-surface-gray)" : "transparent",
      });
      columns.forEach((col, colIdx) => {
        const td = document.createElement("td");
        const raw = row[col.accessor_key];
        const text = formatCellValue(raw);
        const isNumeric =
          typeof raw === "number" ||
          (typeof raw === "string" && /^[$+-]?[\d,.]+%?$/.test(raw.trim()));
        td.textContent = text;
        style(td, {
          padding: "12px 18px",
          textAlign: isNumeric ? "right" : "left",
          fontFamily: isNumeric ? "var(--mono)" : "inherit",
          fontSize: isNumeric ? "13px" : "13.5px",
          color: colIdx === 0 && !isNumeric ? "var(--dm-text-primary)" : "var(--dm-text-secondary)",
          whiteSpace: isNumeric ? "nowrap" : "normal",
        });
        tr.appendChild(td);
      });
      tbody.appendChild(tr);
    });

    if (pageSize > 0 && totalRows > pageSize) {
      pager.replaceChildren();
      const totalPages = Math.ceil(totalRows / pageSize);
      style(pager, {
        display: "flex",
        justifyContent: "space-between",
        alignItems: "center",
        padding: "12px 18px",
        fontSize: "13px",
        color: "var(--dm-text-secondary)",
        background: "transparent",
        borderTop: "1px solid var(--dm-outline)",
      });
      const info = document.createElement("span");
      info.textContent = `Page ${page + 1} of ${totalPages}`;
      const btnWrap = document.createElement("div");
      style(btnWrap, { display: "flex", gap: "8px" });
      const mkBtn = (lbl, disabled, onClick) => {
        const b = document.createElement("button");
        b.type = "button";
        b.textContent = lbl;
        b.disabled = disabled;
        style(b, {
          padding: "6px 14px",
          borderRadius: "999px",
          border: "1px solid var(--dm-outline)",
          background: "var(--dm-surface-gray)",
          color: "var(--dm-text-primary)",
          cursor: disabled ? "default" : "pointer",
          opacity: disabled ? "0.4" : "1",
          fontSize: "12.5px",
          fontWeight: "500",
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
  return { element: wrap, toolbar };
};
