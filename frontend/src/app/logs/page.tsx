"use client";

import { useQuery } from "@tanstack/react-query";
import {
  RefreshCw,
  History,
  CheckCircle2,
  XCircle,
  Clock,
} from "lucide-react";
import { useAccount } from "@/components/AccountContext";
import DataTable, { type Column, legacyColumnsToColumnDefs } from "@/components/data/DataTable";
import { SelectFilter, FilterBar } from "@/components/forms";
import PageHeader from "@/components/layout/PageHeader";
import StatusBadge from "@/components/data/StatusBadge";
import Button from "@/components/data/LoadingButton";
import ErrorBanner from "@/components/feedback/ErrorBanner";
import EmptyState from "@/components/feedback/EmptyState";
import { Card, CardContent } from "@/components/ui/card";
import { fetchMutationLogs } from "@/lib/api";
import type { MutationLog } from "@/lib/types";
import { useState, useMemo } from "react";

export default function LogsPage() {
  const { customerId } = useAccount();
  const [statusFilter, setStatusFilter] = useState("");
  const [opFilter, setOpFilter] = useState("");

  const { data, isLoading, error, isFetching, refetch } = useQuery({
    queryKey: ["logs", customerId],
    queryFn: () => fetchMutationLogs(customerId, 100),
    enabled: !!customerId,
  });

  const allLogs = data?.logs ?? [];

  // Unique operation types
  const opTypes = useMemo(() => [...new Set(allLogs.map((l) => l.operation_type))].filter(Boolean).sort(), [allLogs]);

  // Apply filters
  const logs = useMemo(() => {
    let filtered = allLogs;
    if (statusFilter) filtered = filtered.filter((l) => {
      const s = l.status.toUpperCase();
      if (statusFilter === "SUCCESS") return s === "SUCCESS";
      if (statusFilter === "FAILED") return s === "FAILED" || s === "ERROR";
      return true;
    });
    if (opFilter) filtered = filtered.filter((l) => l.operation_type === opFilter);
    return filtered;
  }, [allLogs, statusFilter, opFilter]);

  const successCount = allLogs.filter((l) => l.status.toUpperCase() === "SUCCESS").length;
  const failedCount = allLogs.filter((l) => { const s = l.status.toUpperCase(); return s === "FAILED" || s === "ERROR"; }).length;
  const activeFilterCount = [statusFilter, opFilter].filter(Boolean).length;

  const columns: Column<MutationLog & Record<string, unknown>>[] = [
    {
      key: "created_at",
      header: "Time",
      sortable: true,
      sortValue: (row) => row.created_at ? new Date(row.created_at).getTime() : 0,
      render: (row) => {
        if (!row.created_at) return <span className="text-muted-foreground/50">—</span>;
        const date = new Date(row.created_at);
        return (
          <div>
            <p className="text-sm text-foreground font-medium">{date.toLocaleDateString()}</p>
            <p className="text-xs text-muted-foreground">{date.toLocaleTimeString()}</p>
          </div>
        );
      },
      width: "140px",
    },
    {
      key: "operation_type",
      header: "Operation",
      sortable: true,
      sortValue: (row) => row.operation_type,
      render: (row) => (
        <span className="px-2.5 py-1 bg-muted text-foreground rounded-lg text-xs font-medium">
          {row.operation_type.replace(/_/g, " ")}
        </span>
      ),
    },
    {
      key: "entity_type",
      header: "Entity Type",
      sortable: true,
      sortValue: (row) => row.entity_type ?? "",
      render: (row) => (
        <span className="text-sm text-foreground capitalize">{row.entity_type?.replace(/_/g, " ") ?? "—"}</span>
      ),
    },
    {
      key: "entity_id",
      header: "Entity ID",
      sortable: false,
      render: (row) => (
        <span className="text-xs font-mono text-muted-foreground bg-muted px-2 py-0.5 rounded">{row.entity_id ?? "—"}</span>
      ),
    },
    {
      key: "status",
      header: "Status",
      sortable: true,
      sortValue: (row) => row.status,
      render: (row) => <StatusBadge status={row.status} />,
    },
    {
      key: "details",
      header: "Details",
      sortable: false,
      render: (row) => (
        <p className="max-w-xs truncate text-xs text-muted-foreground" title={row.details ?? row.error_message ?? ""}>
          {row.details ?? row.error_message ?? "—"}
        </p>
      ),
      width: "250px",
    },
  ];

  const stats = [
    { label: "Total Operations", value: allLogs.length, icon: Clock, color: "text-blue-600", bg: "bg-blue-50" },
    { label: "Successful", value: successCount, icon: CheckCircle2, color: "text-emerald-600", bg: "bg-emerald-50" },
    { label: "Failed", value: failedCount, icon: XCircle, color: "text-red-600", bg: "bg-red-50" },
  ];

  return (
    <div className="space-y-6">
      <PageHeader
        title="Activity Log"
        subtitle={
          <span>
            {allLogs.length} operations &middot;{" "}
            <span className="text-emerald-600">{successCount} successful</span>
            {failedCount > 0 && (
              <>
                {" "}&middot;{" "}
                <span className="text-red-600">{failedCount} failed</span>
              </>
            )}
          </span>
        }
        actions={
          <Button variant="primary" onClick={() => refetch()} loading={isFetching}
            icon={<RefreshCw className={`w-4 h-4 ${isFetching ? "animate-spin" : ""}`} />}>
            {isFetching ? "Syncing…" : "Sync Data"}
          </Button>
        }
      />

      {/* Quick stats */}
      {!isLoading && allLogs.length > 0 && (
        <div className="grid grid-cols-3 gap-4 stagger-children">
          {stats.map((stat) => {
            const Icon = stat.icon;
            return (
              <Card key={stat.label} className="animate-fade-in hover:shadow-md transition-shadow">
                <CardContent className="p-4 flex items-center gap-3">
                  <div className={`p-2 ${stat.bg} rounded-lg`}>
                    <Icon className={`w-5 h-5 ${stat.color}`} />
                  </div>
                  <div>
                    <p className="text-xs text-muted-foreground font-medium">{stat.label}</p>
                    <p className="text-lg font-bold text-foreground">{stat.value}</p>
                  </div>
                </CardContent>
              </Card>
            );
          })}
        </div>
      )}

      {error && <ErrorBanner message={(error as Error).message} />}

      <FilterBar activeCount={activeFilterCount} onClearAll={() => { setStatusFilter(""); setOpFilter(""); }}>
        <SelectFilter label="Status" value={statusFilter} onChange={setStatusFilter}
          options={[
            { value: "SUCCESS", label: "Successful", count: successCount },
            { value: "FAILED", label: "Failed", count: failedCount },
          ]} />
        <SelectFilter label="Operation" value={opFilter} onChange={setOpFilter}
          options={opTypes.map((t) => ({ value: t, label: t.replace(/_/g, " ") }))} />
      </FilterBar>

      {!isLoading && logs.length === 0 && allLogs.length === 0 ? (
        <EmptyState icon={<History className="w-12 h-12" />} title="No activity yet"
          description="Mutation operations (pause, enable, budget changes, etc.) will appear here as you make changes." />
      ) : (
        <DataTable<MutationLog & Record<string, unknown>>
          columns={columns}
          data={logs as (MutationLog & Record<string, unknown>)[]}
          keyField="id"
          loading={isLoading}
          searchable
          searchPlaceholder="Search logs..."
          searchKeys={["operation_type", "entity_type", "entity_id", "details"]}
          exportable
          exportFileName={`logs-${customerId}`}
          emptyMessage={activeFilterCount > 0 ? "No logs match your filters" : "No mutations logged yet"}
          emptyIcon={<History className="w-10 h-10" />}
        />
      )}
    </div>
  );
}
