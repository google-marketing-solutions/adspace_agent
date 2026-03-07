"use client";

import { X, AlertTriangle } from "lucide-react";
import Button from "@/components/Button";
import type { Recommendation } from "@/lib/types";

interface ConfirmationModalProps {
  recommendation: Recommendation;
  loading: boolean;
  onConfirm: () => void;
  onCancel: () => void;
}

/**
 * Modal requiring explicit confirmation before applying a recommendation.
 * Shows full details: action, entity, current vs recommended value,
 * impact estimate, and the raw mutate payload.
 */
export default function ConfirmationModal({
  recommendation: rec,
  loading,
  onConfirm,
  onCancel,
}: ConfirmationModalProps) {
  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/40 backdrop-blur-sm">
      <div className="bg-white rounded-2xl shadow-2xl w-full max-w-lg mx-4 overflow-hidden">
        {/* Header */}
        <div className="flex items-center gap-3 bg-amber-50 border-b border-amber-200 px-5 py-4">
          <AlertTriangle className="w-5 h-5 text-amber-600 shrink-0" />
          <h2 className="text-base font-semibold text-amber-900">
            Confirm Recommendation
          </h2>
          <button
            onClick={onCancel}
            className="ml-auto text-gray-400 hover:text-gray-600"
          >
            <X className="w-5 h-5" />
          </button>
        </div>

        {/* Body */}
        <div className="px-5 py-4 space-y-4 max-h-[60vh] overflow-y-auto">
          <div>
            <span className="inline-block px-2 py-0.5 rounded text-xs font-medium bg-gray-100 text-gray-700 capitalize">
              {rec.type.replace(/_/g, " ")}
            </span>
            <h3 className="text-lg font-semibold text-gray-900 mt-1">
              {rec.title}
            </h3>
            <p className="text-sm text-gray-600 mt-1">{rec.description}</p>
          </div>

          {/* Details grid */}
          <div className="grid grid-cols-2 gap-3 text-sm">
            {rec.entity_name && (
              <div>
                <span className="text-gray-400 text-xs">Entity</span>
                <p className="font-medium text-gray-800">{rec.entity_name}</p>
              </div>
            )}
            {rec.entity_id && (
              <div>
                <span className="text-gray-400 text-xs">Entity ID</span>
                <p className="font-mono text-gray-800 text-xs">
                  {rec.entity_id}
                </p>
              </div>
            )}
            {rec.current_value && (
              <div>
                <span className="text-gray-400 text-xs">Current Value</span>
                <p className="font-medium text-gray-800">
                  {rec.current_value}
                </p>
              </div>
            )}
            {rec.recommended_value && (
              <div>
                <span className="text-gray-400 text-xs">
                  Recommended Value
                </span>
                <p className="font-medium text-blue-700">
                  {rec.recommended_value}
                </p>
              </div>
            )}
            {rec.impact_estimate && (
              <div className="col-span-2">
                <span className="text-gray-400 text-xs">
                  Estimated Impact
                </span>
                <p className="font-medium text-green-700">
                  {rec.impact_estimate}
                </p>
              </div>
            )}
            {rec.confidence_score != null && (
              <div className="col-span-2">
                <span className="text-gray-400 text-xs">
                  Confidence Score
                </span>
                <div className="flex items-center gap-2 mt-0.5">
                  <div className="flex-1 h-2 bg-gray-200 rounded-full overflow-hidden">
                    <div
                      className={`h-full rounded-full ${
                        rec.confidence_score >= 70
                          ? "bg-green-500"
                          : rec.confidence_score >= 40
                            ? "bg-amber-500"
                            : "bg-red-500"
                      }`}
                      style={{ width: `${rec.confidence_score}%` }}
                    />
                  </div>
                  <span className="text-xs font-mono text-gray-700">
                    {rec.confidence_score}/100
                  </span>
                </div>
              </div>
            )}
          </div>

          {/* Mutate payload */}
          {rec.mutate_payload && (
            <div>
              <span className="text-gray-400 text-xs">
                Mutation Payload (what will be executed)
              </span>
              <pre className="mt-1 bg-gray-50 border border-gray-200 rounded-lg p-3 text-xs text-gray-700 overflow-x-auto">
                {JSON.stringify(rec.mutate_payload, null, 2)}
              </pre>
            </div>
          )}
        </div>

        {/* Footer */}
        <div className="flex items-center justify-end gap-3 border-t border-gray-200 px-5 py-4 bg-gray-50">
          <Button variant="ghost" size="sm" onClick={onCancel}>
            Cancel
          </Button>
          <Button
            variant="primary"
            size="sm"
            loading={loading}
            onClick={onConfirm}
          >
            Confirm &amp; Apply
          </Button>
        </div>
      </div>
    </div>
  );
}
