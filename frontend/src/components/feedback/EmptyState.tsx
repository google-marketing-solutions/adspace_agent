"use client";

import { type ReactNode } from "react";
import {
  Card,
  CardContent,
} from "@/components/ui/card";
import { cn } from "@/lib/utils";

interface EmptyStateProps {
  icon: ReactNode;
  title: string;
  description: string;
  action?: ReactNode;
  className?: string;
}

export default function EmptyState({
  icon,
  title,
  description,
  action,
  className,
}: EmptyStateProps) {
  return (
    <Card className={cn("p-12 text-center", className)}>
      <CardContent className="flex flex-col items-center p-0">
        <div className="mb-4 text-muted-foreground/40">{icon}</div>
        <h3 className="text-base font-semibold text-foreground">
          {title}
        </h3>
        <p className="text-sm text-muted-foreground mt-1 max-w-sm mx-auto">
          {description}
        </p>
        {action && <div className="mt-4">{action}</div>}
      </CardContent>
    </Card>
  );
}

export { EmptyState };
export type { EmptyStateProps };
