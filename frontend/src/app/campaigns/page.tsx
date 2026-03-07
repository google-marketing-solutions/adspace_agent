"use client";

import * as React from "react";
import { useQuery, useMutation, useQueryClient } from "@tanstack/react-query";
import {
  Pause,
  Play,
  DollarSign,
  RefreshCw,
  Megaphone,
  TrendingUp,
  MousePointerClick,
  Target,
  ArrowUpRight,
  ArrowDownRight,
  ExternalLink,
} from "lucide-react";
import { useRouter } from "next/navigation";
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
import { fetchCampaigns, campaignAction } from "@/lib/api";
import { Card, CardContent } from "@/components/ui/card";
import { formatDollars, formatNumber, formatPercent } from "@/lib/utils";
import type { Campaign } from "@/lib/types";
import { useState, useMemo } from "react";

interface PendingAction {
  campaignId: string;
  campaignName: string;
  action: "pause" | "enable";
}

export default function CampaignsPage() {
  const { customerId } = useAccount();
  const queryClient = useQueryClient();
  const { addToast } = useToast();
  const router = useRouter();
  const [days, setDays] = useState(30);
  const [statusFilter, setStatusFilter] = useState("");
  const [roasFilter, setRoasFilter] = useState("");
  const [pendingAction, setPendingAction] = useState<PendingAction | null>(null);
  const [mutatingIds, setMutatingIds] = useState<Set<string>>(new Set());

  const { data, isLoading, error, isFetching, refetch } = useQuery({
    queryKey: ["campaigns", customerId, days],
    queryFn: () => fetchCampaigns(customerId, days),
    enabled: !!customerId,
  });

  const toggleMutation = useMutation({
    mutationFn: ({ campaignId, action }: { campaignId: string; action: "pause" | "enable" }) => {
      setMutatingIds((prev) => new Set(prev).add(campaignId));
      return campaignAction(customerId, campaignId, action);
    },
    onSuccess: (_data, variables) => {
      setMutatingIds((prev) => { const next = new Set(prev); next.delete(variables.campaignId); return next; });
      queryClient.invalidateQueries({ queryKey: ["campaigns", customerId] });
      const campaign = campaigns.find((c) => c.id === variables.campaignId);
      const name = campaign?.name ?? `Campaign ${variables.campaignId}`;
      const actionLabel = variables.action === "pause" ? "paused" : "enabled";
      addToast("success", `Campaign ${actionLabel}`, `"${name}" has been ${actionLabel} successfully.`);
      setPendingAction(null);
    },
    onError: (err, variables) => {
      setMutatingIds((prev) => { const next = new Set(prev); next.delete(variables.campaignId); return next; });
      addToast("error", "Action Failed", (err as Error).message);
      setPendingAction(null);
    },
  });

  const allCampaigns = data?.campaigns ?? [];

  // Apply filters
  const campaigns = useMemo(() => {
    let filtered = allCampaigns;
    if (statusFilter) filtered = filtered.filter((c) => c.status === statusFilter);
    if (roasFilter === "high") filtered = filtered.filter((c) => c.roas >= 3);
    else if (roasFilter === "medium") filtered = filtered.filter((c) => c.roas >= 1 && c.roas < 3);
    else if (roasFilter === "low") filtered = filtered.filter((c) => c.roas < 1);
    return filtered;
  }, [allCampaigns, statusFilter, roasFilter]);

  const handleActionClick = (campaignId: string, campaignName: string, action: "pause" | "enable") => {
    setPendingAction({ campaignId, campaignName, action });
  };

  const handleConfirm = () => {
    if (pendingAction) {
      toggleMutation.mutate({ campaignId: pendingAction.campaignId, action: pendingAction.action });
    }
  };

  // Counts from full dataset (for filter dropdown badges)
  const allEnabledCount = allCampaigns.filter((c) => c.status === "ENABLED").length;
  const allPausedCount = allCampaigns.filter((c) => c.status === "PAUSED").length;

  // Stats from filtered data (for KPI cards + subtitle)
  const totalSpend = campaigns.reduce((s, c) => s + c.cost, 0);
  const totalClicks = campaigns.reduce((s, c) => s + c.clicks, 0);
  const totalConversions = campaigns.reduce((s, c) => s + c.conversions, 0);
  const avgRoas = campaigns.length > 0 ? campaigns.reduce((s, c) => s + c.roas, 0) / campaigns.length : 0;
  const enabledCount = campaigns.filter((c) => c.status === "ENABLED").length;
  const pausedCount = campaigns.filter((c) => c.status === "PAUSED").length;

  const activeFilterCount = [statusFilter, roasFilter].filter(Boolean).length;
  const isFiltered = activeFilterCount > 0;

  const columns: Column<Campaign & Record<string, unknown>>[] = [
    {
      key: "name",
      header: "Campaign",
      sortable: true,
      sortValue: (row) => row.name,
      render: (row) => (
        <div className="flex items-center gap-3">
          <div className="w-8 h-8 rounded-lg bg-primary/10 flex items-center justify-center shrink-0">
            <Megaphone className="w-4 h-4 text-primary" />
          </div>
          <div className="min-w-0">
            <p className="font-medium text-foreground truncate max-w-[200px] group-hover:text-primary transition-colors">{row.name}</p>
            <p className="text-xs text-muted-foreground font-mono">ID: {row.id}</p>
          </div>
        </div>
      ),
      width: "280px",
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
      key: "budget_per_day",
      header: "Daily Budget",
      sortable: true,
      sortValue: (row) => row.budget_per_day,
      render: (row) => <span className="font-mono text-sm">{formatDollars(row.budget_per_day)}</span>,
      className: "text-right",
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
      key: "roas",
      header: "ROAS",
      sortable: true,
      sortValue: (row) => row.roas,
      render: (row) => (
        <div className="flex items-center justify-end gap-1">
          {row.roas >= 1 ? (
            <ArrowUpRight className="w-3 h-3 text-emerald-500" />
          ) : (
            <ArrowDownRight className="w-3 h-3 text-red-500" />
          )}
          <span className={`text-sm font-semibold ${
            row.roas >= 3 ? "text-emerald-600" : row.roas >= 1 ? "text-amber-600" : "text-red-600"
          }`}>
            {row.roas.toFixed(2)}x
          </span>
        </div>
      ),
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
              <Button
                variant="ghost"
                size="sm"
                loading={isRowMutating}
                onClick={(e: React.MouseEvent) => { e.stopPropagation(); handleActionClick(row.id, row.name, "pause"); }}
                title="Pause campaign"
                className="text-amber-600 hover:bg-amber-50 hover:text-amber-700"
              >
                <Pause className="w-4 h-4" />
              </Button>
            ) : (
              <Button
                variant="ghost"
                size="sm"
                loading={isRowMutating}
                onClick={(e: React.MouseEvent) => { e.stopPropagation(); handleActionClick(row.id, row.name, "enable"); }}
                title="Enable campaign"
                className="text-emerald-600 hover:bg-emerald-50 hover:text-emerald-700"
              >
                <Play className="w-4 h-4" />
              </Button>
            )}
            <Button
              variant="ghost"
              size="sm"
              onClick={(e: React.MouseEvent) => { e.stopPropagation(); router.push(`/ad-groups?campaign=${row.id}`); }}
              title="View ad groups"
              className="text-muted-foreground hover:text-primary hover:bg-primary/10"
            >
              <ExternalLink className="w-4 h-4" />
            </Button>
          </div>
        );
      },
      width: "100px",
    },
  ];

  // Quick stats cards
  const stats = [
    { label: "Total Spend", value: formatDollars(totalSpend), icon: DollarSign, color: "text-blue-600", bg: "bg-blue-50" },
    { label: "Total Clicks", value: formatNumber(totalClicks), icon: MousePointerClick, color: "text-violet-600", bg: "bg-violet-50" },
    { label: "Conversions", value: totalConversions.toFixed(1), icon: Target, color: "text-emerald-600", bg: "bg-emerald-50" },
    { label: "Avg ROAS", value: `${avgRoas.toFixed(2)}x`, icon: TrendingUp, color: "text-amber-600", bg: "bg-amber-50" },
  ];

  return (
    <div className="space-y-6">
      <PageHeader
        title="Campaigns"
        subtitle={
          <span>
            {isFiltered ? `${campaigns.length} of ${allCampaigns.length}` : allCampaigns.length} campaigns &middot;{" "}
            <span className="text-emerald-600">{enabledCount} enabled</span> &middot;{" "}
            <span className="text-amber-600">{pausedCount} paused</span>
          </span>
        }
        actions={
          <div className="flex items-center gap-2">
            <DateRangeFilter value={days} onChange={setDays} />
            <Button
              variant="primary"
              onClick={() => refetch()}
              loading={isFetching}
              icon={<RefreshCw className={`w-4 h-4 ${isFetching ? "animate-spin" : ""}`} />}
            >
              {isFetching ? "Syncing…" : "Sync Data"}
            </Button>
          </div>
        }
      />

      {/* Quick stats */}
      <div className="grid grid-cols-2 md:grid-cols-4 gap-4 stagger-children">
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

      {/* Filters */}
      <FilterBar activeCount={activeFilterCount} onClearAll={() => { setStatusFilter(""); setRoasFilter(""); }}>
        <SelectFilter
          label="Status"
          value={statusFilter}
          onChange={setStatusFilter}
          options={[
            { value: "ENABLED", label: "Enabled", count: allEnabledCount },
            { value: "PAUSED", label: "Paused", count: allPausedCount },
          ]}
        />
        <SelectFilter
          label="ROAS"
          value={roasFilter}
          onChange={setRoasFilter}
          options={[
            { value: "high", label: "High (3x+)" },
            { value: "medium", label: "Medium (1-3x)" },
            { value: "low", label: "Low (<1x)" },
          ]}
        />
      </FilterBar>

      {/* Table */}
      {!isLoading && campaigns.length === 0 && allCampaigns.length === 0 ? (
        <EmptyState
          icon={<Megaphone className="w-12 h-12" />}
          title="No campaigns found"
          description="No campaigns found for this account. Sync data to pull from Google Ads."
          action={
            <Button variant="primary" onClick={() => refetch()} icon={<RefreshCw className="w-4 h-4" />}>
              Sync Data
            </Button>
          }
        />
      ) : (
        <DataTable<Campaign & Record<string, unknown>>
          columns={columns}
          data={campaigns as (Campaign & Record<string, unknown>)[]}
          keyField="id"
          loading={isLoading}
          searchable
          searchPlaceholder="Search campaigns..."
          searchKeys={["name", "id"]}
          exportable
          exportFileName={`campaigns-${customerId}`}
          onRowClick={(row) => router.push(`/ad-groups?campaign=${row.id}`)}
          emptyMessage={activeFilterCount > 0 ? "No campaigns match your filters" : "No campaigns found"}
          emptyIcon={<Megaphone className="w-10 h-10" />}
        />
      )}

      {/* Confirmation Modal */}
      <ActionConfirmDialog
        open={!!pendingAction}
        action={pendingAction?.action ?? "pause"}
        entityType="Campaign"
        entityName={pendingAction?.campaignName ?? ""}
        entityDetails={pendingAction ? `Campaign ID: ${pendingAction.campaignId}` : ""}
        loading={toggleMutation.isPending}
        onConfirm={handleConfirm}
        onCancel={() => setPendingAction(null)}
      />
    </div>
  );
}
