"use client";

import { type ReactNode } from "react";
import { cn } from "@/lib/utils";

interface FilterBarProps {
  children: ReactNode;
  activeCount?: number;
  onClearAll?: () => void;
  className?: string;
}

export function FilterBar({
  children,
  activeCount,
  onClearAll,
  className,
}: FilterBarProps) {
  return (
    <div className={cn("flex items-center gap-2 flex-wrap", className)}>
      {children}
      {activeCount != null && activeCount > 0 && onClearAll && (
        <button
          onClick={onClearAll}
          className="text-xs text-muted-foreground hover:text-foreground underline underline-offset-2 ml-1 transition-colors"
        >
          Clear all filters
        </button>
      )}
    </div>
  );
}
