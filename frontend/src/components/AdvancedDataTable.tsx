"use client";

import { useState, useMemo, useCallback, type ReactNode } from "react";
import {
  ChevronUp,
  ChevronDown,
  ChevronsUpDown,
  Search,
  ChevronLeft,
  ChevronRight,
  Download,
  Columns3,
  X,
} from "lucide-react";
import { cn } from "@/lib/utils";

/* ── Types ─────────────────────────────────────────────────────────── */

export interface Column<T> {
  key: string;
  header: string;
  render?: (row: T) => ReactNode;
  sortable?: boolean;
  sortValue?: (row: T) => number | string;
  filterable?: boolean;
  className?: string;
  headerClassName?: string;
  hidden?: boolean;
  width?: string;
}

interface AdvancedDataTableProps<T> {
  columns: Column<T>[];
  data: T[];
  keyField: string;
  onRowClick?: (row: T) => void;
  emptyMessage?: string;
  emptyIcon?: ReactNode;
  loading?: boolean;
  searchable?: boolean;
  searchPlaceholder?: string;
  searchKeys?: string[];
  pageSize?: number;
  pageSizes?: number[];
  exportable?: boolean;
  exportFileName?: string;
  stickyHeader?: boolean;
  selectable?: boolean;
  onSelectionChange?: (selectedRows: T[]) => void;
  bulkActions?: ReactNode;
  headerExtra?: ReactNode;
  className?: string;
}

type SortDir = "asc" | "desc" | null;

/* ── Component ─────────────────────────────────────────────────────── */

