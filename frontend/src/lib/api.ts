/**
 * API client for the AdSpace Agent FastAPI backend.
 * All endpoints are prefixed with /api.
 */

import type {
  Account,
  AccountProfile,
  Ad,
  AdGroup,
  AuditReportDetail,
  BatchStartItem,
  BatchStartResponse,
  BlueprintDetail,
  BlueprintSummary,
  Campaign,
  ChatMessage,
  CsvParseResponse,
  Keyword,
  MutationLog,
  PerformanceSummary,
  Recommendation,
  StatusResponse,
} from "./types";

const API_BASE = process.env.NEXT_PUBLIC_API_URL ?? "http://localhost:8000/api";

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

async function request<T>(
  path: string,
  options?: RequestInit
): Promise<T> {
  const res = await fetch(`${API_BASE}${path}`, {
    headers: { "Content-Type": "application/json", ...options?.headers },
    ...options,
  });
  if (!res.ok) {
    const body = await res.json().catch(() => ({}));
    throw new Error(
      body?.detail ?? body?.message ?? `API error ${res.status}`
    );
  }
  return res.json() as Promise<T>;
}

// ---------------------------------------------------------------------------
// Accounts
// ---------------------------------------------------------------------------

export async function fetchAccounts() {
  return request<{ status: string; accounts: Account[] }>("/accounts");
}

export async function fetchAccountHierarchy(customerId: string) {
  return request<{ status: string; hierarchy: Account[] }>(
    `/accounts/${customerId}/hierarchy`
  );
}

// ---------------------------------------------------------------------------
// Campaigns
// ---------------------------------------------------------------------------

export async function fetchCampaigns(customerId: string, days = 30) {
  return request<{ status: string; campaigns: Campaign[] }>(
    `/accounts/${customerId}/campaigns?days=${days}`
  );
}

export async function campaignAction(
  customerId: string,
  campaignId: string,
  action: "pause" | "enable"
) {
  return request<StatusResponse>(
    `/campaigns/${customerId}/${campaignId}/action`,
    { method: "POST", body: JSON.stringify({ action }) }
  );
}

export async function updateBudget(
  customerId: string,
  campaignId: string,
  campaignBudgetId: string,
  newBudgetAmountMicros: number
) {
  return request<StatusResponse>(
    `/campaigns/${customerId}/${campaignId}/budget`,
    {
      method: "POST",
      body: JSON.stringify({
        campaign_budget_id: campaignBudgetId,
        new_budget_amount_micros: newBudgetAmountMicros,
      }),
    }
  );
}

// ---------------------------------------------------------------------------
// Ad Groups
// ---------------------------------------------------------------------------

export async function fetchAdGroups(
  customerId: string,
  campaignId?: string,
  days = 30
) {
  let url = `/accounts/${customerId}/ad-groups?days=${days}`;
  if (campaignId) url += `&campaign_id=${campaignId}`;
  return request<{ status: string; ad_groups: AdGroup[] }>(url);
}

export async function adGroupAction(
  customerId: string,
  adGroupId: string,
  action: "pause" | "enable"
) {
  return request<StatusResponse>(
    `/campaigns/${customerId}/ad-groups/${adGroupId}/action`,
    { method: "POST", body: JSON.stringify({ action }) }
  );
}

export async function updateCpcBid(
  customerId: string,
  adGroupId: string,
  cpcBidMicros: number
) {
  return request<StatusResponse>(
    `/campaigns/${customerId}/ad-groups/${adGroupId}/bid`,
    { method: "POST", body: JSON.stringify({ cpc_bid_micros: cpcBidMicros }) }
  );
}

// ---------------------------------------------------------------------------
// Keywords
// ---------------------------------------------------------------------------

export async function fetchKeywords(
  customerId: string,
  adGroupId?: string,
  days = 30
) {
  let url = `/accounts/${customerId}/keywords?days=${days}`;
  if (adGroupId) url += `&ad_group_id=${adGroupId}`;
  return request<{ status: string; keywords: Keyword[] }>(url);
}

