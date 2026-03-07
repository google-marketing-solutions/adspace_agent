"use client";

import { Badge, badgeVariants } from "@/components/ui/badge";
import { cn } from "@/lib/utils";
import { type VariantProps } from "class-variance-authority";

type BadgeVariant = VariantProps<typeof badgeVariants>["variant"];

/** Map status strings to shadcn badge variants. */
function statusVariant(
  status: string,
): BadgeVariant {
  switch (status.toUpperCase()) {
    case "ENABLED":
    case "APPLIED":
      return "success";
    case "PAUSED":
    case "PENDING":
      return "warning";
    case "REMOVED":
    case "FAILED":
      return "destructive";
    case "DISMISSED":
      return "secondary";
    default:
      return "outline";
  }
}

interface StatusBadgeProps {
  status: string;
  className?: string;
}

export default function StatusBadge({ status, className }: StatusBadgeProps) {
  const label = status ?? "unknown";
  return (
    <Badge
      variant={statusVariant(label)}
      className={cn("capitalize", className)}
    >
      {label.toLowerCase()}
    </Badge>
  );
}

export { StatusBadge, statusVariant };
export type { StatusBadgeProps };