export default function AdvancedDataTable<T extends Record<string, unknown>>({
  columns: columnsProp,
  data,
  keyField,
  onRowClick,
  emptyMessage = "No data available",
  emptyIcon,
  loading = false,
  searchable = true,
  searchPlaceholder = "Search...",
  searchKeys = [],
  pageSize: initialPageSize = 25,
  pageSizes = [10, 25, 50, 100],
  exportable = false,
  exportFileName = "export",
  stickyHeader = true,
  selectable = false,
  onSelectionChange,
  bulkActions,
  headerExtra,
  className,
}: AdvancedDataTableProps<T>) {
  const [search, setSearch] = useState("");
  const [sortKey, setSortKey] = useState<string | null>(null);
  const [sortDir, setSortDir] = useState<SortDir>(null);
  const [page, setPage] = useState(0);
  const [pageSize, setPageSize] = useState(initialPageSize);
  const [selectedKeys, setSelectedKeys] = useState<Set<string>>(new Set());
  const [hiddenCols, setHiddenCols] = useState<Set<string>>(
    new Set(columnsProp.filter((c) => c.hidden).map((c) => c.key))
  );
  const [showColPicker, setShowColPicker] = useState(false);

  const visibleColumns = columnsProp.filter((c) => !hiddenCols.has(c.key));

  /* ── Search ──────────────────────────────────────────────────────── */
  const filtered = useMemo(() => {
    if (!search.trim()) return data;
    const q = search.toLowerCase();
    const keys = searchKeys.length > 0 ? searchKeys : columnsProp.map((c) => c.key);
    return data.filter((row) =>
      keys.some((k) => {
        const val = row[k];
        if (val == null) return false;
        return String(val).toLowerCase().includes(q);
      })
    );
  }, [data, search, searchKeys, columnsProp]);

  /* ── Sort ────────────────────────────────────────────────────────── */
  const sorted = useMemo(() => {
    if (!sortKey || !sortDir) return filtered;
    const col = columnsProp.find((c) => c.key === sortKey);
    return [...filtered].sort((a, b) => {
      const va = col?.sortValue ? col.sortValue(a) : (a[sortKey] ?? "");
      const vb = col?.sortValue ? col.sortValue(b) : (b[sortKey] ?? "");
      if (typeof va === "number" && typeof vb === "number") {
        return sortDir === "asc" ? va - vb : vb - va;
      }
      const sa = String(va).toLowerCase();
      const sb = String(vb).toLowerCase();
      return sortDir === "asc" ? sa.localeCompare(sb) : sb.localeCompare(sa);
    });
  }, [filtered, sortKey, sortDir, columnsProp]);

  /* ── Paginate ────────────────────────────────────────────────────── */
  const totalPages = Math.max(1, Math.ceil(sorted.length / pageSize));
  const safePage = Math.min(page, totalPages - 1);
  const paged = sorted.slice(safePage * pageSize, (safePage + 1) * pageSize);

  /* ── Handlers ────────────────────────────────────────────────────── */
  const handleSort = useCallback(
    (key: string) => {
      if (sortKey === key) {
        setSortDir((d) => (d === "asc" ? "desc" : d === "desc" ? null : "asc"));
        if (sortDir === "desc") setSortKey(null);
      } else {
        setSortKey(key);
        setSortDir("asc");
      }
      setPage(0);
    },
    [sortKey, sortDir]
  );

  const toggleSelect = useCallback(
    (key: string) => {
      setSelectedKeys((prev) => {
        const next = new Set(prev);
        if (next.has(key)) next.delete(key);
        else next.add(key);
        return next;
      });
    },
    []
  );

  const toggleSelectAll = useCallback(() => {
    if (selectedKeys.size === paged.length) {
      setSelectedKeys(new Set());
    } else {
      setSelectedKeys(new Set(paged.map((r) => String(r[keyField]))));
    }
  }, [selectedKeys.size, paged, keyField]);

  // Notify parent of selection changes
  const selectedRows = useMemo(
    () => data.filter((r) => selectedKeys.has(String(r[keyField]))),
    [data, selectedKeys, keyField]
  );
  useMemo(() => onSelectionChange?.(selectedRows), [selectedRows, onSelectionChange]);

  /* ── Export CSV ──────────────────────────────────────────────────── */
  const handleExport = useCallback(() => {
    const headers = visibleColumns.map((c) => c.header);
    const rows = sorted.map((row) =>
      visibleColumns.map((c) => {
        const val = row[c.key];
        return val != null ? String(val) : "";
      })
    );
    const csv = [headers, ...rows].map((r) => r.map((c) => `"${c}"`).join(",")).join("\n");
    const blob = new Blob([csv], { type: "text/csv" });
    const url = URL.createObjectURL(blob);
    const a = document.createElement("a");
    a.href = url;
    a.download = `${exportFileName}.csv`;
    a.click();
    URL.revokeObjectURL(url);
  }, [sorted, visibleColumns, exportFileName]);

  /* ── Loading skeleton ────────────────────────────────────────────── */
  if (loading) {
    return (
      <div className={cn("bg-white rounded-xl border border-gray-200/80 shadow-sm overflow-hidden", className)}>
        <div className="p-4 border-b border-gray-100 flex gap-4">
          <div className="h-9 w-64 bg-gray-100 rounded-lg animate-shimmer" />
          <div className="h-9 w-24 bg-gray-100 rounded-lg animate-shimmer" />
        </div>
        {Array.from({ length: 8 }).map((_, i) => (
          <div key={i} className="flex gap-4 px-4 py-3.5 border-b border-gray-50">
            {Array.from({ length: 5 }).map((_, j) => (
              <div key={j} className={cn("h-4 bg-gray-100 rounded animate-shimmer", j === 0 ? "w-48" : "w-20")} />
            ))}
          </div>
        ))}
      </div>
    );
  }

  return (
    <div className={cn("bg-white rounded-xl border border-gray-200/80 shadow-sm overflow-hidden", className)}>
      {/* ── Toolbar ──────────────────────────────────────────────────── */}
      <div className="flex items-center gap-3 p-3 border-b border-gray-100 bg-gray-50/50">
        {searchable && (
          <div className="relative flex-1 max-w-sm">
            <Search className="absolute left-3 top-1/2 -translate-y-1/2 w-4 h-4 text-gray-400" />
            <input
              type="text"
              value={search}
              onChange={(e) => { setSearch(e.target.value); setPage(0); }}
              placeholder={searchPlaceholder}
              className="w-full pl-9 pr-8 py-2 text-sm border border-gray-200 rounded-lg bg-white focus:outline-none focus:ring-2 focus:ring-indigo-500/30 focus:border-indigo-400 transition-all"
            />
            {search && (
              <button onClick={() => setSearch("")} className="absolute right-2.5 top-1/2 -translate-y-1/2 text-gray-400 hover:text-gray-600">
                <X className="w-3.5 h-3.5" />
              </button>
            )}
          </div>
        )}

        {/* Bulk actions area */}
        {selectable && selectedKeys.size > 0 && (
          <div className="flex items-center gap-2 px-3 py-1.5 bg-indigo-50 border border-indigo-200 rounded-lg">
            <span className="text-xs font-medium text-indigo-700">{selectedKeys.size} selected</span>
            {bulkActions}
            <button onClick={() => setSelectedKeys(new Set())} className="text-indigo-400 hover:text-indigo-600 ml-1">
              <X className="w-3.5 h-3.5" />
            </button>
          </div>
        )}

        <div className="flex items-center gap-2 ml-auto">
          {headerExtra}

          {/* Column picker */}
          <div className="relative">
            <button
              onClick={() => setShowColPicker(!showColPicker)}
              className="p-2 text-gray-400 hover:text-gray-600 hover:bg-gray-100 rounded-lg transition-colors"
              title="Toggle columns"
            >
              <Columns3 className="w-4 h-4" />
            </button>
            {showColPicker && (
              <>
                <div className="fixed inset-0 z-10" onClick={() => setShowColPicker(false)} />
                <div className="absolute right-0 top-full mt-1 z-20 bg-white border border-gray-200 rounded-xl shadow-lg py-2 w-48 animate-scale-in">
                  <div className="px-3 py-1.5 text-xs font-semibold text-gray-400 uppercase tracking-wider">Columns</div>
                  {columnsProp.map((col) => (
                    <label key={col.key} className="flex items-center gap-2.5 px-3 py-1.5 hover:bg-gray-50 cursor-pointer text-sm text-gray-700">
                      <input
                        type="checkbox"
                        checked={!hiddenCols.has(col.key)}
                        onChange={() => {
                          setHiddenCols((prev) => {
                            const n = new Set(prev);
                            n.has(col.key) ? n.delete(col.key) : n.add(col.key);
                            return n;
                          });
                        }}
                        className="rounded border-gray-300 text-indigo-600 focus:ring-indigo-500"
                      />
                      {col.header}
                    </label>
                  ))}
                </div>
              </>
            )}
          </div>

          {exportable && (
            <button
              onClick={handleExport}
              className="p-2 text-gray-400 hover:text-gray-600 hover:bg-gray-100 rounded-lg transition-colors"
              title="Export CSV"
            >
              <Download className="w-4 h-4" />
            </button>
          )}
        </div>
      </div>

      {/* ── Table ────────────────────────────────────────────────────── */}
      <div className="overflow-x-auto">
        <table className="w-full">
          <thead className={cn(stickyHeader && "sticky top-0 z-10")}>
            <tr className="border-b border-gray-200 bg-gray-50/80 backdrop-blur-sm">
              {selectable && (
                <th className="w-10 px-3 py-3">
                  <input
                    type="checkbox"
                    checked={paged.length > 0 && selectedKeys.size === paged.length}
                    onChange={toggleSelectAll}
                    className="rounded border-gray-300 text-indigo-600 focus:ring-indigo-500"
                  />
                </th>
              )}
              {visibleColumns.map((col) => {
                const isSorted = sortKey === col.key;
                const isSortable = col.sortable !== false;
                return (
                  <th
                    key={col.key}
                    className={cn(
                      "px-4 py-3 text-left text-xs font-semibold text-gray-500 uppercase tracking-wider select-none",
                      isSortable && "cursor-pointer hover:text-gray-700 transition-colors group",
                      col.headerClassName ?? col.className
                    )}
                    style={col.width ? { width: col.width } : undefined}
                    onClick={() => isSortable && handleSort(col.key)}
                  >
                    <span className="inline-flex items-center gap-1">
                      {col.header}
                      {isSortable && (
                        <span className="inline-flex flex-col -space-y-0.5">
                          {isSorted && sortDir === "asc" ? (
                            <ChevronUp className="w-3.5 h-3.5 text-indigo-600" />
                          ) : isSorted && sortDir === "desc" ? (
                            <ChevronDown className="w-3.5 h-3.5 text-indigo-600" />
                          ) : (
                            <ChevronsUpDown className="w-3.5 h-3.5 opacity-0 group-hover:opacity-50 transition-opacity" />
                          )}
                        </span>
                      )}
                    </span>
                  </th>
                );
              })}
            </tr>
          </thead>
          <tbody className="divide-y divide-gray-50">
            {paged.length === 0 ? (
              <tr>
                <td colSpan={visibleColumns.length + (selectable ? 1 : 0)} className="px-4 py-16 text-center">
                  {emptyIcon && <div className="flex justify-center mb-3 text-gray-300">{emptyIcon}</div>}
                  <p className="text-sm text-gray-400">{emptyMessage}</p>
                </td>
              </tr>
            ) : (
              paged.map((row, idx) => {
                const rowKey = String(row[keyField]);
                const isSelected = selectedKeys.has(rowKey);
                return (
                  <tr
                    key={`${rowKey}-${idx}`}
                    className={cn(
                      "transition-colors group",
                      isSelected ? "bg-indigo-50/50" : "hover:bg-gray-50/80",
                      onRowClick && "cursor-pointer"
                    )}
                    onClick={() => onRowClick?.(row)}
                  >
                    {selectable && (
                      <td className="w-10 px-3 py-3">
                        <input
                          type="checkbox"
                          checked={isSelected}
                          onChange={(e) => { e.stopPropagation(); toggleSelect(rowKey); }}
                          onClick={(e) => e.stopPropagation()}
                          className="rounded border-gray-300 text-indigo-600 focus:ring-indigo-500"
                        />
                      </td>
                    )}
                    {visibleColumns.map((col) => (
                      <td key={col.key} className={cn("px-4 py-3 text-sm text-gray-700", col.className)}>
                        {col.render ? col.render(row) : String(row[col.key] ?? "")}
                      </td>
                    ))}
                  </tr>
                );
              })
            )}
          </tbody>
        </table>
      </div>

      {/* ── Footer / Pagination ──────────────────────────────────────── */}
      {sorted.length > 0 && (
        <div className="flex items-center justify-between px-4 py-3 border-t border-gray-100 bg-gray-50/30 text-xs text-gray-500">
          <div className="flex items-center gap-4">
            <span>
              Showing {safePage * pageSize + 1}–{Math.min((safePage + 1) * pageSize, sorted.length)} of{" "}
              {sorted.length}{filtered.length !== data.length && ` (filtered from ${data.length})`}
            </span>
            <div className="flex items-center gap-1.5">
              <span>Rows:</span>
              <select
                value={pageSize}
                onChange={(e) => { setPageSize(Number(e.target.value)); setPage(0); }}
                className="border border-gray-200 rounded px-1.5 py-0.5 text-xs bg-white focus:outline-none focus:ring-1 focus:ring-indigo-500"
              >
                {pageSizes.map((s) => (
                  <option key={s} value={s}>{s}</option>
                ))}
              </select>
            </div>
          </div>
          <div className="flex items-center gap-1">
            <button
              onClick={() => setPage((p) => Math.max(0, p - 1))}
              disabled={safePage === 0}
              className="p-1.5 rounded hover:bg-gray-200/80 disabled:opacity-30 disabled:cursor-not-allowed transition-colors"
            >
              <ChevronLeft className="w-4 h-4" />
            </button>
            {Array.from({ length: Math.min(totalPages, 7) }).map((_, i) => {
              let pageNum = i;
              if (totalPages > 7) {
                if (safePage < 4) pageNum = i;
                else if (safePage > totalPages - 5) pageNum = totalPages - 7 + i;
                else pageNum = safePage - 3 + i;
              }
              return (
                <button
                  key={pageNum}
                  onClick={() => setPage(pageNum)}
                  className={cn(
                    "w-7 h-7 rounded text-xs font-medium transition-colors",
                    pageNum === safePage
                      ? "bg-indigo-600 text-white shadow-sm"
                      : "hover:bg-gray-200/80 text-gray-600"
                  )}
                >
                  {pageNum + 1}
                </button>
              );
            })}
            <button
              onClick={() => setPage((p) => Math.min(totalPages - 1, p + 1))}
              disabled={safePage >= totalPages - 1}
              className="p-1.5 rounded hover:bg-gray-200/80 disabled:opacity-30 disabled:cursor-not-allowed transition-colors"
            >
              <ChevronRight className="w-4 h-4" />
            </button>
          </div>
        </div>
      )}
    </div>
  );
}
