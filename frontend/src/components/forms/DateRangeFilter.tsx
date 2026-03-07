"use client";

import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";

interface DateRangeFilterProps {
  value: number;
  onChange: (days: number) => void;
  options?: { value: number; label: string }[];
}

const DEFAULT_OPTIONS = [
  { value: -1, label: "Month to Date" },
  { value: 7, label: "Last 7 days" },
  { value: 14, label: "Last 14 days" },
  { value: 30, label: "Last 30 days" },
  { value: 60, label: "Last 60 days" },
  { value: 90, label: "Last 90 days" },
];

export function DateRangeFilter({
  value,
  onChange,
  options = DEFAULT_OPTIONS,
}: DateRangeFilterProps) {
  return (
    <Select
      value={String(value)}
      onValueChange={(v) => onChange(Number(v))}
    >
      <SelectTrigger className="h-8 w-fit text-xs font-medium gap-1.5">
        <SelectValue />
      </SelectTrigger>
      <SelectContent>
        {options.map((opt) => (
          <SelectItem key={opt.value} value={String(opt.value)}>
            {opt.label}
          </SelectItem>
        ))}
      </SelectContent>
    </Select>
  );
}

/** Resolve a DateRangeFilter value: -1 → days since 1st of month, else passthrough. */
export function resolveDays(value: number): number {
  if (value === -1) {
    const now = new Date();
    return Math.max(now.getDate(), 1);
  }
  return value;
}
