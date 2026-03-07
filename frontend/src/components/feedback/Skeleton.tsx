"use client";

import { Skeleton as ShadcnSkeleton } from "@/components/ui/skeleton";
import { cn } from "@/lib/utils";

/** Re-export the base shadcn Skeleton. */
export { ShadcnSkeleton as Skeleton };

interface TableSkeletonProps {
  rows?: number;
  columns?: number;
}

export function TableSkeleton({
  rows = 8,
  columns = 6,
}: TableSkeletonProps) {
  return (
    <div className="rounded-xl border bg-card shadow-sm overflow-hidden">
      {/* Header */}
      <div className="border-b bg-muted/50 px-4 py-3">
        <div className="flex gap-4">
          {Array.from({ length: columns }).map((_, i) => (
            <ShadcnSkeleton
              key={`h-${i}`}
              className={cn("h-4", i === 0 ? "w-48" : "w-20")}
            />
          ))}
        </div>
      </div>
      {/* Rows */}
      {Array.from({ length: rows }).map((_, rowIdx) => (
        <div
          key={`r-${rowIdx}`}
          className="flex items-center gap-4 px-4 py-3.5 border-b last:border-0"
        >
          {Array.from({ length: columns }).map((_, colIdx) => (
            <ShadcnSkeleton
              key={`c-${rowIdx}-${colIdx}`}
              className={cn(
                "h-4",
                colIdx === 0 ? "w-48" : "w-16",
                rowIdx % 2 === 0 ? "opacity-70" : "opacity-50",
              )}
            />
          ))}
        </div>
      ))}
    </div>
  );
}

export function CardSkeleton() {
  return (
    <div className="rounded-xl border bg-card shadow-sm p-5 space-y-3">
      <ShadcnSkeleton className="h-4 w-24" />
      <ShadcnSkeleton className="h-8 w-32" />
      <ShadcnSkeleton className="h-3 w-20" />
    </div>
  );
}

export function KpiCardsSkeleton({ count = 5 }: { count?: number }) {
  return (
    <div className="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-5 gap-4">
      {Array.from({ length: count }).map((_, i) => (
        <CardSkeleton key={i} />
      ))}
    </div>
  );
}
