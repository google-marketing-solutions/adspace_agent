"use client";

import { useQuery } from "@tanstack/react-query";
import {
  DollarSign,
  MousePointerClick,
  Eye,
  Target,
  TrendingUp,
  RefreshCw,
  ArrowUpRight,
  ArrowDownRight,
  Lightbulb,
  Sparkles,
  Activity,
  Zap,
} from "lucide-react";
import Link from "next/link";
import { useAccount } from "@/components/AccountContext";
import PageHeader from "@/components/layout/PageHeader";
import { SelectFilter, DateRangeFilter, FilterBar, resolveDays } from "@/components/forms";
import PerformanceChart from "@/components/data/PerformanceChart";
import StatusBadge from "@/components/data/StatusBadge";
import Button from "@/components/data/LoadingButton";
import ErrorBanner from "@/components/feedback/ErrorBanner";
import { KpiCardsSkeleton } from "@/components/feedback/Skeleton";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { ScrollArea } from "@/components/ui/scroll-area";
import { Badge } from "@/components/ui/badge";
import {
  Tooltip,
  TooltipContent,
  TooltipProvider,
  TooltipTrigger,
} from "@/components/ui/tooltip";
import { useToast } from "@/hooks/use-toast";
import { fetchMetrics, fetchRecommendations } from "@/lib/api";
import { formatDollars, formatNumber, formatPercent } from "@/lib/utils";
import type { Campaign } from "@/lib/types";
import { useState, useMemo } from "react";

