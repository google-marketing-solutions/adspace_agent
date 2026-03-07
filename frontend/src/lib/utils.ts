import { clsx, type ClassValue } from "clsx";
import { twMerge } from "tailwind-merge";

/** Merge Tailwind classes safely. */
export function cn(...inputs: ClassValue[]) {
  return twMerge(clsx(inputs));
}

/** Format micros to currency string. */
export function formatCurrency(micros: number): string {
  return `$${(micros / 1_000_000).toLocaleString(undefined, {
    minimumFractionDigits: 2,
    maximumFractionDigits: 2,
  })}`;
}

/** Format a dollar amount (already converted from micros). */
export function formatDollars(amount: number): string {
  return `$${amount.toLocaleString(undefined, {
    minimumFractionDigits: 2,
    maximumFractionDigits: 2,
  })}`;
}

/** Format percentage. */
export function formatPercent(value: number): string {
  return `${value.toFixed(2)}%`;
}

/** Format large numbers with K/M suffixes. */
export function formatNumber(n: number): string {
  if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(1)}M`;
  if (n >= 1_000) return `${(n / 1_000).toFixed(1)}K`;
  return n.toLocaleString();
}

/** Status to color mapping. */
export function statusColor(status: unknown): string {
  switch (String(status ?? "").toUpperCase()) {
    case "ENABLED":
      return "text-green-600 bg-green-50";
    case "PAUSED":
      return "text-yellow-600 bg-yellow-50";
    case "REMOVED":
      return "text-red-600 bg-red-50";
    case "PENDING":
      return "text-blue-600 bg-blue-50";
    case "APPLIED":
      return "text-green-600 bg-green-50";
    case "DISMISSED":
      return "text-gray-500 bg-gray-100";
    case "FAILED":
      return "text-red-600 bg-red-50";
    default:
      return "text-gray-600 bg-gray-100";
  }
}
