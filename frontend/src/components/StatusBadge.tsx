import { cn, statusColor } from "@/lib/utils";

interface StatusBadgeProps {
  status: string;
  className?: string;
}

export default function StatusBadge({ status, className }: StatusBadgeProps) {
  const label = status ?? "unknown";
  return (
    <span
      className={cn(
        "inline-flex items-center px-2.5 py-0.5 rounded-full text-xs font-medium capitalize",
        statusColor(label),
        className
      )}
    >
      {label.toLowerCase()}
    </span>
  );
}
