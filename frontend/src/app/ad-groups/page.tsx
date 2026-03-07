"use client";

import * as React from "react";
import { useQuery, useMutation, useQueryClient } from "@tanstack/react-query";
import {
  Pause,
  Play,
  RefreshCw,
  Layers,
  DollarSign,
  MousePointerClick,
  Target,
  ExternalLink,
} from "lucide-react";
import { useRouter, useSearchParams } from "next/navigation";
import { useAccount } from "@/components/AccountContext";
import DataTable, { type Column, legacyColumnsToColumnDefs } from "@/components/data/DataTable";
import { SelectFilter, DateRangeFilter, FilterBar } from "@/components/forms";
import PageHeader from "@/components/layout/PageHeader";
import StatusBadge from "@/components/data/StatusBadge";
import Button from "@/components/data/LoadingButton";
import ErrorBanner from "@/components/feedback/ErrorBanner";
import ActionConfirmDialog from "@/components/modals/ActionConfirmDialog";
import EmptyState from "@/components/feedback/EmptyState";
import { useToast } from "@/hooks/use-toast";
import { fetchAdGroups, adGroupAction } from "@/lib/api";
import { Card, CardContent } from "@/components/ui/card";
import { formatDollars, formatNumber, formatPercent } from "@/lib/utils";
import type { AdGroup } from "@/lib/types";
import { useState, useMemo, Suspense } from "react";

interface PendingAction {
  adGroupId: string;
  adGroupName: string;
  action: "pause" | "enable";
}

export default function AdGroupsPage() {
  return (
    <Suspense fallback={<div className="p-8 text-center text-muted-foreground">Loading…</div>}>
      <AdGroupsPageInner />
    </Suspense>
  );
}