export async function addKeyword(
  customerId: string,
  adGroupId: string,
  keywordText: string,
  matchType = "BROAD",
  cpcBidMicros?: number
) {
  return request<StatusResponse>(`/mutate/${customerId}/keywords/add`, {
    method: "POST",
    body: JSON.stringify({
      ad_group_id: adGroupId,
      keyword_text: keywordText,
      match_type: matchType,
      cpc_bid_micros: cpcBidMicros,
    }),
  });
}

export async function removeKeyword(
  customerId: string,
  adGroupId: string,
  criterionId: string
) {
  return request<StatusResponse>(`/mutate/${customerId}/keywords/remove`, {
    method: "POST",
    body: JSON.stringify({
      ad_group_id: adGroupId,
      criterion_id: criterionId,
    }),
  });
}

export async function addNegativeKeyword(
  customerId: string,
  campaignId: string,
  keywordText: string,
  matchType = "BROAD"
) {
  return request<StatusResponse>(
    `/mutate/${customerId}/keywords/negative`,
    {
      method: "POST",
      body: JSON.stringify({
        campaign_id: campaignId,
        keyword_text: keywordText,
        match_type: matchType,
      }),
    }
  );
}

// ---------------------------------------------------------------------------
// Ads
// ---------------------------------------------------------------------------

export async function fetchAds(
  customerId: string,
  adGroupId?: string,
  days = 30
) {
  let url = `/accounts/${customerId}/ads?days=${days}`;
  if (adGroupId) url += `&ad_group_id=${adGroupId}`;
  return request<{ status: string; ads: Ad[] }>(url);
}

export async function createAd(
  customerId: string,
  adGroupId: string,
  headlines: string[],
  descriptions: string[],
  finalUrls: string[],
  path1?: string,
  path2?: string
) {
  return request<StatusResponse>(`/mutate/${customerId}/ads/create`, {
    method: "POST",
    body: JSON.stringify({
      ad_group_id: adGroupId,
      headlines,
      descriptions,
      final_urls: finalUrls,
      path1,
      path2,
    }),
  });
}

export async function adAction(
  customerId: string,
  adGroupId: string,
  adId: string,
  action: "pause" | "enable"
) {
  return request<StatusResponse>(
    `/mutate/${customerId}/ads/${adGroupId}/${adId}/${action}`,
    { method: "POST" }
  );
}

// ---------------------------------------------------------------------------
// Metrics / Performance
// ---------------------------------------------------------------------------

export async function fetchMetrics(customerId: string, days = 30) {
  return request<PerformanceSummary>(
    `/accounts/${customerId}/metrics?days=${days}`
  );
}

// ---------------------------------------------------------------------------
// Recommendations
// ---------------------------------------------------------------------------

export async function generateRecommendations(
  customerId: string,
  days = 30
) {
  return request<{ status: string; recommendations: Recommendation[] }>(
    `/recommendations/${customerId}/generate`,
    { method: "POST", body: JSON.stringify({ days }) }
  );
}

export async function fetchRecommendations(customerId: string) {
  return request<{ status: string; recommendations: Recommendation[] }>(
    `/recommendations/${customerId}`
  );
}

export async function previewRecommendation(
  customerId: string,
  recommendationId: number
) {
  return request<Recommendation>(
    `/recommendations/${customerId}/${recommendationId}/preview`
  );
}

export async function applyRecommendation(
  customerId: string,
  recommendationId: number,
  confirmed = true
) {
  return request<StatusResponse>(
    `/recommendations/${customerId}/${recommendationId}/apply`,
    { method: "POST", body: JSON.stringify({ confirmed }) }
  );
}

export async function dismissRecommendation(
  customerId: string,
  recommendationId: number,
  reason?: string
) {
  return request<StatusResponse>(
    `/recommendations/${customerId}/${recommendationId}/dismiss`,
    { method: "POST", body: JSON.stringify({ reason }) }
  );
}

// ---------------------------------------------------------------------------
// Chat
// ---------------------------------------------------------------------------

export async function sendChatMessage(
  message: string,
  sessionId?: string,
  customerId?: string
): Promise<{ response: string; session_id: string }> {
  return request("/chat", {
    method: "POST",
    body: JSON.stringify({ message, session_id: sessionId, customer_id: customerId }),
  });
}

export async function fetchChatHistory(
  sessionId: string
): Promise<{ messages: ChatMessage[] }> {
  return request(`/chat/history?session_id=${sessionId}`);
}

