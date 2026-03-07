"use client";

import * as React from "react";
import { useQuery, useMutation, useQueryClient } from "@tanstack/react-query";
import {
  Pause,
  Play,
  RefreshCw,
  FileText,
  DollarSign,
  MousePointerClick,
  Target,
} from "lucide-react";
import { useSearchParams } from "next/navigation";
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
import { Card, CardContent } from "@/components/ui/card";
import { fetchAds, adAction } from "@/lib/api";
import { formatDollars, formatNumber, formatPercent } from "@/lib/utils";
import type { Ad } from "@/lib/types";
import { useState, useMemo, Suspense } from "react";

interface PendingAction {
  adGroupId: string;
  adId: string;
  adName: string;
  action: "pause" | "enable";
}

export default function AdsPage() {
  return (
    <Suspense fallback={<div className="p-8 text-center text-muted-foreground">Loading…</div>}>
      <AdsPageInner />
    </Suspense>
  );
}

function AdsPageInner() {
  const { customerId } = useAccount();
  const queryClient = useQueryClient();
  const { addToast } = useToast();
  const searchParams = useSearchParams();
  const campaignParam = searchParams.get("campaign") ?? "";
  const adGroupParam = searchParams.get("adgroup") ?? "";

  const [days, setDays] = useState(30);
  const [statusFilter, setStatusFilter] = useState("");
  const [typeFilter, setTypeFilter] = useState("");
  const [campaignFilter, setCampaignFilter] = useState(campaignParam);
  const [adGroupFilter, setAdGroupFilter] = useState(adGroupParam);
  const [pendingAction, setPendingAction] = useState<PendingAction | null>(null);
  const [mutatingIds, setMutatingIds] = useState<Set<string>>(new Set());

  const { data, isLoading, error, isFetching, refetch } = useQuery({
    queryKey: ["ads", customerId, days],
    queryFn: () => fetchAds(customerId, undefined, days),
    enabled: !!customerId,
  });

  const toggleMutation = useMutation({
    mutationFn: ({ adGroupId, adId, action }: { adGroupId: string; adId: string; action: "pause" | "enable" }) => {
      setMutatingIds((prev) => new Set(prev).add(adId));
      return adAction(customerId, adGroupId, adId, action);
    },
    onSuccess: (_data, variables) => {
      setMutatingIds((prev) => { const next = new Set(prev); next.delete(variables.adId); return next; });
      queryClient.invalidateQueries({ queryKey: ["ads", customerId] });
      const ad = allAds.find((a) => a.id === variables.adId);
      const name = ad && ad.headlines?.length > 0 ? ad.headlines[0] : `Ad ${variables.adId}`;
      const actionLabel = variables.action === "pause" ? "paused" : "enabled";
      addToast("success", `Ad ${actionLabel}`, `"${name}" has been ${actionLabel} successfully.`);
      setPendingAction(null);
    },
    onError: (err, variables) => {
      setMutatingIds((prev) => { const next = new Set(prev); next.delete(variables.adId); return next; });
      addToast("error", "Action Failed", (err as Error).message);
      setPendingAction(null);
    },
  });

  const allAds = data?.ads ?? [];

  // Unique filter options from full dataset
  const adTypes = useMemo(() => [...new Set(allAds.map((a) => a.type))].filter(Boolean).sort(), [allAds]);
  const campaignOptions = useMemo(
    () => [...new Map(allAds.map((a) => [a.campaign_id, a.campaign_name])).entries()].sort((a, b) => a[1].localeCompare(b[1])),
    [allAds],
  );
  const adGroupOptions = useMemo(
    () => [...new Map(allAds.map((a) => [a.ad_group_id, a.ad_group_name])).entries()].sort((a, b) => a[1].localeCompare(b[1])),
    [allAds],
  );

  // Apply filters
  const ads = useMemo(() => {
    let filtered = allAds;
    if (campaignFilter) filtered = filtered.filter((a) => a.campaign_id === campaignFilter || a.campaign_name === campaignFilter);
    if (adGroupFilter) filtered = filtered.filter((a) => a.ad_group_id === adGroupFilter || a.ad_group_name === adGroupFilter);
    if (statusFilter) filtered = filtered.filter((a) => a.status === statusFilter);
    if (typeFilter) filtered = filtered.filter((a) => a.type === typeFilter);
    return filtered;
  }, [allAds, campaignFilter, adGroupFilter, statusFilter, typeFilter]);

  const handleActionClick = (ad: Ad, action: "pause" | "enable") => {
    const name = ad.headlines?.length > 0 ? ad.headlines.slice(0, 2).join(" | ") : `Ad ${ad.id}`;
    setPendingAction({ adGroupId: ad.ad_group_id, adId: ad.id, adName: name, action });
  };

  const handleConfirm = () => {
    if (pendingAction) {
      toggleMutation.mutate({ adGroupId: pendingAction.adGroupId, adId: pendingAction.adId, action: pendingAction.action });
    }
  };

  // Counts from full dataset (for filter dropdown badges)
  const allEnabledCount = allAds.filter((a) => a.status === "ENABLED").length;
  const allPausedCount = allAds.filter((a) => a.status === "PAUSED").length;

  // Stats from filtered data (for KPI cards + subtitle)
  const enabledCount = ads.filter((a) => a.status === "ENABLED").length;
  const pausedCount = ads.filter((a) => a.status === "PAUSED").length;
  const totalSpend = ads.reduce((s, a) => s + a.cost, 0);
  const totalClicks = ads.reduce((s, a) => s + a.clicks, 0);
  const totalConversions = ads.reduce((s, a) => s + a.conversions, 0);
  const activeFilterCount = [campaignFilter, adGroupFilter, statusFilter, typeFilter].filter(Boolean).length;
  const isFiltered = activeFilterCount > 0;

  const columns: Column<Ad & Record<string, unknown>>[] = [
    {
      key: "headlines",
      header: "Ad",
      sortable: true,
      sortValue: (row) => (row.headlines ?? []).length > 0 ? row.headlines[0] : row.id,
      render: (row) => (
        <div className="flex items-center gap-3 max-w-xs">
          <div className="w-8 h-8 rounded-lg bg-pink-50 flex items-center justify-center shrink-0">
            <FileText className="w-4 h-4 text-pink-500" />
          </div>
          <div className="min-w-0">
            <p className="font-medium text-foreground text-sm truncate">
              {(row.headlines ?? []).length > 0 ? row.headlines.slice(0, 3).join(" | ") : `Ad ${row.id}`}
            </p>
            <p className="text-xs text-muted-foreground truncate">{(row.descriptions ?? [])[0] ?? ""}</p>
            <p className="text-xs text-muted-foreground mt-0.5 truncate">{row.campaign_name} / {row.ad_group_name}</p>
          </div>
        </div>
      ),
      width: "320px",
    },
    {
      key: "type",
      header: "Type",
      sortable: true,
      sortValue: (row) => row.type,
      render: (row) => (
        <span className="px-2 py-0.5 bg-muted text-muted-foreground rounded text-xs font-medium">{row.type}</span>
      ),
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
                onClick={(e: React.MouseEvent) => { e.stopPropagation(); handleActionClick(row as unknown as Ad, "pause"); }}
                title="Pause ad" className="text-amber-600 hover:bg-amber-50 hover:text-amber-700">
                <Pause className="w-4 h-4" />
              </Button>
            ) : (
              <Button variant="ghost" size="sm" loading={isRowMutating}
                onClick={(e: React.MouseEvent) => { e.stopPropagation(); handleActionClick(row as unknown as Ad, "enable"); }}
                title="Enable ad" className="text-emerald-600 hover:bg-emerald-50 hover:text-emerald-700">
                <Play className="w-4 h-4" />
              </Button>
            )}
          </div>
        );
      },
      width: "64px",
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
        title="Ads"
        subtitle={
          <span>
            {isFiltered ? `${ads.length} of ${allAds.length}` : allAds.length} ads &middot;{" "}
            <span className="text-emerald-600">{enabledCount} enabled</span> &middot;{" "}
            <span className="text-amber-600">{pausedCount} paused</span>
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

      <FilterBar activeCount={activeFilterCount} onClearAll={() => { setCampaignFilter(""); setAdGroupFilter(""); setStatusFilter(""); setTypeFilter(""); }}>
        <SelectFilter label="Campaign" value={campaignFilter} onChange={setCampaignFilter}
          options={campaignOptions.map(([id, name]) => ({ value: id, label: name }))} />
        <SelectFilter label="Ad Group" value={adGroupFilter} onChange={setAdGroupFilter}
          options={adGroupOptions.map(([id, name]) => ({ value: id, label: name }))} />
        <SelectFilter label="Status" value={statusFilter} onChange={setStatusFilter}
          options={[
            { value: "ENABLED", label: "Enabled", count: allEnabledCount },
            { value: "PAUSED", label: "Paused", count: allPausedCount },
          ]} />
        <SelectFilter label="Type" value={typeFilter} onChange={setTypeFilter}
          options={adTypes.map((t) => ({ value: t, label: t }))} />
      </FilterBar>

      {!isLoading && ads.length === 0 && allAds.length === 0 ? (
        <EmptyState icon={<FileText className="w-12 h-12" />} title="No ads found"
          description="No ads found for this account. Sync data to pull from Google Ads."
          action={<Button variant="primary" onClick={() => refetch()} icon={<RefreshCw className="w-4 h-4" />}>Sync Data</Button>} />
      ) : (
        <DataTable<Ad & Record<string, unknown>>
          columns={columns}
          data={ads as (Ad & Record<string, unknown>)[]}
          keyField="id"
          loading={isLoading}
          searchable
          searchPlaceholder="Search ads..."
          searchKeys={["headlines", "campaign_name", "ad_group_name"]}
          exportable
          exportFileName={`ads-${customerId}`}
          emptyMessage={activeFilterCount > 0 ? "No ads match your filters" : "No ads found"}
          emptyIcon={<FileText className="w-10 h-10" />}
        />
      )}

      <ActionConfirmDialog open={!!pendingAction} action={pendingAction?.action ?? "pause"} entityType="Ad" entityName={pendingAction?.adName ?? ""}
        entityDetails={pendingAction ? `Ad ID: ${pendingAction.adId}` : ""} loading={toggleMutation.isPending}
        onConfirm={handleConfirm} onCancel={() => setPendingAction(null)} />
    </div>
  );
}
