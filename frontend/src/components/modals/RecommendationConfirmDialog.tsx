"use client";

import { AlertTriangle } from "lucide-react";
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
import { Badge } from "@/components/ui/badge";
import { Progress } from "@/components/ui/progress";
import { Loader2 } from "lucide-react";
import { ScrollArea } from "@/components/ui/scroll-area";
import type { Recommendation } from "@/lib/types";

interface RecommendationConfirmDialogProps {
  recommendation: Recommendation | null;
  loading: boolean;
  onConfirm: () => void;
  onCancel: () => void;
}

export default function RecommendationConfirmDialog({
  recommendation: rec,
  loading,
  onConfirm,
  onCancel,
}: RecommendationConfirmDialogProps) {
  if (!rec) return null;

  return (
    <AlertDialog open={!!rec} onOpenChange={(v) => !v && onCancel()}>
      <AlertDialogContent className="max-w-lg">
        <AlertDialogHeader>
          <div className="flex items-center gap-2.5">
            <div className="p-2 rounded-lg bg-amber-500/10">
              <AlertTriangle className="size-5 text-amber-600" />
            </div>
            <AlertDialogTitle>Confirm Recommendation</AlertDialogTitle>
          </div>
          <AlertDialogDescription asChild>
            <div className="pt-2 space-y-4">
              <div>
                <Badge variant="outline" className="capitalize mb-2">
                  {rec.type.replace(/_/g, " ")}
                </Badge>
                <h3 className="text-base font-semibold text-foreground">
                  {rec.title}
                </h3>
                <p className="text-sm text-muted-foreground mt-1">
                  {rec.description}
                </p>
              </div>

              <ScrollArea className="max-h-[40vh]">
                <div className="grid grid-cols-2 gap-3 text-sm">
                  {rec.entity_name && (
                    <div>
                      <span className="text-muted-foreground text-xs">
                        Entity
                      </span>
                      <p className="font-medium text-foreground">
                        {rec.entity_name}
                      </p>
                    </div>
                  )}
                  {rec.entity_id && (
                    <div>
                      <span className="text-muted-foreground text-xs">
                        Entity ID
                      </span>
                      <p className="font-mono text-foreground text-xs">
                        {rec.entity_id}
                      </p>
                    </div>
                  )}
                  {rec.current_value && (
                    <div>
                      <span className="text-muted-foreground text-xs">
                        Current Value
                      </span>
                      <p className="font-medium text-foreground">
                        {rec.current_value}
                      </p>
                    </div>
                  )}
                  {rec.recommended_value && (
                    <div>
                      <span className="text-muted-foreground text-xs">
                        Recommended Value
                      </span>
                      <p className="font-medium text-primary">
                        {rec.recommended_value}
                      </p>
                    </div>
                  )}
                  {rec.impact_estimate && (
                    <div className="col-span-2">
                      <span className="text-muted-foreground text-xs">
                        Estimated Impact
                      </span>
                      <p className="font-medium text-emerald-600">
                        {rec.impact_estimate}
                      </p>
                    </div>
                  )}
                  {rec.confidence_score != null && (
                    <div className="col-span-2">
                      <span className="text-muted-foreground text-xs">
                        Confidence Score
                      </span>
                      <div className="flex items-center gap-2 mt-1">
                        <Progress
                          value={rec.confidence_score * 100}
                          className="h-2 flex-1"
                        />
                        <span className="text-xs font-mono text-foreground">
                          {(rec.confidence_score * 100).toFixed(0)}%
                        </span>
                      </div>
                    </div>
                  )}
                </div>

                {rec.mutate_payload && (
                  <div className="mt-4">
                    <span className="text-muted-foreground text-xs">
                      Mutation Payload
                    </span>
                    <pre className="mt-1 rounded-lg border bg-muted p-3 text-xs text-foreground overflow-x-auto">
                      {JSON.stringify(rec.mutate_payload, null, 2)}
                    </pre>
                  </div>
                )}
              </ScrollArea>
            </div>
          </AlertDialogDescription>
        </AlertDialogHeader>
        <AlertDialogFooter>
          <AlertDialogCancel disabled={loading}>Cancel</AlertDialogCancel>
          <AlertDialogAction onClick={onConfirm} disabled={loading}>
            {loading && <Loader2 className="size-4 animate-spin mr-1.5" />}
            Confirm &amp; Apply
          </AlertDialogAction>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  );
}

export { RecommendationConfirmDialog };
