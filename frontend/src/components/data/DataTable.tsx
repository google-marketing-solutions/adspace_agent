"use client";

import * as React from "react";
import {
  type ColumnDef,
  type SortingState,
  type ColumnFiltersState,
  type VisibilityState,
  type RowSelectionState,
  flexRender,
  getCoreRowModel,
  getFilteredRowModel,
  getPaginationRowModel,
  getSortedRowModel,
  useReactTable,
} from "@tanstack/react-table";
import {
  Search,
  X,
  Download,
  ChevronLeft,
  ChevronRight,
  ChevronsLeft,
  ChevronsRight,
  ArrowUpDown,
  ArrowUp,
  ArrowDown,
  Columns3,
} from "lucide-react";
import { cn } from "@/lib/utils";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Checkbox } from "@/components/ui/checkbox";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import {
  DropdownMenu,
  DropdownMenuCheckboxItem,
  DropdownMenuContent,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import {
  Tooltip,
  TooltipContent,
  TooltipProvider,
  TooltipTrigger,
} from "@/components/ui/tooltip";

/* ── Legacy Column type for backward compat ─────────────────────── */

export interface Column<T> {
  key: string;
  header: string;
  render?: (row: T) => React.ReactNode;
  sortable?: boolean;
  sortValue?: (row: T) => number | string;
  filterable?: boolean;
  className?: string;
  headerClassName?: string;
  hidden?: boolean;
  width?: string;
}

/** Convert old Column<T> definitions to TanStack ColumnDef<T> */
export function legacyColumnsToColumnDefs<T extends Record<string, unknown>>(
  columns: Column<T>[],
): ColumnDef<T>[] {
  return columns.map((col) => ({
    id: col.key,
    accessorKey: col.key,
    header: ({ column }) => {
      if (col.sortable === false) {
        return <span className={col.headerClassName}>{col.header}</span>;
      }
      return (
        <button
          className={cn(
            "flex items-center gap-1 text-xs font-semibold uppercase tracking-wider select-none hover:text-foreground transition-colors -ml-2 px-2 py-1 rounded-md hover:bg-muted/50",
            col.headerClassName,
          )}
          onClick={() => column.toggleSorting(column.getIsSorted() === "asc")}
        >
          {col.header}
          {column.getIsSorted() === "asc" ? (
            <ArrowUp className="size-3.5 text-primary" />
          ) : column.getIsSorted() === "desc" ? (
            <ArrowDown className="size-3.5 text-primary" />
          ) : (
            <ArrowUpDown className="size-3.5 opacity-30" />
          )}
        </button>
      );
    },
    cell: ({ row }) =>
      col.render
        ? col.render(row.original)
        : String(row.original[col.key] ?? ""),
    enableSorting: col.sortable !== false,
    sortingFn: col.sortValue
      ? (rowA, rowB) => {
          const a = col.sortValue!(rowA.original);
          const b = col.sortValue!(rowB.original);
          if (typeof a === "number" && typeof b === "number") return a - b;
          return String(a).localeCompare(String(b));
        }
      : "auto",
    enableHiding: true,
    meta: { width: col.width, className: col.className },
    size: col.width ? parseInt(col.width) : undefined,
  }));
}

/* ── Props ───────────────────────────────────────────────────────── */

interface DataTableProps<T> {
  columns: ColumnDef<T>[];
  data: T[];
  keyField?: string;
  onRowClick?: (row: T) => void;
  emptyMessage?: string;
  emptyIcon?: React.ReactNode;
  loading?: boolean;
  searchable?: boolean;
  searchPlaceholder?: string;
  searchKeys?: string[];
  pageSize?: number;
  pageSizes?: number[];
  exportable?: boolean;
  exportFileName?: string;
  selectable?: boolean;
  onSelectionChange?: (selectedRows: T[]) => void;
  bulkActions?: React.ReactNode;
  headerExtra?: React.ReactNode;
  className?: string;
}

/* ── Component ───────────────────────────────────────────────────── */

export default function DataTable<T extends Record<string, unknown>>({
  columns,
  data,
  keyField = "id",
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
  selectable = false,
  onSelectionChange,
  bulkActions,
  headerExtra,
  className,
}: DataTableProps<T>) {
  const [sorting, setSorting] = React.useState<SortingState>([]);
  const [columnFilters, setColumnFilters] =
    React.useState<ColumnFiltersState>([]);
  const [columnVisibility, setColumnVisibility] =
    React.useState<VisibilityState>({});
  const [rowSelection, setRowSelection] = React.useState<RowSelectionState>({});
  const [globalFilter, setGlobalFilter] = React.useState("");

  // Prepend selection column if selectable
  const allColumns = React.useMemo(() => {
    if (!selectable) return columns;
    const selectCol: ColumnDef<T> = {
      id: "select",
      header: ({ table }) => (
        <Checkbox
          checked={
            table.getIsAllPageRowsSelected() ||
            (table.getIsSomePageRowsSelected() && "indeterminate")
          }
          onCheckedChange={(value) =>
            table.toggleAllPageRowsSelected(!!value)
          }
          aria-label="Select all"
          className="translate-y-[2px]"
        />
      ),
      cell: ({ row }) => (
        <Checkbox
          checked={row.getIsSelected()}
          onCheckedChange={(value) => row.toggleSelected(!!value)}
          aria-label="Select row"
          className="translate-y-[2px]"
          onClick={(e) => e.stopPropagation()}
        />
      ),
      enableSorting: false,
      enableHiding: false,
      size: 40,
    };
    return [selectCol, ...columns];
  }, [columns, selectable]);

  // Global fuzzy filter
  const globalFilterFn = React.useCallback(
    (row: { original: T }, _columnId: string, filterValue: string) => {
      if (!filterValue) return true;
      const q = filterValue.toLowerCase();
      const keys =
        searchKeys.length > 0
          ? searchKeys
          : columns
              .map((c) => ("accessorKey" in c ? String(c.accessorKey) : c.id))
              .filter(Boolean) as string[];
      return keys.some((k) => {
        const val = (row.original as Record<string, unknown>)[k];
        return val != null && String(val).toLowerCase().includes(q);
      });
    },
    [searchKeys, columns],
  );

  const table = useReactTable({
    data,
    columns: allColumns,
    state: {
      sorting,
      columnFilters,
      columnVisibility,
      rowSelection,
      globalFilter,
    },
    onSortingChange: setSorting,
    onColumnFiltersChange: setColumnFilters,
    onColumnVisibilityChange: setColumnVisibility,
    onRowSelectionChange: setRowSelection,
    onGlobalFilterChange: setGlobalFilter,
    getCoreRowModel: getCoreRowModel(),
    getSortedRowModel: getSortedRowModel(),
    getFilteredRowModel: getFilteredRowModel(),
    getPaginationRowModel: getPaginationRowModel(),
    globalFilterFn,
    getRowId: (row) => String((row as Record<string, unknown>)[keyField]),
    initialState: {
      pagination: { pageSize: initialPageSize },
    },
  });

  // Notify parent of selection changes
  React.useEffect(() => {
    if (!onSelectionChange) return;
    const selectedRows = table
      .getSelectedRowModel()
      .rows.map((r) => r.original);
    onSelectionChange(selectedRows);
  }, [rowSelection, onSelectionChange, table]);

  /* ── CSV Export ─────────────────────────────────────────────────── */
  const handleExport = React.useCallback(() => {
    const visibleCols = table.getVisibleFlatColumns().filter((c) => c.id !== "select");
    const headers = visibleCols.map((c) => typeof c.columnDef.header === "string" ? c.columnDef.header : c.id);
    const rows = table.getFilteredRowModel().rows.map((row) =>
      visibleCols.map((col) => {
        const val = (row.original as Record<string, unknown>)[col.id];
        return val != null ? String(val) : "";
      }),
    );
    const csv = [headers, ...rows]
      .map((r) => r.map((c) => `"${c}"`).join(","))
      .join("\n");
    const blob = new Blob([csv], { type: "text/csv" });
    const url = URL.createObjectURL(blob);
    const a = document.createElement("a");
    a.href = url;
    a.download = `${exportFileName}.csv`;
    a.click();
    URL.revokeObjectURL(url);
  }, [table, exportFileName]);

  /* ── Loading skeleton ──────────────────────────────────────────── */
  if (loading) {
    return (
      <div
        className={cn(
          "rounded-xl border bg-card shadow-sm overflow-hidden",
          className,
        )}
      >
        <div className="p-4 border-b flex gap-4">
          <div className="h-9 w-64 bg-muted rounded-lg animate-pulse" />
          <div className="h-9 w-24 bg-muted rounded-lg animate-pulse" />
        </div>
        {Array.from({ length: 8 }).map((_, i) => (
          <div key={i} className="flex gap-4 px-4 py-3.5 border-b border-muted/30">
            {Array.from({ length: 5 }).map((_, j) => (
              <div
                key={j}
                className={cn(
                  "h-4 bg-muted rounded animate-pulse",
                  j === 0 ? "w-48" : "w-20",
                )}
              />
            ))}
          </div>
        ))}
      </div>
    );
  }

  const selectedCount = table.getFilteredSelectedRowModel().rows.length;

  return (
    <div
      className={cn(
        "rounded-xl border bg-card shadow-sm overflow-hidden",
        className,
      )}
    >
      {/* ── Toolbar ──────────────────────────────────────────────── */}
      <div className="flex items-center gap-3 p-3 border-b bg-muted/30">
        {searchable && (
          <div className="relative flex-1 max-w-sm">
            <Search className="absolute left-3 top-1/2 -translate-y-1/2 size-4 text-muted-foreground" />
            <Input
              value={globalFilter}
              onChange={(e) => setGlobalFilter(e.target.value)}
              placeholder={searchPlaceholder}
              className="pl-9 pr-8 h-9 bg-background"
            />
            {globalFilter && (
              <button
                onClick={() => setGlobalFilter("")}
                className="absolute right-2.5 top-1/2 -translate-y-1/2 text-muted-foreground hover:text-foreground"
              >
                <X className="size-3.5" />
              </button>
            )}
          </div>
        )}

        {/* Bulk actions */}
        {selectable && selectedCount > 0 && (
          <div className="flex items-center gap-2 px-3 py-1.5 bg-primary/10 border border-primary/20 rounded-lg">
            <span className="text-xs font-medium text-primary">
              {selectedCount} selected
            </span>
            {bulkActions}
            <button
              onClick={() => table.resetRowSelection()}
              className="text-primary/60 hover:text-primary ml-1"
            >
              <X className="size-3.5" />
            </button>
          </div>
        )}

        <div className="flex items-center gap-1.5 ml-auto">
          {headerExtra}

          {/* Column visibility */}
          <TooltipProvider delayDuration={300}>
            <DropdownMenu>
              <Tooltip>
                <TooltipTrigger asChild>
                  <DropdownMenuTrigger asChild>
                    <Button variant="ghost" size="icon" className="size-8">
                      <Columns3 className="size-4" />
                    </Button>
                  </DropdownMenuTrigger>
                </TooltipTrigger>
                <TooltipContent>Toggle columns</TooltipContent>
              </Tooltip>
              <DropdownMenuContent align="end" className="w-48">
                {table
                  .getAllColumns()
                  .filter((c) => c.getCanHide())
                  .map((column) => (
                    <DropdownMenuCheckboxItem
                      key={column.id}
                      className="capitalize text-sm"
                      checked={column.getIsVisible()}
                      onCheckedChange={(value) =>
                        column.toggleVisibility(!!value)
                      }
                    >
                      {typeof column.columnDef.header === "string"
                        ? column.columnDef.header
                        : column.id.replace(/_/g, " ")}
                    </DropdownMenuCheckboxItem>
                  ))}
              </DropdownMenuContent>
            </DropdownMenu>
          </TooltipProvider>

          {exportable && (
            <TooltipProvider delayDuration={300}>
              <Tooltip>
                <TooltipTrigger asChild>
                  <Button
                    variant="ghost"
                    size="icon"
                    className="size-8"
                    onClick={handleExport}
                  >
                    <Download className="size-4" />
                  </Button>
                </TooltipTrigger>
                <TooltipContent>Export CSV</TooltipContent>
              </Tooltip>
            </TooltipProvider>
          )}
        </div>
      </div>

      {/* ── Table ────────────────────────────────────────────────── */}
      <Table>
        <TableHeader>
          {table.getHeaderGroups().map((headerGroup) => (
            <TableRow key={headerGroup.id} className="bg-muted/30 hover:bg-muted/30">
              {headerGroup.headers.map((header) => {
                const meta = header.column.columnDef.meta as
                  | { width?: string; className?: string }
                  | undefined;
                return (
                  <TableHead
                    key={header.id}
                    style={meta?.width ? { width: meta.width } : undefined}
                    className={cn("text-xs text-muted-foreground", meta?.className)}
                  >
                    {header.isPlaceholder
                      ? null
                      : flexRender(
                          header.column.columnDef.header,
                          header.getContext(),
                        )}
                  </TableHead>
                );
              })}
            </TableRow>
          ))}
        </TableHeader>
        <TableBody>
          {table.getRowModel().rows?.length ? (
            table.getRowModel().rows.map((row) => (
              <TableRow
                key={row.id}
                data-state={row.getIsSelected() && "selected"}
                className={cn(onRowClick && "cursor-pointer")}
                onClick={() => onRowClick?.(row.original)}
              >
                {row.getVisibleCells().map((cell) => {
                  const meta = cell.column.columnDef.meta as
                    | { className?: string }
                    | undefined;
                  return (
                    <TableCell key={cell.id} className={cn("text-sm", meta?.className)}>
                      {flexRender(
                        cell.column.columnDef.cell,
                        cell.getContext(),
                      )}
                    </TableCell>
                  );
                })}
              </TableRow>
            ))
          ) : (
            <TableRow>
              <TableCell
                colSpan={allColumns.length}
                className="h-32 text-center"
              >
                {emptyIcon && (
                  <div className="flex justify-center mb-3 text-muted-foreground/40">
                    {emptyIcon}
                  </div>
                )}
                <p className="text-sm text-muted-foreground">{emptyMessage}</p>
              </TableCell>
            </TableRow>
          )}
        </TableBody>
      </Table>

      {/* ── Pagination ───────────────────────────────────────────── */}
      {table.getFilteredRowModel().rows.length > 0 && (
        <div className="flex items-center justify-between px-4 py-3 border-t bg-muted/20 text-xs text-muted-foreground">
          <div className="flex items-center gap-4">
            <span>
              Showing{" "}
              {table.getState().pagination.pageIndex *
                table.getState().pagination.pageSize +
                1}
              –
              {Math.min(
                (table.getState().pagination.pageIndex + 1) *
                  table.getState().pagination.pageSize,
                table.getFilteredRowModel().rows.length,
              )}{" "}
              of {table.getFilteredRowModel().rows.length}
              {table.getFilteredRowModel().rows.length !== data.length &&
                ` (filtered from ${data.length})`}
            </span>
            <div className="flex items-center gap-1.5">
              <span>Rows:</span>
              <Select
                value={String(table.getState().pagination.pageSize)}
                onValueChange={(value) => table.setPageSize(Number(value))}
              >
                <SelectTrigger className="h-7 w-[60px] text-xs">
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  {pageSizes.map((size) => (
                    <SelectItem key={size} value={String(size)}>
                      {size}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
            </div>
          </div>
          <div className="flex items-center gap-1">
            <Button
              variant="ghost"
              size="icon"
              className="size-7"
              onClick={() => table.setPageIndex(0)}
              disabled={!table.getCanPreviousPage()}
            >
              <ChevronsLeft className="size-4" />
            </Button>
            <Button
              variant="ghost"
              size="icon"
              className="size-7"
              onClick={() => table.previousPage()}
              disabled={!table.getCanPreviousPage()}
            >
              <ChevronLeft className="size-4" />
            </Button>
            <span className="px-2 text-xs font-medium">
              Page {table.getState().pagination.pageIndex + 1} of{" "}
              {table.getPageCount()}
            </span>
            <Button
              variant="ghost"
              size="icon"
              className="size-7"
              onClick={() => table.nextPage()}
              disabled={!table.getCanNextPage()}
            >
              <ChevronRight className="size-4" />
            </Button>
            <Button
              variant="ghost"
              size="icon"
              className="size-7"
              onClick={() => table.setPageIndex(table.getPageCount() - 1)}
              disabled={!table.getCanNextPage()}
            >
              <ChevronsRight className="size-4" />
            </Button>
          </div>
        </div>
      )}
    </div>
  );
}
