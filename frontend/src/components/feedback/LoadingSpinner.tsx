"use client";

import { Loader2 } from "lucide-react";
import { cn } from "@/lib/utils";

const sizeMap = {
  sm: "size-4",
  md: "size-8",
  lg: "size-12",
} as const;

interface LoadingSpinnerProps {
  size?: "sm" | "md" | "lg";
  className?: string;
  message?: string;
}

export default function LoadingSpinner({
  size = "md",
  className,
  message,
}: LoadingSpinnerProps) {
  return (
    <div
      className={cn(
        "flex flex-col items-center justify-center gap-3",
        className,
      )}
    >
      <Loader2
        className={cn("animate-spin text-primary", sizeMap[size])}
      />
      {message && (
        <p className="text-sm text-muted-foreground">{message}</p>
      )}
    </div>
  );
}

export { LoadingSpinner };
export type { LoadingSpinnerProps };
