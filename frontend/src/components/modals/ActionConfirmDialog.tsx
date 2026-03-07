"use client";

import { Pause, Play, Trash2, type LucideIcon } from "lucide-react";
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from "@/components/ui/alert-dialog";
import { Loader2 } from "lucide-react";
import { cn } from "@/lib/utils";

export type ActionType = "pause" | "enable" | "delete" | "remove";

interface ActionConfirmDialogProps {
  open: boolean;
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
    destructive: boolean;
    iconColor: string;
  }
> = {
  pause: {
    icon: Pause,
    label: "Pause",
    description:
      "This will pause the {entity}. It will stop serving ads but can be re-enabled later.",
    destructive: false,
    iconColor: "text-amber-600",
  },
  enable: {
    icon: Play,
    label: "Enable",
    description:
      "This will enable the {entity}. It will start serving ads again.",
    destructive: false,
    iconColor: "text-emerald-600",
  },
  delete: {
    icon: Trash2,
    label: "Delete",
    description:
      "This will permanently delete the {entity}. This action cannot be undone.",
    destructive: true,
    iconColor: "text-destructive",
  },
  remove: {
    icon: Trash2,
    label: "Remove",
    description:
      "This will remove the {entity}. This action cannot be undone.",
    destructive: true,
    iconColor: "text-destructive",
  },
};

export default function ActionConfirmDialog({
  open,
  action,
  entityType,
  entityName,
  entityDetails,
  loading,
  onConfirm,
  onCancel,
}: ActionConfirmDialogProps) {
  const config = ACTION_CONFIG[action];
  const Icon = config.icon;

  return (
    <AlertDialog open={open} onOpenChange={(v) => !v && onCancel()}>
      <AlertDialogContent>
        <AlertDialogHeader>
          <div className="flex items-center gap-2.5">
            <div
              className={cn(
                "p-2 rounded-lg",
                config.destructive ? "bg-destructive/10" : "bg-muted",
              )}
            >
              <Icon className={cn("size-5", config.iconColor)} />
            </div>
            <AlertDialogTitle>
              {config.label} {entityType}?
            </AlertDialogTitle>
          </div>
          <AlertDialogDescription className="pt-2">
            <span className="block rounded-lg bg-muted p-3 mb-3">
              <span className="text-sm font-semibold text-foreground">
                {entityName}
              </span>
              {entityDetails && (
                <span className="block text-xs text-muted-foreground mt-0.5">
                  {entityDetails}
                </span>
              )}
            </span>
            {config.description.replace("{entity}", entityType.toLowerCase())}
          </AlertDialogDescription>
        </AlertDialogHeader>
        <AlertDialogFooter>
          <AlertDialogCancel disabled={loading}>Cancel</AlertDialogCancel>
          <AlertDialogAction
            onClick={onConfirm}
            disabled={loading}
            className={cn(
              config.destructive &&
                "bg-destructive text-white hover:bg-destructive/90",
            )}
          >
            {loading && <Loader2 className="size-4 animate-spin mr-1.5" />}
            {config.label} {entityType}
          </AlertDialogAction>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  );
}

export { ActionConfirmDialog };
