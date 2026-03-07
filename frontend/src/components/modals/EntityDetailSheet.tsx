"use client";

import {
  Sheet,
  SheetContent,
  SheetHeader,
  SheetTitle,
  SheetDescription,
} from "@/components/ui/sheet";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Separator } from "@/components/ui/separator";
import { ScrollArea } from "@/components/ui/scroll-area";
import { StatusBadge } from "@/components/data/StatusBadge";
import Sparkline from "@/components/data/Sparkline";
import {
  formatDollars,
  formatPercent,
  formatNumber,
  formatCurrency,
} from "@/lib/utils";
import {
  ExternalLink,
  Pause,
  Play,
  TrendingUp,
  DollarSign,
  MousePointerClick,
  Eye,
  Target,
} from "lucide-react";

/* ------------------------------------------------------------------ */
/* Metric row                                                         */
/* ------------------------------------------------------------------ */
function MetricRow({
  icon: Icon,
  label,
  value,
}: {
  icon: React.ElementType;
  label: string;
  value: string;
}) {
  return (
    <div className="flex items-center justify-between py-1.5">
      <span className="flex items-center gap-2 text-sm text-muted-foreground">
        <Icon className="size-3.5" />
        {label}
      </span>
      <span className="text-sm font-medium text-foreground">{value}</span>
    </div>
  );
}

/* ------------------------------------------------------------------ */
/* Detail field                                                       */
/* ------------------------------------------------------------------ */
function DetailField({ label, value }: { label: string; value: string }) {
  return (
    <div>
      <span className="text-xs text-muted-foreground">{label}</span>
      <p className="text-sm font-medium text-foreground">{value}</p>
    </div>
  );
}

/* ------------------------------------------------------------------ */
/* Props                                                              */
/* ------------------------------------------------------------------ */

export interface EntityField {
  label: string;
  value: string;
}

export interface EntityMetric {
  icon: React.ElementType;
  label: string;
  value: string;
}

export interface EntityDetailSheetProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  title: string;
  subtitle?: string;
  status?: string;
  entityType?: string;
  fields?: EntityField[];
  metrics?: EntityMetric[];
  sparklineData?: number[];
  actions?: {
    label: string;
    icon?: React.ElementType;
    variant?: "default" | "outline" | "destructive" | "ghost";
    onClick: () => void;
  }[];
  urls?: string[];
}

/* ------------------------------------------------------------------ */
/* Helpers to build props from typed entities                         */
/* ------------------------------------------------------------------ */

export function campaignSheetProps(c: {
  name: string;
  status: string;
  bidding_strategy_type: string;
  budget_per_day: number;
  impressions: number;
  clicks: number;
  cost: number;
  conversions: number;
  ctr: number;
  average_cpc: number;
  roas: number;
}): Partial<EntityDetailSheetProps> {
  return {
    title: c.name,
    status: c.status,
    entityType: "Campaign",
    fields: [
      { label: "Bidding Strategy", value: c.bidding_strategy_type.replace(/_/g, " ") },
      { label: "Daily Budget", value: formatDollars(c.budget_per_day) },
    ],
    metrics: [
      { icon: Eye, label: "Impressions", value: formatNumber(c.impressions) },
      { icon: MousePointerClick, label: "Clicks", value: formatNumber(c.clicks) },
      { icon: DollarSign, label: "Cost", value: formatDollars(c.cost) },
      { icon: Target, label: "Conversions", value: c.conversions.toFixed(1) },
      { icon: TrendingUp, label: "CTR", value: formatPercent(c.ctr) },
      { icon: DollarSign, label: "Avg CPC", value: formatDollars(c.average_cpc) },
      { icon: TrendingUp, label: "ROAS", value: c.roas.toFixed(2) + "x" },
    ],
  };
}

export function adGroupSheetProps(ag: {
  name: string;
  status: string;
  campaign_name: string;
  cpc_bid_micros: number;
  impressions: number;
  clicks: number;
  cost: number;
  conversions: number;
  ctr: number;
}): Partial<EntityDetailSheetProps> {
  return {
    title: ag.name,
    subtitle: ag.campaign_name,
    status: ag.status,
    entityType: "Ad Group",
    fields: [
      { label: "Campaign", value: ag.campaign_name },
      { label: "CPC Bid", value: formatCurrency(ag.cpc_bid_micros) },
    ],
    metrics: [
      { icon: Eye, label: "Impressions", value: formatNumber(ag.impressions) },
      { icon: MousePointerClick, label: "Clicks", value: formatNumber(ag.clicks) },
      { icon: DollarSign, label: "Cost", value: formatDollars(ag.cost) },
      { icon: Target, label: "Conversions", value: ag.conversions.toFixed(1) },
      { icon: TrendingUp, label: "CTR", value: formatPercent(ag.ctr) },
    ],
  };
}

