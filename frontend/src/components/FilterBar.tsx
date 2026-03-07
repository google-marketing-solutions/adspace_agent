"use client";

import { type ReactNode } from "react";
import { X, ChevronDown } from "lucide-react";
import { cn } from "@/lib/utils";
import { useState, useRef, useEffect } from "react";

/* ── Filter Chip ───────────────────────────────────────────────────── */

interface FilterChipProps {
  label: string;
  value?: string;
  active?: boolean;
  onRemove?: () => void;
  onClick?: () => void;
}

export function FilterChip({ label, value, active, onRemove, onClick }: FilterChipProps) {
  return (
    <button
      onClick={onClick}
      className={cn(
        "inline-flex items-center gap-1.5 px-3 py-1.5 rounded-lg text-xs font-medium transition-all border",
        active
          ? "bg-indigo-50 border-indigo-200 text-indigo-700"
          : "bg-white border-gray-200 text-gray-600 hover:border-gray-300 hover:bg-gray-50"
      )}
    >
      <span className="text-gray-400">{label}:</span>
      <span className={active ? "text-indigo-700" : "text-gray-700"}>{value || "All"}</span>
      {active && onRemove && (
        <X
          className="w-3 h-3 ml-0.5 text-indigo-400 hover:text-indigo-600"
          onClick={(e) => {
            e.stopPropagation();
            onRemove();
          }}
        />
      )}
      {!active && <ChevronDown className="w-3 h-3 text-gray-400" />}
    </button>
  );
}

/* ── Select Dropdown ───────────────────────────────────────────────── */

interface SelectFilterProps {
  label: string;
  value: string;
  options: { value: string; label: string; count?: number }[];
  onChange: (value: string) => void;
  allLabel?: string;
}

export function SelectFilter({ label, value, options, onChange, allLabel = "All" }: SelectFilterProps) {
  const [open, setOpen] = useState(false);
  const ref = useRef<HTMLDivElement>(null);

  useEffect(() => {
    function handleClick(e: MouseEvent) {
      if (ref.current && !ref.current.contains(e.target as Node)) setOpen(false);
    }
    document.addEventListener("mousedown", handleClick);
    return () => document.removeEventListener("mousedown", handleClick);
  }, []);

  const active = value !== "";
  const selectedLabel = options.find((o) => o.value === value)?.label ?? allLabel;

  return (
    <div className="relative" ref={ref}>
      <FilterChip
        label={label}
        value={active ? selectedLabel : allLabel}
        active={active}
        onRemove={active ? () => onChange("") : undefined}
        onClick={() => setOpen(!open)}
      />
      {open && (
        <div className="absolute top-full left-0 mt-1 z-30 bg-white border border-gray-200 rounded-xl shadow-lg py-1 min-w-[180px] max-w-[480px] max-h-60 overflow-y-auto animate-scale-in">
          <button
            onClick={() => { onChange(""); setOpen(false); }}
            className={cn(
              "w-full text-left px-3 py-2 text-sm transition-colors",
              value === "" ? "bg-indigo-50 text-indigo-700 font-medium" : "text-gray-700 hover:bg-gray-50"
            )}
          >
            {allLabel}
          </button>
          {options.map((opt) => (
            <button
              key={opt.value}
              onClick={() => { onChange(opt.value); setOpen(false); }}
              className={cn(
                "w-full text-left px-3 py-2 text-sm transition-colors flex items-center justify-between",
                value === opt.value ? "bg-indigo-50 text-indigo-700 font-medium" : "text-gray-700 hover:bg-gray-50"
              )}
            >
              <span>{opt.label}</span>
              {opt.count !== undefined && (
                <span className="text-xs text-gray-400 ml-2">{opt.count}</span>
              )}
            </button>
          ))}
        </div>
      )}
    </div>
  );
}

/* ── Date Range Filter ─────────────────────────────────────────────── */

interface DateRangeFilterProps {
  value: number;
  onChange: (days: number) => void;
  options?: { value: number; label: string }[];
}

/** Use value `-1` for "Month to Date". The component resolves it to the
 *  number of days since the 1st of the current month before calling onChange. */
export function DateRangeFilter({
  value,
  onChange,
  options = [
    { value: -1, label: "Month to Date" },
    { value: 7, label: "Last 7 days" },
    { value: 14, label: "Last 14 days" },
    { value: 30, label: "Last 30 days" },
    { value: 60, label: "Last 60 days" },
    { value: 90, label: "Last 90 days" },
  ],
}: DateRangeFilterProps) {
  return (
    <select
      value={value}
      onChange={(e) => onChange(Number(e.target.value))}
      className="text-xs font-medium border border-gray-200 rounded-lg px-3 py-1.5 bg-white text-gray-600 focus:outline-none focus:ring-2 focus:ring-indigo-500/30 focus:border-indigo-400 transition-all cursor-pointer hover:border-gray-300"
    >
      {options.map((opt) => (
        <option key={opt.value} value={opt.value}>
          {opt.label}
        </option>
      ))}
    </select>
  );
}

/** Resolve a DateRangeFilter value: -1 → days since 1st of month, else passthrough. */
export function resolveDays(value: number): number {
  if (value === -1) {
    const now = new Date();
    return Math.max(now.getDate(), 1); // day-of-month = days into the month
  }
  return value;
}

/* ── Filter Bar ────────────────────────────────────────────────────── */

interface FilterBarProps {
  children: ReactNode;
  activeCount?: number;
  onClearAll?: () => void;
  className?: string;
}

export function FilterBar({ children, activeCount, onClearAll, className }: FilterBarProps) {
  return (
    <div className={cn("flex items-center gap-2 flex-wrap", className)}>
      {children}
      {activeCount != null && activeCount > 0 && onClearAll && (
        <button
          onClick={onClearAll}
          className="text-xs text-gray-400 hover:text-gray-600 underline underline-offset-2 ml-1 transition-colors"
        >
          Clear all filters
        </button>
      )}
    </div>
  );
}
