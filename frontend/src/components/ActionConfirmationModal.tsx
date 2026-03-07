"use client";

import { X, AlertTriangle, Pause, Play, Trash2, type LucideIcon } from "lucide-react";
import Button from "@/components/Button";

export type ActionType = "pause" | "enable" | "delete" | "remove";

interface ActionConfirmationModalProps {
  action: ActionType;
  entityType: string;
  entityName: string;
  entityDetails?: string;
  loading: boolean;
  onConfirm: () => void;
  onCancel: () => void;
}

const ACTION_CONFIG: Record<
  ActionType,
  {
    icon: LucideIcon;
    label: string;
    description: string;
    buttonVariant: "primary" | "danger";
    headerBg: string;
    headerBorder: string;
    iconColor: string;
    titleColor: string;
  }
> = {
  pause: {
    icon: Pause,
    label: "Pause",
    description: "This will pause the {entity}. It will stop serving ads but can be re-enabled later.",
    buttonVariant: "primary",
    headerBg: "bg-amber-50",
    headerBorder: "border-amber-200",
    iconColor: "text-amber-600",
    titleColor: "text-amber-900",
  },
  enable: {
    icon: Play,
    label: "Enable",
    description: "This will enable the {entity}. It will start serving ads again.",
    buttonVariant: "primary",
    headerBg: "bg-green-50",
    headerBorder: "border-green-200",
    iconColor: "text-green-600",
    titleColor: "text-green-900",
  },
  delete: {
    icon: Trash2,
    label: "Delete",
    description: "This will permanently delete the {entity}. This action cannot be undone.",
    buttonVariant: "danger",
    headerBg: "bg-red-50",
    headerBorder: "border-red-200",
    iconColor: "text-red-600",
    titleColor: "text-red-900",
  },
  remove: {
    icon: Trash2,
    label: "Remove",
    description: "This will remove the {entity}. This action cannot be undone.",
    buttonVariant: "danger",
    headerBg: "bg-red-50",
    headerBorder: "border-red-200",
    iconColor: "text-red-600",
    titleColor: "text-red-900",
  },
};

export default function ActionConfirmationModal({
  action,
  entityType,
  entityName,
  entityDetails,
  loading,
  onConfirm,
  onCancel,
}: ActionConfirmationModalProps) {
  const config = ACTION_CONFIG[action];
  const Icon = config.icon;

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/40 backdrop-blur-sm animate-in fade-in duration-200">
      <div className="bg-white rounded-2xl shadow-2xl w-full max-w-md mx-4 overflow-hidden animate-in zoom-in-95 duration-200">
        {/* Header */}
        <div
          className={`flex items-center gap-3 ${config.headerBg} border-b ${config.headerBorder} px-5 py-4`}
        >
          <div className={`p-2 rounded-full ${config.headerBg}`}>
            <Icon className={`w-5 h-5 ${config.iconColor}`} />
          </div>
          <h2 className={`text-base font-semibold ${config.titleColor}`}>
            {config.label} {entityType}?
          </h2>
          <button
            onClick={onCancel}
            className="ml-auto text-gray-400 hover:text-gray-600 transition-colors"
          >
            <X className="w-5 h-5" />
          </button>
        </div>

        {/* Body */}
        <div className="px-5 py-4 space-y-3">
          <div className="bg-gray-50 rounded-lg p-3">
            <p className="text-sm font-semibold text-gray-900">{entityName}</p>
            {entityDetails && (
              <p className="text-xs text-gray-500 mt-0.5">{entityDetails}</p>
            )}
          </div>
          <p className="text-sm text-gray-600">
            {config.description.replace("{entity}", entityType.toLowerCase())}
          </p>
        </div>

        {/* Footer */}
        <div className="flex items-center justify-end gap-3 border-t border-gray-200 px-5 py-4 bg-gray-50">
          <Button variant="ghost" size="sm" onClick={onCancel} disabled={loading}>
            Cancel
          </Button>
          <Button
            variant={config.buttonVariant}
            size="sm"
            loading={loading}
            onClick={onConfirm}
          >
            {config.label} {entityType}
          </Button>
        </div>
      </div>
    </div>
  );
}