export function keywordSheetProps(kw: {
  keyword_text: string;
  match_type: string;
  status: string;
  ad_group_name: string;
  campaign_name: string;
  cpc_bid_micros: number;
  impressions: number;
  clicks: number;
  cost: number;
  conversions: number;
  ctr: number;
  quality_score: number | null;
}): Partial<EntityDetailSheetProps> {
  return {
    title: kw.keyword_text,
    subtitle: `${kw.campaign_name} › ${kw.ad_group_name}`,
    status: kw.status,
    entityType: "Keyword",
    fields: [
      { label: "Match Type", value: kw.match_type },
      { label: "CPC Bid", value: formatCurrency(kw.cpc_bid_micros) },
      { label: "Quality Score", value: kw.quality_score != null ? String(kw.quality_score) + "/10" : "N/A" },
    ],
    metrics: [
      { icon: Eye, label: "Impressions", value: formatNumber(kw.impressions) },
      { icon: MousePointerClick, label: "Clicks", value: formatNumber(kw.clicks) },
      { icon: DollarSign, label: "Cost", value: formatDollars(kw.cost) },
      { icon: Target, label: "Conversions", value: kw.conversions.toFixed(1) },
      { icon: TrendingUp, label: "CTR", value: formatPercent(kw.ctr) },
    ],
  };
}

export function adSheetProps(ad: {
  id: string;
  type: string;
  status: string;
  ad_group_name: string;
  campaign_name: string;
  headlines: string[];
  descriptions: string[];
  final_urls: string[];
  impressions: number;
  clicks: number;
  cost: number;
  conversions: number;
  ctr: number;
}): Partial<EntityDetailSheetProps> {
  return {
    title: ad.headlines[0] ?? `Ad ${ad.id}`,
    subtitle: `${ad.campaign_name} › ${ad.ad_group_name}`,
    status: ad.status,
    entityType: ad.type.replace(/_/g, " "),
    fields: [
      { label: "Headlines", value: ad.headlines.join(" | ") },
      { label: "Descriptions", value: ad.descriptions.join(" | ") },
    ],
    metrics: [
      { icon: Eye, label: "Impressions", value: formatNumber(ad.impressions) },
      { icon: MousePointerClick, label: "Clicks", value: formatNumber(ad.clicks) },
      { icon: DollarSign, label: "Cost", value: formatDollars(ad.cost) },
      { icon: Target, label: "Conversions", value: ad.conversions.toFixed(1) },
      { icon: TrendingUp, label: "CTR", value: formatPercent(ad.ctr) },
    ],
    urls: ad.final_urls,
  };
}

/* ------------------------------------------------------------------ */
/* Component                                                          */
/* ------------------------------------------------------------------ */

export default function EntityDetailSheet({
  open,
  onOpenChange,
  title,
  subtitle,
  status,
  entityType,
  fields = [],
  metrics = [],
  sparklineData,
  actions = [],
  urls = [],
}: EntityDetailSheetProps) {
  return (
    <Sheet open={open} onOpenChange={onOpenChange}>
      <SheetContent side="right" className="sm:max-w-md w-full">
        <SheetHeader>
          <div className="flex items-center gap-2 flex-wrap">
            {entityType && (
              <Badge variant="outline" className="text-xs capitalize">
                {entityType}
              </Badge>
            )}
            {status && <StatusBadge status={status} />}
          </div>
          <SheetTitle className="text-lg">{title}</SheetTitle>
          {subtitle && <SheetDescription>{subtitle}</SheetDescription>}
        </SheetHeader>

        <ScrollArea className="flex-1 px-4">
          {/* Sparkline */}
          {sparklineData && sparklineData.length > 1 && (
            <div className="mb-4 rounded-lg border bg-muted/30 p-3">
              <span className="text-xs text-muted-foreground mb-1 block">
                Trend
              </span>
              <Sparkline data={sparklineData} width={320} height={48} />
            </div>
          )}

          {/* Detail fields */}
          {fields.length > 0 && (
            <div className="grid grid-cols-2 gap-x-4 gap-y-3 mb-4">
              {fields.map((f) => (
                <DetailField key={f.label} label={f.label} value={f.value} />
              ))}
            </div>
          )}

          {(fields.length > 0 && metrics.length > 0) && <Separator className="my-3" />}

          {/* Metrics */}
          {metrics.length > 0 && (
            <div className="space-y-0.5">
              <span className="text-xs font-medium text-muted-foreground uppercase tracking-wider">
                Performance
              </span>
              {metrics.map((m) => (
                <MetricRow
                  key={m.label}
                  icon={m.icon}
                  label={m.label}
                  value={m.value}
                />
              ))}
            </div>
          )}

          {/* URLs */}
          {urls.length > 0 && (
            <>
              <Separator className="my-3" />
              <div>
                <span className="text-xs font-medium text-muted-foreground uppercase tracking-wider">
                  Landing Pages
                </span>
                {urls.map((url) => (
                  <a
                    key={url}
                    href={url}
                    target="_blank"
                    rel="noopener noreferrer"
                    className="flex items-center gap-1.5 text-sm text-primary hover:underline mt-1.5"
                  >
                    <ExternalLink className="size-3" />
                    {url}
                  </a>
                ))}
              </div>
            </>
          )}
        </ScrollArea>

        {/* Actions */}
        {actions.length > 0 && (
          <div className="flex gap-2 p-4 border-t">
            {actions.map((action) => {
              const ActionIcon = action.icon;
              return (
                <Button
                  key={action.label}
                  variant={action.variant ?? "outline"}
                  size="sm"
                  onClick={action.onClick}
                >
                  {ActionIcon && <ActionIcon className="size-4 mr-1.5" />}
                  {action.label}
                </Button>
              );
            })}
          </div>
        )}
      </SheetContent>
    </Sheet>
  );
}

export { EntityDetailSheet };