function AdGroupsPageInner() {
  const { customerId } = useAccount();
  const queryClient = useQueryClient();
  const { addToast } = useToast();
  const router = useRouter();
  const searchParams = useSearchParams();
  const campaignParam = searchParams.get("campaign") ?? "";

  const [days, setDays] = useState(30);
  const [statusFilter, setStatusFilter] = useState("");
  const [campaignFilter, setCampaignFilter] = useState(campaignParam);
  const [pendingAction, setPendingAction] = useState<PendingAction | null>(null);
  const [mutatingIds, setMutatingIds] = useState<Set<string>>(new Set());

  const { data, isLoading, error, isFetching, refetch } = useQuery({
    queryKey: ["adGroups", customerId, days],
    queryFn: () => fetchAdGroups(customerId, undefined, days),
    enabled: !!customerId,
  });

  const toggleMutation = useMutation({
    mutationFn: ({ adGroupId, action }: { adGroupId: string; action: "pause" | "enable" }) => {
      setMutatingIds((prev) => new Set(prev).add(adGroupId));
      return adGroupAction(customerId, adGroupId, action);
    },
    onSuccess: (_data, variables) => {
      setMutatingIds((prev) => { const next = new Set(prev); next.delete(variables.adGroupId); return next; });
      queryClient.invalidateQueries({ queryKey: ["adGroups", customerId] });
      const adGroup = adGroups.find((ag) => ag.id === variables.adGroupId);
      const name = adGroup?.name ?? `Ad Group ${variables.adGroupId}`;
      const actionLabel = variables.action === "pause" ? "paused" : "enabled";
      addToast("success", `Ad Group ${actionLabel}`, `"${name}" has been ${actionLabel} successfully.`);
      setPendingAction(null);
    },
    onError: (err, variables) => {
      setMutatingIds((prev) => { const next = new Set(prev); next.delete(variables.adGroupId); return next; });
      addToast("error", "Action Failed", (err as Error).message);
      setPendingAction(null);
    },
  });

  const allAdGroups = data?.ad_groups ?? [];

  // Unique campaign names for filter
  const campaignNames = useMemo(() => {
    const unique = [...new Set(allAdGroups.map((ag) => ag.campaign_name))].filter(Boolean).sort();
    return unique;
  }, [allAdGroups]);

  // Apply filters
  const adGroups = useMemo(() => {
    let filtered = allAdGroups;
    if (statusFilter) filtered = filtered.filter((ag) => ag.status === statusFilter);
    if (campaignFilter) filtered = filtered.filter((ag) => ag.campaign_name === campaignFilter || ag.campaign_id === campaignFilter);
    return filtered;
  }, [allAdGroups, statusFilter, campaignFilter]);

  const handleActionClick = (adGroupId: string, adGroupName: string, action: "pause" | "enable") => {
    setPendingAction({ adGroupId, adGroupName, action });
  };

  const handleConfirm = () => {
    if (pendingAction) {
      toggleMutation.mutate({ adGroupId: pendingAction.adGroupId, action: pendingAction.action });
    }
  };

  // Counts from full dataset (for filter dropdown badges)
  const allEnabledCount = allAdGroups.filter((ag) => ag.status === "ENABLED").length;
  const allPausedCount = allAdGroups.filter((ag) => ag.status === "PAUSED").length;

  // Stats from filtered data (for KPI cards + subtitle)
  const enabledCount = adGroups.filter((ag) => ag.status === "ENABLED").length;
  const pausedCount = adGroups.filter((ag) => ag.status === "PAUSED").length;
  const totalSpend = adGroups.reduce((s, ag) => s + ag.cost, 0);
  const totalClicks = adGroups.reduce((s, ag) => s + ag.clicks, 0);
  const totalConversions = adGroups.reduce((s, ag) => s + ag.conversions, 0);

  const activeFilterCount = [statusFilter, campaignFilter].filter(Boolean).length;
  const isFiltered = activeFilterCount > 0;

  const columns: Column<AdGroup & Record<string, unknown>>[] = [
    {
      key: "name",
      header: "Ad Group",
      sortable: true,
      sortValue: (row) => row.name,
      render: (row) => (
        <div className="flex items-center gap-3">
          <div className="w-8 h-8 rounded-lg bg-violet-50 flex items-center justify-center shrink-0">
            <Layers className="w-4 h-4 text-violet-500" />
          </div>
          <div className="min-w-0">
            <p className="font-medium text-foreground truncate max-w-[180px] group-hover:text-primary transition-colors">{row.name}</p>
            <p className="text-xs text-muted-foreground truncate">{row.campaign_name}</p>
          </div>
        </div>
      ),
      width: "260px",
    },
    {
      key: "status",
      header: "Status",
      sortable: true,
      sortValue: (row) => row.status,
      render: (row) => (
        <div className="flex items-center gap-2">
          <StatusBadge status={row.status} />
          {mutatingIds.has(row.id) && (
            <span className="inline-flex items-center gap-1 text-xs text-primary animate-pulse">
              <RefreshCw className="w-3 h-3 animate-spin" />
            </span>
          )}
        </div>
      ),
    },
    {
      key: "cost",
      header: "Spend",
      sortable: true,
      sortValue: (row) => row.cost,
      render: (row) => <span className="font-semibold text-sm">{formatDollars(row.cost)}</span>,
      className: "text-right",
    },
    {
      key: "impressions",
      header: "Impressions",
      sortable: true,
      sortValue: (row) => row.impressions,
      render: (row) => <span className="text-sm text-muted-foreground">{formatNumber(row.impressions)}</span>,
      className: "text-right",
    },
    {
      key: "clicks",
      header: "Clicks",
      sortable: true,
      sortValue: (row) => row.clicks,
      render: (row) => <span className="text-sm text-muted-foreground">{formatNumber(row.clicks)}</span>,
      className: "text-right",
    },
    {
      key: "ctr",
      header: "CTR",
      sortable: true,
      sortValue: (row) => row.ctr,
      render: (row) => <span className="text-sm">{formatPercent(row.ctr)}</span>,
      className: "text-right",
    },
    {
      key: "conversions",
      header: "Conv.",
      sortable: true,
      sortValue: (row) => row.conversions,
      render: (row) => <span className="text-sm">{row.conversions.toFixed(1)}</span>,
      className: "text-right",
    },
    {
      key: "actions",
      header: "",
      sortable: false,
      render: (row) => {
        const isRowMutating = mutatingIds.has(row.id);
        return (
          <div className="flex gap-1" onClick={(e: React.MouseEvent) => e.stopPropagation()}>
            {row.status === "ENABLED" ? (
              <Button variant="ghost" size="sm" loading={isRowMutating}
                onClick={(e: React.MouseEvent) => { e.stopPropagation(); handleActionClick(row.id, row.name, "pause"); }}
                title="Pause ad group" className="text-amber-600 hover:bg-amber-50 hover:text-amber-700">
                <Pause className="w-4 h-4" />
              </Button>
            ) : (
              <Button variant="ghost" size="sm" loading={isRowMutating}
                onClick={(e: React.MouseEvent) => { e.stopPropagation(); handleActionClick(row.id, row.name, "enable"); }}
                title="Enable ad group" className="text-emerald-600 hover:bg-emerald-50 hover:text-emerald-700">
                <Play className="w-4 h-4" />
              </Button>
            )}
            <Button variant="ghost" size="sm"
              onClick={(e: React.MouseEvent) => { e.stopPropagation(); router.push(`/keywords?adgroup=${row.id}`); }}
              title="View keywords" className="text-muted-foreground hover:text-primary hover:bg-primary/10">
              <ExternalLink className="w-4 h-4" />
            </Button>
          </div>
        );
      },
      width: "100px",
    },
  ];

  const stats = [
    { label: "Total Spend", value: formatDollars(totalSpend), icon: DollarSign, color: "text-blue-600", bg: "bg-blue-50" },
    { label: "Total Clicks", value: formatNumber(totalClicks), icon: MousePointerClick, color: "text-violet-600", bg: "bg-violet-50" },
    { label: "Conversions", value: totalConversions.toFixed(1), icon: Target, color: "text-emerald-600", bg: "bg-emerald-50" },
  ];

  return (
    <div className="space-y-6">
      <PageHeader
        title="Ad Groups"
        subtitle={
          <span>
            {isFiltered ? `${adGroups.length} of ${allAdGroups.length}` : allAdGroups.length} ad groups &middot;{" "}
            <span className="text-emerald-600">{enabledCount} enabled</span> &middot;{" "}
            <span className="text-amber-600">{pausedCount} paused</span> &middot;{" "}
            {formatDollars(totalSpend)} total spend
          </span>
        }
        actions={
          <div className="flex items-center gap-2">
            <DateRangeFilter value={days} onChange={setDays} />
            <Button variant="primary" onClick={() => refetch()} loading={isFetching}
              icon={<RefreshCw className={`w-4 h-4 ${isFetching ? "animate-spin" : ""}`} />}>
              {isFetching ? "Syncing…" : "Sync Data"}
            </Button>
          </div>
        }
      />

      {/* Quick stats */}
      <div className="grid grid-cols-3 gap-4 stagger-children">
        {stats.map((stat) => {
          const Icon = stat.icon;
          return (
            <Card key={stat.label} className="animate-fade-in hover:shadow-md transition-shadow">
              <CardContent className="p-4">
                <div className="flex items-center gap-2 mb-1.5">
                  <div className={`p-1.5 ${stat.bg} rounded-lg`}>
                    <Icon className={`w-3.5 h-3.5 ${stat.color}`} />
                  </div>
                  <p className="text-xs text-muted-foreground font-medium">{stat.label}</p>
                </div>
                <p className="text-xl font-bold text-foreground tracking-tight">{stat.value}</p>
              </CardContent>
            </Card>
          );
        })}
      </div>

      {error && <ErrorBanner message={(error as Error).message} />}

      <FilterBar activeCount={activeFilterCount} onClearAll={() => { setStatusFilter(""); setCampaignFilter(""); }}>
        <SelectFilter label="Status" value={statusFilter} onChange={setStatusFilter}
          options={[
            { value: "ENABLED", label: "Enabled", count: allEnabledCount },
            { value: "PAUSED", label: "Paused", count: allPausedCount },
          ]} />
        <SelectFilter label="Campaign" value={campaignFilter} onChange={setCampaignFilter}
          options={campaignNames.map((name) => ({ value: name, label: name }))} />
      </FilterBar>

      {!isLoading && adGroups.length === 0 && allAdGroups.length === 0 ? (
        <EmptyState icon={<Layers className="w-12 h-12" />} title="No ad groups found"
          description="No ad groups found for this account. Sync data to pull from Google Ads."
          action={<Button variant="primary" onClick={() => refetch()} icon={<RefreshCw className="w-4 h-4" />}>Sync Data</Button>} />
      ) : (
        <DataTable<AdGroup & Record<string, unknown>>
          columns={columns}
          data={adGroups as (AdGroup & Record<string, unknown>)[]}
          keyField="id"
          loading={isLoading}
          searchable
          searchPlaceholder="Search ad groups..."
          searchKeys={["name", "campaign_name"]}
          exportable
          exportFileName={`ad-groups-${customerId}`}
          onRowClick={(row) => router.push(`/keywords?adgroup=${row.id}`)}
          emptyMessage={activeFilterCount > 0 ? "No ad groups match your filters" : "No ad groups found"}
          emptyIcon={<Layers className="w-10 h-10" />}
        />
      )}

      <ActionConfirmDialog open={!!pendingAction} action={pendingAction?.action ?? "pause"} entityType="Ad Group" entityName={pendingAction?.adGroupName ?? ""}
        entityDetails={pendingAction ? `Ad Group ID: ${pendingAction.adGroupId}` : ""} loading={toggleMutation.isPending}
        onConfirm={handleConfirm} onCancel={() => setPendingAction(null)} />
    </div>
  );
}
