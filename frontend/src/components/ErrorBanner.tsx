import { AlertCircle, X } from "lucide-react";
import { cn } from "@/lib/utils";

interface ErrorBannerProps {
  message: string;
  onDismiss?: () => void;
  className?: string;
}

export default function ErrorBanner({
  message,
  onDismiss,
  className,
}: ErrorBannerProps) {
  return (
    <div
      className={cn(
        "flex items-center gap-3 bg-red-50 border border-red-200 text-red-700 rounded-lg px-4 py-3",
        className
      )}
    >
      <AlertCircle className="w-5 h-5 shrink-0" />
      <p className="text-sm flex-1">{message}</p>
      {onDismiss && (
        <button onClick={onDismiss} className="hover:text-red-900">
          <X className="w-4 h-4" />
        </button>
      )}
    </div>
  );
}
