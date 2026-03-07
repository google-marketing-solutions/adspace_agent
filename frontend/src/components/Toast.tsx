"use client";

import { useEffect, useState } from "react";
import { CheckCircle2, XCircle, AlertTriangle, Info, X } from "lucide-react";

export type ToastVariant = "success" | "error" | "warning" | "info";

export interface ToastMessage {
  id: string;
  variant: ToastVariant;
  title: string;
  description?: string;
}

const ICONS: Record<ToastVariant, React.ReactNode> = {
  success: <CheckCircle2 className="w-5 h-5 text-green-500 shrink-0" />,
  error: <XCircle className="w-5 h-5 text-red-500 shrink-0" />,
  warning: <AlertTriangle className="w-5 h-5 text-amber-500 shrink-0" />,
  info: <Info className="w-5 h-5 text-blue-500 shrink-0" />,
};

const BG: Record<ToastVariant, string> = {
  success: "border-green-200 bg-green-50",
  error: "border-red-200 bg-red-50",
  warning: "border-amber-200 bg-amber-50",
  info: "border-blue-200 bg-blue-50",
};

function ToastItem({
  toast,
  onDismiss,
}: {
  toast: ToastMessage;
  onDismiss: (id: string) => void;
}) {
  const [exiting, setExiting] = useState(false);

  useEffect(() => {
    const timer = setTimeout(() => setExiting(true), 4500);
    const remove = setTimeout(() => onDismiss(toast.id), 5000);
    return () => {
      clearTimeout(timer);
      clearTimeout(remove);
    };
  }, [toast.id, onDismiss]);

  return (
    <div
      className={`flex items-start gap-3 px-4 py-3 rounded-lg border shadow-lg transition-all duration-300 ${
        BG[toast.variant]
      } ${exiting ? "opacity-0 translate-x-4" : "opacity-100 translate-x-0"}`}
    >
      {ICONS[toast.variant]}
      <div className="flex-1 min-w-0">
        <p className="text-sm font-semibold text-gray-900">{toast.title}</p>
        {toast.description && (
          <p className="text-xs text-gray-600 mt-0.5">{toast.description}</p>
        )}
      </div>
      <button
        onClick={() => onDismiss(toast.id)}
        className="text-gray-400 hover:text-gray-600 shrink-0"
      >
        <X className="w-4 h-4" />
      </button>
    </div>
  );
}

/**
 * Toast container — renders in the top-right corner.
 * Pass `toasts` array and `onDismiss` handler.
 */
export function ToastContainer({
  toasts,
  onDismiss,
}: {
  toasts: ToastMessage[];
  onDismiss: (id: string) => void;
}) {
  if (toasts.length === 0) return null;

  return (
    <div className="fixed top-4 right-4 z-[100] flex flex-col gap-2 w-96 max-w-[calc(100vw-2rem)]">
      {toasts.map((t) => (
        <ToastItem key={t.id} toast={t} onDismiss={onDismiss} />
      ))}
    </div>
  );
}

/** Helper to create a toast message with an auto-generated ID. */
export function createToast(
  variant: ToastVariant,
  title: string,
  description?: string
): ToastMessage {
  return {
    id: `${Date.now()}-${Math.random().toString(36).slice(2, 7)}`,
    variant,
    title,
    description,
  };
}
