"use client";

import { useQuery } from "@tanstack/react-query";
import {
  RefreshCw,
  Key,
  DollarSign,
  MousePointerClick,
  Star,
} from "lucide-react";
import { useSearchParams } from "next/navigation";
import { useAccount } from "@/components/AccountContext";
import DataTable, { type Column, legacyColumnsToColumnDefs } from "@/components/data/DataTable";
import { SelectFilter, DateRangeFilter, FilterBar } from "@/components/forms";
import PageHeader from "@/components/layout/PageHeader";
import StatusBadge from "@/components/data/StatusBadge";
import Button from "@/components/data/LoadingButton";
import ErrorBanner from "@/components/feedback/ErrorBanner";
import EmptyState from "@/components/feedback/EmptyState";
import { Card, CardContent } from "@/components/ui/card";
import { fetchKeywords } from "@/lib/api";
import { formatDollars, formatNumber, formatPercent } from "@/lib/utils";
import type { Keyword } from "@/lib/types";
import { useState, useMemo, Suspense } from "react";

export default function KeywordsPage() {
  return (
    <Suspense fallback={<div className="p-8 text-center text-muted-foreground">Loading…</div>}>
      <KeywordsPageInner />
    </Suspense>
  );
}

function KeywordsPageInner() {
  const { customerId } = useAccount();
  const searchParams = useSearchParams();
  const adGroupParam = searchParams.get("adgroup") ?? "";
  const campaignParam = searchParams.get("campaign") ?? "";

  const [days, setDays] = useState(30);
  const [statusFilter, setStatusFilter] = useState("");
  const [matchFilter, setMatchFilter] = useState("");
  const [qsFilter, setQsFilter] = useState("");
  const [campaignFilter, setCampaignFilter] = useState(campaignParam);
  const [adGroupFilter, setAdGroupFilter] = useState(adGroupParam);

  const { data, isLoading, error, isFetching, refetch } = useQuery({
    queryKey: ["keywords", customerId, days],
    queryFn: () => fetchKeywords(customerId, undefined, days),
    enabled: !!customerId,
  });

  const allKeywords = data?.keywords ?? [];

  // Unique filter options from full dataset
  const matchTypes = useMemo(() => [...new Set(allKeywords.map((k) => k.match_type))].filter(Boolean).sort(), [allKeywords]);
  const campaignOptions = useMemo(
    () => [...new Map(allKeywords.map((k) => [k.campaign_id, k.campaign_name])).entries()].sort((a, b) => a[1].localeCompare(b[1])),
    [allKeywords],
  );
  const adGroupOptions = useMemo(
    () => [...new Map(allKeywords.map((k) => [k.ad_group_id, k.ad_group_name])).entries()].sort((a, b) => a[1].localeCompare(b[1])),
    [allKeywords],
  );

  // Apply filters
  const keywords = useMemo(() => {
    let filtered = allKeywords;
    if (campaignFilter) filtered = filtered.filter((k) => k.campaign_id === campaignFilter || k.campaign_name === campaignFilter);
    if (adGroupFilter) filtered = filtered.filter((k) => k.ad_group_id === adGroupFilter || k.ad_group_name === adGroupFilter);
    if (statusFilter) filtered = filtered.filter((k) => k.status === statusFilter);
    if (matchFilter) filtered = filtered.filter((k) => k.match_type === matchFilter);
    if (qsFilter === "high") filtered = filtered.filter((k) => (k.quality_score ?? 0) >= 7);
    else if (qsFilter === "medium") filtered = filtered.filter((k) => (k.quality_score ?? 0) >= 4 && (k.quality_score ?? 0) < 7);
    else if (qsFilter === "low") filtered = filtered.filter((k) => k.quality_score != null && (k.quality_score ?? 0) < 4);
    return filtered;
  }, [allKeywords, campaignFilter, adGroupFilter, statusFilter, matchFilter, qsFilter]);

  // Counts from full dataset (for filter dropdown badges)
  const allEnabledCount = allKeywords.filter((k) => k.status === "ENABLED").length;
  const allPausedCount = allKeywords.filter((k) => k.status === "PAUSED").length;

  // Stats from filtered data (for KPI cards + subtitle)
  const enabledCount = keywords.filter((k) => k.status === "ENABLED").length;
  const pausedCount = keywords.filter((k) => k.status === "PAUSED").length;
  const totalSpend = keywords.reduce((s, k) => s + k.cost, 0);
  const totalClicks = keywords.reduce((s, k) => s + k.clicks, 0);
  const avgQs = keywords.filter((k) => k.quality_score != null);
  const avgQsValue = avgQs.length > 0 ? avgQs.reduce((s, k) => s + (k.quality_score ?? 0), 0) / avgQs.length : null;
  const activeFilterCount = [campaignFilter, adGroupFilter, statusFilter, matchFilter, qsFilter].filter(Boolean).length;
  const isFiltered = activeFilterCount > 0;

  const columns: Column<Keyword & Record<string, unknown>>[] = [
    {
      key: "keyword_text",
      header: "Keyword",
      sortable: true,
      sortValue: (row) => row.keyword_text,
      render: (row) => (
        <div className="flex items-center gap-3">
          <div className="w-8 h-8 rounded-lg bg-teal-50 flex items-center justify-center shrink-0">
            <Key className="w-4 h-4 text-teal-500" />
          </div>
          <div className="min-w-0">
            <p className="font-medium text-foreground truncate max-w-[200px]">{row.keyword_text}</p>
            <p className="text-xs text-muted-foreground truncate">{row.campaign_name} / {row.ad_group_name}</p>
          </div>
        </div>
      ),
      width: "280px",
    },
    {
      key: "match_type",
      header: "Match Type",
      sortable: true,
      sortValue: (row) => row.match_type,
      render: (row) => {
        const colors: Record<string, string> = {
          EXACT: "bg-primary/10 text-primary",
          PHRASE: "bg-violet-50 text-violet-700",
          BROAD: "bg-muted text-muted-foreground",
        };
        return (
          <span className={`px-2 py-0.5 rounded text-xs font-medium ${colors[row.match_type] ?? "bg-muted text-muted-foreground"}`}>
            {row.match_type}
          </span>
        );
      },
    },
    {
      key: "status",
      header: "Status",
      sortable: true,
      sortValue: (row) => row.status,
      render: (row) => <StatusBadge status={row.status} />,
    },
    {
      key: "quality_score",
      header: "QS",
      sortable: true,
      sortValue: (row) => row.quality_score ?? -1,
      render: (row) => {
        const qs = row.quality_score;
        if (qs === null || qs === undefined) return <span className="text-muted-foreground/50">—</span>;
        const color = qs >= 7 ? "text-emerald-600 bg-emerald-50 border-emerald-200" : qs >= 4 ? "text-amber-600 bg-amber-50 border-amber-200" : "text-red-600 bg-red-50 border-red-200";
        return (
          <span className={`inline-flex items-center justify-center w-8 h-8 rounded-full font-bold text-sm border ${color}`}>
            {qs}
          </span>
        );
      },
      className: "text-center",
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
  ];

  const stats = [
    { label: "Total Spend", value: formatDollars(totalSpend), icon: DollarSign, color: "text-blue-600", bg: "bg-blue-50" },
    { label: "Total Clicks", value: formatNumber(totalClicks), icon: MousePointerClick, color: "text-violet-600", bg: "bg-violet-50" },
    { label: "Avg Quality Score", value: avgQsValue !== null ? avgQsValue.toFixed(1) : "—", icon: Star, color: avgQsValue !== null && avgQsValue >= 7 ? "text-emerald-600" : avgQsValue !== null && avgQsValue >= 4 ? "text-amber-600" : "text-red-600", bg: "bg-amber-50" },
  ];

  return (
    <div className="space-y-6">
      <PageHeader
        title="Keywords"
        subtitle={
          <span>
            {isFiltered ? `${keywords.length} of ${allKeywords.length}` : allKeywords.length} keywords &middot;{" "}
            <span className="text-emerald-600">{enabledCount} enabled</span> &middot;{" "}
            <span className="text-amber-600">{pausedCount} paused</span>
            {avgQsValue !== null && (
              <>
                {" "}&middot;{" "}
                <span className={avgQsValue >= 7 ? "text-emerald-600" : avgQsValue >= 4 ? "text-amber-600" : "text-red-600"}>
                  Avg QS: {avgQsValue.toFixed(1)}
                </span>
              </>
            )}
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

      <FilterBar activeCount={activeFilterCount} onClearAll={() => { setCampaignFilter(""); setAdGroupFilter(""); setStatusFilter(""); setMatchFilter(""); setQsFilter(""); }}>
        <SelectFilter label="Campaign" value={campaignFilter} onChange={setCampaignFilter}
          options={campaignOptions.map(([id, name]) => ({ value: id, label: name }))} />
        <SelectFilter label="Ad Group" value={adGroupFilter} onChange={setAdGroupFilter}
          options={adGroupOptions.map(([id, name]) => ({ value: id, label: name }))} />
        <SelectFilter label="Status" value={statusFilter} onChange={setStatusFilter}
          options={[
            { value: "ENABLED", label: "Enabled", count: allEnabledCount },
            { value: "PAUSED", label: "Paused", count: allPausedCount },
          ]} />
        <SelectFilter label="Match Type" value={matchFilter} onChange={setMatchFilter}
          options={matchTypes.map((t) => ({ value: t, label: t }))} />
        <SelectFilter label="Quality Score" value={qsFilter} onChange={setQsFilter}
          options={[
            { value: "high", label: "High (7-10)" },
            { value: "medium", label: "Medium (4-6)" },
            { value: "low", label: "Low (1-3)" },
          ]} />
      </FilterBar>

      {!isLoading && keywords.length === 0 && allKeywords.length === 0 ? (
        <EmptyState icon={<Key className="w-12 h-12" />} title="No keywords found"
          description="No keywords found for this account. Sync data to pull from Google Ads."
          action={<Button variant="primary" onClick={() => refetch()} icon={<RefreshCw className="w-4 h-4" />}>Sync Data</Button>} />
      ) : (
        <DataTable<Keyword & Record<string, unknown>>
          columns={columns}
          data={keywords as (Keyword & Record<string, unknown>)[]}
          keyField="criterion_id"
          loading={isLoading}
          searchable
          searchPlaceholder="Search keywords..."
          searchKeys={["keyword_text", "campaign_name", "ad_group_name"]}
          exportable
          exportFileName={`keywords-${customerId}`}
          emptyMessage={activeFilterCount > 0 ? "No keywords match your filters" : "No keywords found"}
          emptyIcon={<Key className="w-10 h-10" />}
        />
      )}
    </div>
  );
}
