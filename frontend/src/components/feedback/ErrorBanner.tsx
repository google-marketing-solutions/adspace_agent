"use client";

import { AlertCircle, X } from "lucide-react";
import {
  Alert,
  AlertDescription,
} from "@/components/ui/alert";
import { Button } from "@/components/ui/button";
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
    <Alert variant="destructive" className={cn("flex items-center", className)}>
      <AlertCircle className="size-4" />
      <AlertDescription className="flex-1">{message}</AlertDescription>
      {onDismiss && (
        <Button
          variant="ghost"
          size="icon-xs"
          onClick={onDismiss}
          className="shrink-0 text-destructive hover:text-destructive/80"
        >
          <X className="size-4" />
          <span className="sr-only">Dismiss</span>
        </Button>
      )}
    </Alert>
  );
}

export { ErrorBanner };
export type { ErrorBannerProps };
