"use client";

import {
  createContext,
  useCallback,
  useContext,
  useState,
  type ReactNode,
} from "react";
import {
  ToastContainer,
  createToast,
  type ToastMessage,
  type ToastVariant,
} from "@/components/Toast";

interface ToastContextType {
  addToast: (variant: ToastVariant, title: string, description?: string) => void;
  dismissToast: (id: string) => void;
}

const ToastContext = createContext<ToastContextType>({
  addToast: () => {},
  dismissToast: () => {},
});

export function useToast() {
  return useContext(ToastContext);
}

export function ToastProvider({ children }: { children: ReactNode }) {
  const [toasts, setToasts] = useState<ToastMessage[]>([]);

  const addToast = useCallback(
    (variant: ToastVariant, title: string, description?: string) => {
      setToasts((prev) => [...prev, createToast(variant, title, description)]);
    },
    []
  );

  const dismissToast = useCallback(
    (id: string) => setToasts((prev) => prev.filter((t) => t.id !== id)),
    []
  );

  return (
    <ToastContext.Provider value={{ addToast, dismissToast }}>
      {children}
      <ToastContainer toasts={toasts} onDismiss={dismissToast} />
    </ToastContext.Provider>
  );
}