// ---------------------------------------------------------------------------
// Mutation Logs
// ---------------------------------------------------------------------------

export async function fetchMutationLogs(
  customerId: string,
  limit = 50
): Promise<{ status: string; logs: MutationLog[] }> {
  return request(`/logs/${customerId}?limit=${limit}`);
}

// ---------------------------------------------------------------------------
// Account Profile
// ---------------------------------------------------------------------------

export async function fetchAccountProfile(customerId: string) {
  return request<AccountProfile>(`/profile/${customerId}`);
}

export async function saveAccountProfile(
  customerId: string,
  profile: Omit<AccountProfile, "id" | "customer_id" | "created_at" | "updated_at">
) {
  return request<AccountProfile>(`/profile/${customerId}`, {
    method: "PUT",
    body: JSON.stringify(profile),
  });
}

// ── Campaign Builder ────────────────────────────────────────────────

export interface StartBuildPayload {
  name: string;
  campaign_type: string;
  daily_budget_micros?: number;
  target_locations?: string[];
  target_languages?: string[];
  keyword_themes?: string[];
}

export async function startBuild(
  customerId: string,
  payload: StartBuildPayload
) {
  return request<{ status: string; blueprint: BlueprintDetail }>(
    `/builder/${customerId}/start`,
    { method: "POST", body: JSON.stringify(payload) }
  );
}

export async function fetchBlueprints(customerId: string) {
  return request<{ status: string; blueprints: BlueprintSummary[] }>(
    `/builder/${customerId}`
  );
}

export async function fetchBlueprint(
  customerId: string,
  blueprintId: number
) {
  return request<{ status: string; blueprint: BlueprintDetail }>(
    `/builder/${customerId}/${blueprintId}`
  );
}

export async function selectStrategy(
  customerId: string,
  blueprintId: number,
  strategyIndex: number
) {
  return request<{ status: string; blueprint: BlueprintDetail }>(
    `/builder/${customerId}/${blueprintId}/strategy`,
    { method: "PUT", body: JSON.stringify({ strategy_index: strategyIndex }) }
  );
}

export async function approveBlueprint(
  customerId: string,
  blueprintId: number
) {
  return request<{ status: string; blueprint: BlueprintDetail }>(
    `/builder/${customerId}/${blueprintId}/approve`,
    { method: "POST" }
  );
}

export async function advanceBlueprint(
  customerId: string,
  blueprintId: number
) {
  return request<{ status: string; blueprint: BlueprintDetail }>(
    `/builder/${customerId}/${blueprintId}/advance`,
    { method: "POST" }
  );
}

export async function deleteBlueprint(
  customerId: string,
  blueprintId: number
) {
  return request<StatusResponse>(
    `/builder/${customerId}/${blueprintId}`,
    { method: "DELETE" }
  );
}

export async function fetchAuditReport(
  customerId: string,
  blueprintId: number
) {
  return request<{ status: string; audit_report: AuditReportDetail }>(
    `/builder/${customerId}/${blueprintId}/audit`
  );
}

export async function parseCsv(
  customerId: string,
  files: File[]
): Promise<CsvParseResponse> {
  const formData = new FormData();
  for (const file of files) {
    formData.append("files", file);
  }
  const res = await fetch(
    `${API_BASE}/builder/${customerId}/csv-parse`,
    { method: "POST", body: formData }
  );
  if (!res.ok) {
    const err = await res.json().catch(() => ({}));
    throw new Error(
      (err as Record<string, unknown>).detail as string ?? `CSV parse failed (${res.status})`
    );
  }
  return res.json() as Promise<CsvParseResponse>;
}

/** Re-parse a single replacement file and merge into existing results. */
export async function parseSingleCsv(
  customerId: string,
  file: File
): Promise<CsvParseResponse> {
  return parseCsv(customerId, [file]);
}

export async function batchStartBuild(
  customerId: string,
  campaigns: BatchStartItem[]
): Promise<BatchStartResponse> {
  return request<BatchStartResponse>(
    `/builder/${customerId}/batch-start`,
    {
      method: "POST",
      body: JSON.stringify({ campaigns }),
    }
  );
}
