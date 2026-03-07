"use client";

import { useQuery, useMutation, useQueryClient } from "@tanstack/react-query";
import {
  Sparkles,
  Check,
  X,
  TrendingUp,
  AlertTriangle,
  Zap,
  Target,
  Shield,
} from "lucide-react";
import { useAccount } from "@/components/AccountContext";
import PageHeader from "@/components/layout/PageHeader";
import StatusBadge from "@/components/data/StatusBadge";
import Button from "@/components/data/LoadingButton";
import LoadingSpinner from "@/components/feedback/LoadingSpinner";
import ErrorBanner from "@/components/feedback/ErrorBanner";
import RecommendationConfirmDialog from "@/components/modals/RecommendationConfirmDialog";
import { SelectFilter, FilterBar } from "@/components/forms";
import { useToast } from "@/hooks/use-toast";
import { Card, CardContent } from "@/components/ui/card";
import { Badge } from "@/components/ui/badge";
import { Progress } from "@/components/ui/progress";
import { Tabs, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { ScrollArea } from "@/components/ui/scroll-area";
import {
  fetchRecommendations,
  fetchCampaigns,
  generateRecommendations,
  applyRecommendation,
  previewRecommendation,
  dismissRecommendation,
} from "@/lib/api";
import type { Recommendation } from "@/lib/types";
import { useState, useMemo } from "react";

const TYPE_COLORS: Record<string, { bg: string; text: string; icon: typeof TrendingUp }> = {
  bid_adjustment: { bg: "bg-purple-50 dark:bg-purple-950 border-purple-200 dark:border-purple-800", text: "text-purple-700 dark:text-purple-300", icon: TrendingUp },
  budget_reallocation: { bg: "bg-emerald-50 dark:bg-emerald-950 border-emerald-200 dark:border-emerald-800", text: "text-emerald-700 dark:text-emerald-300", icon: Zap },
  pause_underperformer: { bg: "bg-red-50 dark:bg-red-950 border-red-200 dark:border-red-800", text: "text-red-700 dark:text-red-300", icon: AlertTriangle },
  enable_campaign: { bg: "bg-blue-50 dark:bg-blue-950 border-blue-200 dark:border-blue-800", text: "text-blue-700 dark:text-blue-300", icon: Zap },
  add_keyword: { bg: "bg-teal-50 dark:bg-teal-950 border-teal-200 dark:border-teal-800", text: "text-teal-700 dark:text-teal-300", icon: Target },
  add_negative_keyword: { bg: "bg-orange-50 dark:bg-orange-950 border-orange-200 dark:border-orange-800", text: "text-orange-700 dark:text-orange-300", icon: Shield },
  ad_copy_suggestion: { bg: "bg-pink-50 dark:bg-pink-950 border-pink-200 dark:border-pink-800", text: "text-pink-700 dark:text-pink-300", icon: Sparkles },
  bid_strategy_change: { bg: "bg-indigo-50 dark:bg-indigo-950 border-indigo-200 dark:border-indigo-800", text: "text-indigo-700 dark:text-indigo-300", icon: TrendingUp },
};

const FILTER_TABS = ["all", "pending", "applied", "dismissed"] as const;

export default function RecommendationsPage() {
  const { customerId } = useAccount();
  const queryClient = useQueryClient();
  const [filter, setFilter] = useState<string>("all");
  const [typeFilter, setTypeFilter] = useState("");
  const [campaignFilter, setCampaignFilter] = useState("");
  const [activeCampaignsOnly, setActiveCampaignsOnly] = useState(false);
  const [previewRec, setPreviewRec] = useState<Recommendation | null>(null);
  const [previewLoading, setPreviewLoading] = useState(false);
  const { addToast } = useToast();

  const { data, isLoading, error } = useQuery({
    queryKey: ["recommendations", customerId],
    queryFn: () => fetchRecommendations(customerId),
    enabled: !!customerId,
  });

  const { data: campaignsData } = useQuery({
    queryKey: ["campaigns", customerId, 30],
    queryFn: () => fetchCampaigns(customerId, 30),
    enabled: !!customerId,
  });

  const enabledCampaignNames = useMemo(() => {
    const campaigns = campaignsData?.campaigns ?? [];
    return new Set(campaigns.filter((c) => c.status === "ENABLED").map((c) => c.name));
  }, [campaignsData]);

  const generateMutation = useMutation({
    mutationFn: (days: number) => generateRecommendations(customerId, days),
    onSuccess: (data) => {
      queryClient.invalidateQueries({ queryKey: ["recommendations", customerId] });
      const count = data?.recommendations?.length ?? 0;
      addToast("success", "Recommendations Generated", `${count} new recommendation${count !== 1 ? "s" : ""} created.`);
    },
    onError: (err) => addToast("error", "Generation Failed", (err as Error).message),
  });

  const applyMutation = useMutation({
    mutationFn: (id: number) => applyRecommendation(customerId, id, true),
    onSuccess: () => {
      const title = previewRec?.title ?? "Recommendation";
      setPreviewRec(null);
      queryClient.invalidateQueries({ queryKey: ["recommendations", customerId] });
      addToast("success", "Applied Successfully", `"${title}" has been applied to your Google Ads account.`);
    },
    onError: (err) => addToast("error", "Apply Failed", (err as Error).message),
  });

  const handleApplyClick = async (rec: Recommendation) => {
    setPreviewLoading(true);
    try {
      const full = await previewRecommendation(customerId, rec.id);
      setPreviewRec(full);
    } catch {
      setPreviewRec(rec);
    } finally {
      setPreviewLoading(false);
    }
  };

  const dismissMutation = useMutation({
    mutationFn: (id: number) => dismissRecommendation(customerId, id),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ["recommendations", customerId] });
      addToast("info", "Recommendation Dismissed");
    },
    onError: (err) => addToast("error", "Dismiss Failed", (err as Error).message),
  });

  const allRecs = data?.recommendations ?? [];

  const typeOptions = useMemo(
    () => [...new Set(allRecs.map((r) => r.type))].filter(Boolean).sort(),
    [allRecs],
  );
  const campaignOptions = useMemo(
    () => [...new Set(allRecs.map((r) => r.entity_name))].filter(Boolean).sort(),
    [allRecs],
  );

  const recs = useMemo(() => {
    let filtered = allRecs;
    if (filter !== "all") filtered = filtered.filter((r) => r.status === filter);
    if (typeFilter) filtered = filtered.filter((r) => r.type === typeFilter);
    if (campaignFilter) filtered = filtered.filter((r) => r.entity_name === campaignFilter);
    if (activeCampaignsOnly && enabledCampaignNames.size > 0) {
      filtered = filtered.filter((r) =>
        [...enabledCampaignNames].some((name) => r.entity_name?.startsWith(name))
      );
    }
    return filtered;
  }, [allRecs, filter, typeFilter, campaignFilter, activeCampaignsOnly, enabledCampaignNames]);

  const filterCounts = useMemo(() => ({
    all: allRecs.length,
    pending: allRecs.filter((r) => r.status === "pending").length,
    applied: allRecs.filter((r) => r.status === "applied").length,
    dismissed: allRecs.filter((r) => r.status === "dismissed").length,
  }), [allRecs]);

  const activeFilterCount = [typeFilter, campaignFilter, activeCampaignsOnly ? "active" : ""].filter(Boolean).length;

  if (isLoading) {
    return <LoadingSpinner size="lg" message="Loading recommendations..." className="py-20" />;
  }

  return (
    <div className="space-y-6">
      <PageHeader
        title="AI Recommendations"
        badge={filterCounts.pending > 0 ? `${filterCounts.pending} pending` : undefined}
        subtitle={`${filterCounts.pending} pending · ${filterCounts.applied} applied${activeFilterCount > 0 ? ` · Showing ${recs.length} of ${allRecs.length}` : ""}`}
        actions={
          <Button
            onClick={() => generateMutation.mutate(30)}
            loading={generateMutation.isPending}
            icon={<Sparkles className="w-4 h-4" />}
          >
            Generate New
          </Button>
        }
      />

      {error && <ErrorBanner message={(error as Error).message} />}
      {generateMutation.isError && <ErrorBanner message={(generateMutation.error as Error).message} />}

      {/* Filter tabs - shadcn Tabs */}
      <Tabs value={filter} onValueChange={setFilter}>
        <TabsList>
          {FILTER_TABS.map((f) => (
            <TabsTrigger key={f} value={f} className="capitalize">
              {f}
              <Badge variant="secondary" className="ml-1.5 text-xs">
                {filterCounts[f as keyof typeof filterCounts]}
              </Badge>
            </TabsTrigger>
          ))}
        </TabsList>
      </Tabs>

      {/* Filters */}
      <FilterBar activeCount={activeFilterCount} onClearAll={() => { setTypeFilter(""); setCampaignFilter(""); setActiveCampaignsOnly(false); }}>
        <SelectFilter label="Type" value={typeFilter} onChange={setTypeFilter}
          options={typeOptions.map((t) => ({ value: t, label: t.replace(/_/g, " ") }))} />
        <SelectFilter label="Campaign" value={campaignFilter} onChange={setCampaignFilter}
          options={campaignOptions.map((name) => ({ value: name, label: name }))} />
        <button
          onClick={() => setActiveCampaignsOnly(!activeCampaignsOnly)}
          className={`flex items-center gap-1.5 px-3 py-1.5 rounded-lg text-xs font-medium border transition-all whitespace-nowrap ${
            activeCampaignsOnly
              ? "bg-emerald-50 dark:bg-emerald-950 border-emerald-300 dark:border-emerald-700 text-emerald-700 dark:text-emerald-300"
              : "bg-card border-border text-muted-foreground hover:border-border hover:text-foreground"
          }`}
        >
          <Zap className="w-3 h-3" />
          Active campaigns only
        </button>
      </FilterBar>

      {/* Recommendation cards */}
      <ScrollArea className="max-h-[70vh]">
        <div className="space-y-4 stagger-children">
          {recs.length === 0 ? (
            <Card className="animate-fade-in">
              <CardContent className="p-16 text-center">
                <div className="w-16 h-16 rounded-2xl bg-muted flex items-center justify-center mx-auto mb-4">
                  <Sparkles className="w-8 h-8 text-muted-foreground" />
                </div>
                <p className="text-foreground font-medium">No recommendations yet</p>
                <p className="text-sm text-muted-foreground mt-1">Click &quot;Generate New&quot; to analyze your account.</p>
              </CardContent>
            </Card>
          ) : (
            recs.map((rec: Recommendation) => {
              const typeInfo = TYPE_COLORS[rec.type] ?? { bg: "bg-muted border-border", text: "text-muted-foreground", icon: Sparkles };
              const TypeIcon = typeInfo.icon;
              return (
                <Card key={rec.id} className="group hover:shadow-md transition-all animate-fade-in overflow-hidden">
                  <CardContent className="p-5">
                    <div className="flex items-start justify-between gap-4">
                      <div className="flex-1 min-w-0">
                        <div className="flex items-center gap-2.5 mb-2.5 flex-wrap">
                          <div className={`w-8 h-8 rounded-lg ${typeInfo.bg} border flex items-center justify-center shrink-0`}>
                            <TypeIcon className={`w-4 h-4 ${typeInfo.text}`} />
                          </div>
                          <Badge variant="outline" className={`text-xs capitalize ${typeInfo.text}`}>
                            {rec.type.replace(/_/g, " ")}
                          </Badge>
                          <StatusBadge status={rec.status} />
                          {rec.confidence_score != null && (
                            <div className="flex items-center gap-1.5" title={`Confidence: ${(rec.confidence_score * 100).toFixed(0)}%`}>
                              <Progress
                                value={rec.confidence_score * 100}
                                className="w-16 h-1.5"
                              />
                              <span className="text-[10px] text-muted-foreground font-mono">{(rec.confidence_score * 100).toFixed(0)}%</span>
                            </div>
                          )}
                        </div>

                        <h3 className="text-base font-semibold text-foreground group-hover:text-primary transition-colors">{rec.title}</h3>
                        <p className="text-sm text-muted-foreground mt-1 leading-relaxed">{rec.description}</p>

                        <div className="mt-3 flex flex-wrap gap-4 text-xs">
                          {rec.entity_name && (
                            <div className="flex items-center gap-1.5">
                              <span className="text-muted-foreground">Entity:</span>
                              <span className="font-medium text-foreground">{rec.entity_name}</span>
                            </div>
                          )}
                          {rec.current_value && (
                            <div className="flex items-center gap-1.5">
                              <span className="text-muted-foreground">Current:</span>
                              <span className="font-medium text-foreground">{rec.current_value}</span>
                            </div>
                          )}
                          {rec.recommended_value && (
                            <div className="flex items-center gap-1.5">
                              <span className="text-muted-foreground">Recommended:</span>
                              <span className="font-semibold text-primary">{rec.recommended_value}</span>
                            </div>
                          )}
                          {rec.impact_estimate && (
                            <div className="flex items-center gap-1.5">
                              <Zap className="w-3 h-3 text-emerald-500" />
                              <span className="font-semibold text-emerald-700 dark:text-emerald-300">{rec.impact_estimate}</span>
                            </div>
                          )}
                        </div>
                      </div>

                      {rec.status === "pending" && (
                        <div className="flex gap-2 shrink-0">
                          <Button variant="primary" size="sm" loading={previewLoading}
                            onClick={() => handleApplyClick(rec)}
                          >
                            <Check className="w-4 h-4 mr-1" /> Apply
                          </Button>
                          <Button variant="ghost" size="sm" loading={dismissMutation.isPending}
                            onClick={() => dismissMutation.mutate(rec.id)}
                            className="text-muted-foreground hover:text-red-600 hover:bg-red-50 dark:hover:bg-red-950"
                          >
                            <X className="w-4 h-4" />
                          </Button>
                        </div>
                      )}
                    </div>
                  </CardContent>
                </Card>
              );
            })
          )}
        </div>
      </ScrollArea>

      <RecommendationConfirmDialog
        recommendation={previewRec}
        loading={applyMutation.isPending}
        onConfirm={() => previewRec && applyMutation.mutate(previewRec.id)}
        onCancel={() => setPreviewRec(null)}
      />
    </div>
  );
}
