// Types matching the FastAPI backend Pydantic models

// === Accounts ===
export interface Account {
  id: string;
  descriptive_name: string;
  currency_code: string;
  time_zone: string;
  manager: boolean;
  children: Account[];
}

// === Campaigns ===
export interface Campaign {
  id: string;
  name: string;
  status: string;
  bidding_strategy_type: string;
  budget_amount_micros: number;
  budget_per_day: number;
  impressions: number;
  clicks: number;
  cost_micros: number;
  cost: number;
  conversions: number;
  conversion_value: number;
  ctr: number;
  average_cpc: number;
  roas: number;
}

// === Ad Groups ===
export interface AdGroup {
  id: string;
  name: string;
  status: string;
  campaign_id: string;
  campaign_name: string;
  cpc_bid_micros: number;
  impressions: number;
  clicks: number;
  cost_micros: number;
  cost: number;
  conversions: number;
  ctr: number;
}

// === Keywords ===
export interface Keyword {
  criterion_id: string;
  keyword_text: string;
  match_type: string;
  status: string;
  ad_group_id: string;
  ad_group_name: string;
  campaign_id: string;
  campaign_name: string;
  cpc_bid_micros: number;
  impressions: number;
  clicks: number;
  cost_micros: number;
  cost: number;
  conversions: number;
  ctr: number;
  quality_score: number | null;
}

// === Ads ===
export interface Ad {
  id: string;
  ad_group_id: string;
  ad_group_name: string;
  campaign_id: string;
  campaign_name: string;
  type: string;
  status: string;
  headlines: string[];
  descriptions: string[];
  final_urls: string[];
  impressions: number;
  clicks: number;
  cost_micros: number;
  cost: number;
  conversions: number;
  ctr: number;
}

// === Recommendations ===
export interface Recommendation {
  id: number;
  customer_id: string;
  type: string;
  status: string;
  title: string;
  description: string;
  impact_estimate: string;
  entity_type: string;
  entity_id: string;
  entity_name: string;
  current_value: string;
  recommended_value: string;
  mutate_payload: Record<string, unknown> | null;
  confidence_score: number | null;
  created_at: string | null;
}

// === Account Profile ===
export interface AccountProfile {
  id: number;
  customer_id: string;
  business_name: string;
  industry_vertical: string;
  sub_vertical: string;
  target_roas: number | null;
  target_cpa: number | null;
  monthly_budget_cap: number | null;
  brand_guidelines: string;
  compliance_notes: string;
  restricted_terms: string[];
  created_at: string | null;
  updated_at: string | null;
}

// === Metrics ===
export interface MetricsSummary {
  total_cost: number;
  total_clicks: number;
  total_impressions: number;
  total_conversions: number;
  total_conversion_value: number;
  overall_ctr: number;
  overall_cpc: number;
  overall_roas: number;
  cost_per_conversion: number;
}

export interface PerformanceSummary {
  period: string;
  summary: MetricsSummary;
  campaigns: Campaign[];
}

// === Mutation Log ===
export interface MutationLog {
  id: number;
  customer_id: string;
  operation_type: string;
  entity_type: string;
  entity_id: string | null;
  resource_name: string | null;
  details: string | null;
  status: string;
  error_message: string | null;
  created_at: string | null;
}

// === Chat ===
export interface ChatMessage {
  role: "user" | "assistant";
  content: string;
  timestamp?: string;
}

// === Campaign Builder ===
export type BlueprintStatus =
  | "DRAFT"
  | "AUDIT_COMPLETE"
  | "STRATEGY_READY"
  | "CREATIVE_READY"
  | "BUILDING"
  | "BUILT"
  | "REVIEW_PASSED"
  | "DEPLOYED"
  | "FAILED"
  | "EXPIRED";

export type BuilderPhase =
  | "DATA_AUDIT"
  | "DEEP_ANALYSIS"
  | "STRATEGY_GENERATION"
  | "CREATIVE_GENERATION"
  | "BUILD_DEPLOY"
  | "REVIEW_OPTIMIZE";

export interface BlueprintSummary {
  id: number;
  customer_id: string;
  name: string;
  status: BlueprintStatus;
  current_phase: BuilderPhase | null;
  campaign_type: string | null;
  created_at: string | null;
  updated_at: string | null;
}

export interface BlueprintDetail extends BlueprintSummary {
  audit_data: Record<string, unknown> | null;
  analysis_summary: Record<string, unknown> | null;
  strategies: Record<string, unknown>[] | null;
  selected_strategy_index: number | null;
  creative_assets: Record<string, unknown> | null;
  build_result: Record<string, unknown> | null;
  review_report: Record<string, unknown> | null;
  campaign_resource_name: string | null;
  error_message: string | null;
  daily_budget_micros: number | null;
  target_locations: string[] | null;
  target_languages: string[] | null;
  keyword_themes: string[] | null;
  bidding_strategy: string | null;
  expires_at: string | null;
}

export interface AuditReportDetail {
  id: number;
  customer_id: string;
  blueprint_id: number;
  lookback_days: number;
  account_summary: Record<string, unknown> | null;
  campaign_performance: Record<string, unknown> | null;
  keyword_performance: Record<string, unknown> | null;
  search_term_analysis: Record<string, unknown> | null;
  quality_score_distribution: Record<string, unknown> | null;
  geo_performance: Record<string, unknown> | null;
  device_performance: Record<string, unknown> | null;
  top_opportunities: Record<string, unknown> | null;
  wasted_spend: Record<string, unknown> | null;
  created_at: string | null;
}

// === CSV Bulk Upload ===
export interface CsvRowPreview {
  row_number: number;
  name: string;
  campaign_type: string;
  daily_budget: number | null;
  locations: string[];
  languages: string[];
  keywords: string[];
  valid: boolean;
  errors: string[];
  source_file?: string;
}

export interface CsvFileStatus {
  filename: string;
  status: "ok" | "error";
  error: string | null;
  row_count: number;
}

export interface CsvParseResponse {
  status: string;
  customer_id: string;
  rows: CsvRowPreview[];
  valid_count: number;
  error_count: number;
  error?: string;
  files_parsed?: number;
  file_statuses?: CsvFileStatus[];
}

export interface BatchStartItem {
  name: string;
  campaign_type?: string;
  daily_budget_micros?: number;
  target_locations?: string[];
  target_languages?: string[];
  keyword_themes?: string[];
}

export interface BatchStartResponse {
  status: string;
  created: number;
  blueprints: BlueprintSummary[];
}

// === Generic ===
export interface StatusResponse {
  status: string;
  message: string;
  error_details?: string;
}
