"use client";

import { toast } from "sonner";

export type ToastVariant = "success" | "error" | "warning" | "info";

/**
 * Backward-compatible toast hook.
 * Maps the old `addToast(variant, title, description?)` API to Sonner.
 */
export function useToast() {
  const addToast = (
    variant: ToastVariant,
    title: string,
    description?: string,
  ) => {
    const options = description ? { description } : undefined;
    switch (variant) {
      case "success":
        toast.success(title, options);
        break;
      case "error":
        toast.error(title, options);
        break;
      case "warning":
        toast.warning(title, options);
        break;
      case "info":
        toast.info(title, options);
        break;
      default:
        toast(title, options);
    }
  };

  const dismissToast = (id?: string | number) => {
    toast.dismiss(id);
  };

  return { addToast, dismissToast };
}
