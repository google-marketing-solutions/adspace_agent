import { type ReactNode } from "react";
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
    <div
      className={cn(
        "bg-white rounded-xl border border-gray-200 p-12 text-center",
        className
      )}
    >
      <div className="flex justify-center mb-4 text-gray-300">{icon}</div>
      <h3 className="text-base font-semibold text-gray-700">{title}</h3>
      <p className="text-sm text-gray-500 mt-1 max-w-sm mx-auto">
        {description}
      </p>
      {action && <div className="mt-4">{action}</div>}
    </div>
  );
}
