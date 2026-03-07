"use client";

import * as React from "react";
import { cn } from "@/lib/utils";
import { Card, CardContent } from "@/components/ui/card";
import { ArrowUpRight, ArrowDownRight } from "lucide-react";

interface MetricCardProps {
  title: string;
  value: string | number;
  subtitle?: string;
  icon?: React.ReactNode;
  trend?: { value: number; label?: string };
  className?: string;
}

export default function MetricCard({
  title,
  value,
  subtitle,
  icon,
  trend,
  className,
}: MetricCardProps) {
  return (
    <Card className={cn("py-4 gap-0", className)}>
      <CardContent className="px-5 py-0">
        <div className="flex items-start justify-between">
          <div className="space-y-1">
            <p className="text-sm font-medium text-muted-foreground">{title}</p>
            <p className="text-2xl font-bold text-foreground">{value}</p>
            {subtitle && (
              <p className="text-xs text-muted-foreground/70">{subtitle}</p>
            )}
          </div>
          {icon && (
            <div className="p-2 bg-primary/10 text-primary rounded-lg">
              {icon}
            </div>
          )}
        </div>
        {trend && (
          <div className="mt-3 flex items-center gap-1">
            {trend.value >= 0 ? (
              <ArrowUpRight className="size-3.5 text-emerald-500" />
            ) : (
              <ArrowDownRight className="size-3.5 text-destructive" />
            )}
            <span
              className={cn(
                "text-xs font-semibold",
                trend.value >= 0 ? "text-emerald-600" : "text-destructive",
              )}
            >
              {trend.value >= 0 ? "+" : ""}
              {trend.value.toFixed(1)}%
            </span>
            {trend.label && (
              <span className="text-xs text-muted-foreground">
                {trend.label}
              </span>
            )}
          </div>
        )}
      </CardContent>
    </Card>
  );
}