export default function DashboardPage() {
  const { customerId } = useAccount();
  const { addToast } = useToast();
  const [lastRefresh, setLastRefresh] = useState<Date | null>(null);
  const [daysSelection, setDaysSelection] = useState(30);
  const days = resolveDays(daysSelection);
  const [statusFilter, setStatusFilter] = useState("");
  const [campaignFilter, setCampaignFilter] = useState("");

  const {
    data: perfData,
    isLoading,
    error,
    isFetching,
    refetch,
  } = useQuery({
    queryKey: ["metrics", customerId, days],
    queryFn: () => fetchMetrics(customerId, days),
    enabled: !!customerId,
  });

  const { data: recsData } = useQuery({
    queryKey: ["recommendations", customerId],
    queryFn: () => fetchRecommendations(customerId),
    enabled: !!customerId,
  });

  const handleRefresh = () => {
    refetch().then(() => {
      setLastRefresh(new Date());
      addToast("success", "Data refreshed", "Dashboard data has been updated from Google Ads.");
    });
  };

  const allCampaigns = perfData?.campaigns ?? [];

  const campaigns = useMemo(() => {
    let filtered = allCampaigns;
    if (statusFilter) filtered = filtered.filter((c) => c.status === statusFilter);
    if (campaignFilter) filtered = filtered.filter((c) => c.id === campaignFilter);
    return filtered;
  }, [allCampaigns, statusFilter, campaignFilter]);

  const summary = useMemo(() => {
    if (!perfData?.summary) return undefined;
    if (!statusFilter && !campaignFilter) return perfData.summary;
    const totalCost = campaigns.reduce((s, c) => s + c.cost, 0);
    const totalClicks = campaigns.reduce((s, c) => s + c.clicks, 0);
    const totalImpressions = campaigns.reduce((s, c) => s + c.impressions, 0);
    const totalConversions = campaigns.reduce((s, c) => s + c.conversions, 0);
    const totalConversionValue = campaigns.reduce((s, c) => s + (c.conversion_value ?? 0), 0);
    return {
      total_cost: totalCost,
      total_clicks: totalClicks,
      total_impressions: totalImpressions,
      total_conversions: totalConversions,
      total_conversion_value: totalConversionValue,
      overall_ctr: totalImpressions > 0 ? (totalClicks / totalImpressions) * 100 : 0,
      overall_cpc: totalClicks > 0 ? totalCost / totalClicks : 0,
      overall_roas: totalCost > 0 ? totalConversionValue / totalCost : 0,
      cost_per_conversion: totalConversions > 0 ? totalCost / totalConversions : 0,
    };
  }, [perfData?.summary, campaigns, statusFilter, campaignFilter]);

  const pendingRecs = recsData?.recommendations?.filter((r) => r.status === "pending") ?? [];

  const chartData = campaigns.slice(0, 8).map((c: Campaign) => ({
    name: c.name.length > 20 ? c.name.slice(0, 20) + "…" : c.name,
    Cost: c.cost,
    Conversions: c.conversions,
    Clicks: c.clicks,
  }));

  const statusCounts = useMemo(() => {
    const enabled = allCampaigns.filter((c) => c.status === "ENABLED").length;
    const paused = allCampaigns.filter((c) => c.status === "PAUSED").length;
    return { enabled, paused };
  }, [allCampaigns]);

  const campaignOptions = useMemo(
    () => allCampaigns.map((c) => ({ value: c.id, label: c.name })),
    [allCampaigns]
  );

  const activeFilterCount = [statusFilter, campaignFilter].filter(Boolean).length;
  const dateLabel = daysSelection === -1 ? "Month to Date" : `${days} days`;

  const kpis = [
    { title: "Total Spend", value: formatDollars(summary?.total_cost ?? 0), icon: DollarSign, color: "text-blue-600 dark:text-blue-400", bg: "bg-blue-50 dark:bg-blue-950" },
    { title: "Clicks", value: formatNumber(summary?.total_clicks ?? 0), icon: MousePointerClick, color: "text-violet-600 dark:text-violet-400", bg: "bg-violet-50 dark:bg-violet-950" },
    { title: "Impressions", value: formatNumber(summary?.total_impressions ?? 0), icon: Eye, color: "text-cyan-600 dark:text-cyan-400", bg: "bg-cyan-50 dark:bg-cyan-950" },
    { title: "Conversions", value: (summary?.total_conversions ?? 0).toFixed(1), icon: Target, color: "text-emerald-600 dark:text-emerald-400", bg: "bg-emerald-50 dark:bg-emerald-950" },
    { title: "ROAS", value: `${(summary?.overall_roas ?? 0).toFixed(2)}x`, icon: TrendingUp, color: "text-amber-600 dark:text-amber-400", bg: "bg-amber-50 dark:bg-amber-950" },
  ];

  return (
    <TooltipProvider>
      <div className="space-y-6">
        <PageHeader
          title="Dashboard"
          subtitle={
            <span>
              Account {customerId} &middot; Last {dateLabel}
              {lastRefresh && (
                <span className="ml-2 text-emerald-600 dark:text-emerald-400">
                  &middot; Updated {lastRefresh.toLocaleTimeString()}
                </span>
              )}
            </span>
          }
          actions={
            <div className="flex items-center gap-3">
              <DateRangeFilter value={daysSelection} onChange={setDaysSelection} />
              <Button
                variant="primary"
                onClick={handleRefresh}
                loading={isFetching}
                icon={<RefreshCw className={`w-4 h-4 ${isFetching ? "animate-spin" : ""}`} />}
              >
                {isFetching ? "Syncing…" : "Sync Data"}
              </Button>
            </div>
          }
        />

        <FilterBar
          activeCount={activeFilterCount}
          onClearAll={() => { setStatusFilter(""); setCampaignFilter(""); }}
        >
          <SelectFilter
            label="Status"
            value={statusFilter}
            onChange={setStatusFilter}
            options={[
              { value: "ENABLED", label: "Enabled", count: statusCounts.enabled },
              { value: "PAUSED", label: "Paused", count: statusCounts.paused },
            ]}
          />
          <SelectFilter
            label="Campaign"
            value={campaignFilter}
            onChange={setCampaignFilter}
            options={campaignOptions}
          />
        </FilterBar>

        {error && <ErrorBanner message={(error as Error).message} />}

        {/* KPI Cards */}
        {isLoading ? (
          <KpiCardsSkeleton count={5} />
        ) : (
          <div className="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-5 gap-4 stagger-children">
            {kpis.map((kpi) => {
              const Icon = kpi.icon;
              return (
                <Tooltip key={kpi.title}>
                  <TooltipTrigger asChild>
                    <Card className="group relative overflow-hidden hover:shadow-md transition-all duration-200 animate-fade-in">
                      <CardContent className="p-5">
                        <div className="flex items-start justify-between">
                          <div className="space-y-1">
                            <p className="text-xs font-medium text-muted-foreground uppercase tracking-wider">{kpi.title}</p>
                            <p className="text-2xl font-bold text-foreground tracking-tight">{kpi.value}</p>
                          </div>
                          <div className={`p-2.5 ${kpi.bg} rounded-xl ${kpi.color} group-hover:scale-110 transition-transform`}>
                            <Icon className="w-5 h-5" />
                          </div>
                        </div>
                      </CardContent>
                    </Card>
                  </TooltipTrigger>
                  <TooltipContent>
                    <p>{kpi.title} over the last {dateLabel}</p>
                  </TooltipContent>
                </Tooltip>
              );
            })}
          </div>
        )}

        {/* Charts with Tabs */}
        {!isLoading && chartData.length > 0 && (
          <Tabs defaultValue="cost-conv" className="animate-fade-in">
            <TabsList>
              <TabsTrigger value="cost-conv">Cost vs Conversions</TabsTrigger>
              <TabsTrigger value="clicks">Clicks</TabsTrigger>
            </TabsList>
            <TabsContent value="cost-conv">
              <PerformanceChart
                title="Campaign Cost vs Conversions"
                data={chartData}
                dataKeys={[
                  { key: "Cost", color: "#6366f1", label: "Cost ($)" },
                  { key: "Conversions", color: "#10b981", label: "Conversions" },
                ]}
              />
            </TabsContent>
            <TabsContent value="clicks">
              <PerformanceChart
                title="Campaign Clicks"
                data={chartData}
                dataKeys={[{ key: "Clicks", color: "#8b5cf6", label: "Clicks" }]}
              />
            </TabsContent>
          </Tabs>
        )}

        {/* Two columns: Top Campaigns + AI Recommendations */}
        <div className="grid grid-cols-1 lg:grid-cols-2 gap-6 animate-fade-in">
          {/* Top Campaigns */}
          <Card>
            <CardHeader className="flex flex-row items-center justify-between pb-3">
              <div className="flex items-center gap-2">
                <div className="p-1.5 bg-primary/10 rounded-lg">
                  <Activity className="w-4 h-4 text-primary" />
                </div>
                <CardTitle className="text-sm">Top Campaigns by Spend</CardTitle>
              </div>
              <Link
                href="/campaigns"
                className="text-xs text-primary hover:text-primary/80 font-medium flex items-center gap-1 hover:gap-2 transition-all"
              >
                View all
                <ArrowUpRight className="w-3 h-3" />
              </Link>
            </CardHeader>
            <CardContent>
              <ScrollArea className="max-h-[400px]">
                {isLoading ? (
                  <div className="space-y-4">
                    {[1, 2, 3, 4, 5].map((i) => (
                      <div key={i} className="flex items-center justify-between animate-shimmer rounded-lg p-3">
                        <div className="space-y-2">
                          <div className="h-4 bg-muted rounded w-48" />
                          <div className="h-3 bg-muted rounded w-32" />
                        </div>
                        <div className="h-5 bg-muted rounded w-16" />
                      </div>
                    ))}
                  </div>
                ) : (
                  <div className="space-y-1">
                    {campaigns.slice(0, 5).map((c: Campaign, idx: number) => (
                      <Link
                        key={c.id}
                        href="/campaigns"
                        className="flex items-center justify-between py-3 px-3 rounded-lg hover:bg-muted/50 transition-colors group"
                      >
                        <div className="flex items-center gap-3 flex-1 min-w-0">
                          <span className="text-xs font-mono text-muted-foreground w-5">{idx + 1}</span>
                          <div className="min-w-0">
                            <p className="text-sm font-medium text-foreground truncate group-hover:text-primary transition-colors">
                              {c.name}
                            </p>
                            <p className="text-xs text-muted-foreground">
                              {formatNumber(c.clicks)} clicks &middot; {formatPercent(c.ctr)} CTR
                            </p>
                          </div>
                        </div>
                        <div className="text-right ml-4 flex items-center gap-3">
                          <div>
                            <p className="text-sm font-semibold text-foreground">{formatDollars(c.cost)}</p>
                            <div className="flex items-center justify-end gap-1">
                              {c.roas >= 1 ? (
                                <ArrowUpRight className="w-3 h-3 text-emerald-500" />
                              ) : (
                                <ArrowDownRight className="w-3 h-3 text-red-500" />
                              )}
                              <span className={`text-xs font-semibold ${c.roas >= 1 ? "text-emerald-600 dark:text-emerald-400" : "text-red-600 dark:text-red-400"}`}>
                                {c.roas.toFixed(2)}x
                              </span>
                            </div>
                          </div>
                          <StatusBadge status={c.status} />
                        </div>
                      </Link>
                    ))}
                    {campaigns.length === 0 && (
                      <p className="text-sm text-muted-foreground text-center py-8">No campaign data</p>
                    )}
                  </div>
                )}
              </ScrollArea>
            </CardContent>
          </Card>

          {/* Pending Recommendations */}
          <Card>
            <CardHeader className="flex flex-row items-center justify-between pb-3">
              <div className="flex items-center gap-2">
                <div className="p-1.5 bg-amber-50 dark:bg-amber-950 rounded-lg">
                  <Lightbulb className="w-4 h-4 text-amber-600 dark:text-amber-400" />
                </div>
                <CardTitle className="text-sm">AI Recommendations</CardTitle>
                {pendingRecs.length > 0 && (
                  <Badge variant="secondary" className="text-xs">
                    {pendingRecs.length}
                  </Badge>
                )}
              </div>
              <Link
                href="/recommendations"
                className="text-xs text-primary hover:text-primary/80 font-medium flex items-center gap-1 hover:gap-2 transition-all"
              >
                View all
                <ArrowUpRight className="w-3 h-3" />
              </Link>
            </CardHeader>
            <CardContent>
              <ScrollArea className="max-h-[400px]">
                <div className="space-y-3">
                  {pendingRecs.slice(0, 5).map((r) => (
                    <div
                      key={r.id}
                      className="group p-3.5 border border-border rounded-xl hover:border-primary/30 hover:bg-primary/5 transition-all duration-200 cursor-pointer"
                    >
                      <div className="flex items-start justify-between gap-2">
                        <div className="flex-1 min-w-0">
                          <div className="flex items-center gap-2 mb-1">
                            <Sparkles className="w-3.5 h-3.5 text-primary" />
                            <p className="text-sm font-medium text-foreground truncate">{r.title}</p>
                          </div>
                          <p className="text-xs text-muted-foreground line-clamp-2 pl-5.5">{r.description}</p>
                        </div>
                        {r.confidence_score != null && (
                          <Badge
                            variant={r.confidence_score >= 70 ? "default" : "secondary"}
                            className="text-xs"
                          >
                            {r.confidence_score}%
                          </Badge>
                        )}
                      </div>
                      {r.impact_estimate && (
                        <p className="text-xs text-emerald-600 dark:text-emerald-400 mt-2 font-medium pl-5.5">
                          <Zap className="w-3 h-3 inline mr-1" />
                          {r.impact_estimate}
                        </p>
                      )}
                    </div>
                  ))}
                  {pendingRecs.length === 0 && (
                    <div className="text-center py-8">
                      <div className="w-12 h-12 mx-auto mb-3 bg-muted rounded-xl flex items-center justify-center">
                        <Lightbulb className="w-6 h-6 text-muted-foreground" />
                      </div>
                      <p className="text-sm text-muted-foreground mb-3">No pending recommendations</p>
                      <Link href="/recommendations">
                        <Button variant="secondary" size="sm" icon={<Sparkles className="w-3.5 h-3.5" />}>
                          Generate
                        </Button>
                      </Link>
                    </div>
                  )}
                </div>
              </ScrollArea>
            </CardContent>
          </Card>
        </div>
      </div>
    </TooltipProvider>
  );
}
