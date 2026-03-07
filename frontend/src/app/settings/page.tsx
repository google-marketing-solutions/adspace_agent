"use client";

import { useAccount } from "@/components/AccountContext";
import { useEffect, useState } from "react";
import { useQuery, useMutation, useQueryClient } from "@tanstack/react-query";
import Button from "@/components/data/LoadingButton";
import PageHeader from "@/components/layout/PageHeader";
import { useToast } from "@/hooks/use-toast";
import {
  Save,
  Building2,
  Target,
  ShieldCheck,
  Loader2,
  Link2,
  Sparkles,
  Server,
  Monitor,
  Brain,
  Database,
} from "lucide-react";
import { fetchAccountProfile, saveAccountProfile } from "@/lib/api";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Textarea } from "@/components/ui/textarea";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { Separator } from "@/components/ui/separator";

const INDUSTRY_VERTICALS = [
  "HEALTHCARE",
  "ECOMMERCE",
  "SAAS",
  "FINANCE",
  "LEGAL",
  "EDUCATION",
  "GENERAL",
];

export default function SettingsPage() {
  const { customerId, setCustomerId } = useAccount();
  const queryClient = useQueryClient();
  const { addToast } = useToast();
  const [inputId, setInputId] = useState(customerId);

  const [businessName, setBusinessName] = useState("");
  const [industryVertical, setIndustryVertical] = useState("GENERAL");
  const [subVertical, setSubVertical] = useState("");
  const [targetRoas, setTargetRoas] = useState("");
  const [targetCpa, setTargetCpa] = useState("");
  const [monthlyBudgetCap, setMonthlyBudgetCap] = useState("");
  const [brandGuidelines, setBrandGuidelines] = useState("");
  const [complianceNotes, setComplianceNotes] = useState("");
  const [restrictedTerms, setRestrictedTerms] = useState("");

  const { data: profile, isLoading: profileLoading } = useQuery({
    queryKey: ["profile", customerId],
    queryFn: () => fetchAccountProfile(customerId),
    enabled: !!customerId,
    retry: false,
  });

  useEffect(() => {
    if (profile) {
      setBusinessName(profile.business_name || "");
      setIndustryVertical(profile.industry_vertical || "GENERAL");
      setSubVertical(profile.sub_vertical || "");
      setTargetRoas(profile.target_roas?.toString() || "");
      setTargetCpa(profile.target_cpa?.toString() || "");
      setMonthlyBudgetCap(profile.monthly_budget_cap?.toString() || "");
      setBrandGuidelines(profile.brand_guidelines || "");
      setComplianceNotes(profile.compliance_notes || "");
      setRestrictedTerms(
        profile.restricted_terms ? profile.restricted_terms.join(", ") : ""
      );
    }
  }, [profile]);

  const profileMutation = useMutation({
    mutationFn: () =>
      saveAccountProfile(customerId, {
        business_name: businessName,
        industry_vertical: industryVertical,
        sub_vertical: subVertical,
        target_roas: targetRoas ? parseFloat(targetRoas) : null,
        target_cpa: targetCpa ? parseFloat(targetCpa) : null,
        monthly_budget_cap: monthlyBudgetCap ? parseFloat(monthlyBudgetCap) : null,
        brand_guidelines: brandGuidelines,
        compliance_notes: complianceNotes,
        restricted_terms: restrictedTerms
          ? restrictedTerms.split(",").map((t) => t.trim()).filter(Boolean)
          : [],
      }),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ["profile", customerId] });
      addToast("success", "Profile saved", "Your account profile has been updated successfully.");
    },
    onError: (err: Error) => {
      addToast("error", "Save failed", err.message);
    },
  });

  const handleSave = () => {
    setCustomerId(inputId);
    addToast("success", "Settings saved", `Active account set to ${inputId}.`);
  };

  return (
    <div className="space-y-6 max-w-2xl">
      <PageHeader
        title="Settings"
        subtitle="Configure your AdSpace Agent dashboard and account profile"
      />

      {/* Connection Settings */}
      <Card className="animate-fade-in">
        <CardHeader className="flex flex-row items-center gap-3">
          <div className="p-2 bg-primary/10 rounded-xl">
            <Link2 className="w-5 h-5 text-primary" />
          </div>
          <CardTitle>Connection Settings</CardTitle>
        </CardHeader>
        <CardContent className="space-y-4">
          <div>
            <Label htmlFor="customer-id">Google Ads Customer ID</Label>
            <p className="text-xs text-muted-foreground mb-2">
              The customer ID to query (without dashes). For sub-accounts, use the child account ID.
            </p>
            <Input
              id="customer-id"
              type="text"
              value={inputId}
              onChange={(e) => setInputId(e.target.value.replace(/\D/g, ""))}
              placeholder="e.g. 7616751962"
            />
          </div>
          <div className="flex justify-end pt-2">
            <Button onClick={handleSave} icon={<Save className="w-4 h-4" />}>
              Save Settings
            </Button>
          </div>
        </CardContent>
      </Card>

      {/* Account Profile */}
      <Card className="animate-fade-in">
        <CardHeader className="flex flex-row items-center gap-3">
          <div className="p-2 bg-violet-50 dark:bg-violet-950 rounded-xl">
            <Building2 className="w-5 h-5 text-violet-600 dark:text-violet-400" />
          </div>
          <div>
            <CardTitle>Account Profile</CardTitle>
            <p className="text-xs text-muted-foreground">
              Business context used by AI for recommendations and ad copy
            </p>
          </div>
        </CardHeader>

        {profileLoading ? (
          <CardContent className="flex flex-col items-center justify-center gap-3 py-12 text-muted-foreground">
            <Loader2 className="w-6 h-6 animate-spin text-primary" />
            <span className="text-sm">Loading profile…</span>
          </CardContent>
        ) : (
          <CardContent className="space-y-0 p-0">
            {/* Business Info */}
            <div className="p-6 space-y-4">
              <div>
                <Label htmlFor="business-name">Business Name</Label>
                <Input
                  id="business-name"
                  type="text"
                  value={businessName}
                  onChange={(e) => setBusinessName(e.target.value)}
                  placeholder="e.g. PlutoCare"
                />
              </div>
              <div className="grid grid-cols-2 gap-4">
                <div>
                  <Label htmlFor="industry">Industry Vertical</Label>
                  <Select value={industryVertical} onValueChange={setIndustryVertical}>
                    <SelectTrigger id="industry">
                      <SelectValue placeholder="Select industry" />
                    </SelectTrigger>
                    <SelectContent>
                      {INDUSTRY_VERTICALS.map((v) => (
                        <SelectItem key={v} value={v}>{v}</SelectItem>
                      ))}
                    </SelectContent>
                  </Select>
                </div>
                <div>
                  <Label htmlFor="sub-vertical">Sub-Vertical</Label>
                  <Input
                    id="sub-vertical"
                    type="text"
                    value={subVertical}
                    onChange={(e) => setSubVertical(e.target.value)}
                    placeholder="e.g. Telehealth, Ketamine Therapy"
                  />
                </div>
              </div>
            </div>

            <Separator />

            {/* Performance Targets */}
            <div className="p-6 space-y-4">
              <h3 className="text-sm font-semibold text-foreground flex items-center gap-2">
                <div className="p-1.5 bg-amber-50 dark:bg-amber-950 rounded-lg">
                  <Target className="w-3.5 h-3.5 text-amber-600 dark:text-amber-400" />
                </div>
                Performance Targets
              </h3>
              <div className="grid grid-cols-3 gap-4">
                <div>
                  <Label htmlFor="target-roas">Target ROAS</Label>
                  <Input
                    id="target-roas"
                    type="number"
                    step="0.1"
                    value={targetRoas}
                    onChange={(e) => setTargetRoas(e.target.value)}
                    placeholder="e.g. 3.0"
                  />
                </div>
                <div>
                  <Label htmlFor="target-cpa">Target CPA ($)</Label>
                  <Input
                    id="target-cpa"
                    type="number"
                    step="1"
                    value={targetCpa}
                    onChange={(e) => setTargetCpa(e.target.value)}
                    placeholder="e.g. 50"
                  />
                </div>
                <div>
                  <Label htmlFor="monthly-budget">Monthly Budget ($)</Label>
                  <Input
                    id="monthly-budget"
                    type="number"
                    step="100"
                    value={monthlyBudgetCap}
                    onChange={(e) => setMonthlyBudgetCap(e.target.value)}
                    placeholder="e.g. 5000"
                  />
                </div>
              </div>
            </div>

            <Separator />

            {/* Compliance & Brand */}
            <div className="p-6 space-y-4">
              <h3 className="text-sm font-semibold text-foreground flex items-center gap-2">
                <div className="p-1.5 bg-emerald-50 dark:bg-emerald-950 rounded-lg">
                  <ShieldCheck className="w-3.5 h-3.5 text-emerald-600 dark:text-emerald-400" />
                </div>
                Compliance &amp; Brand
              </h3>
              <div>
                <Label htmlFor="brand-guidelines">Brand Guidelines</Label>
                <Textarea
                  id="brand-guidelines"
                  value={brandGuidelines}
                  onChange={(e) => setBrandGuidelines(e.target.value)}
                  rows={3}
                  placeholder="Tone of voice, messaging pillars, dos and don'ts..."
                />
              </div>
              <div>
                <Label htmlFor="compliance-notes">Compliance Notes</Label>
                <Textarea
                  id="compliance-notes"
                  value={complianceNotes}
                  onChange={(e) => setComplianceNotes(e.target.value)}
                  rows={3}
                  placeholder="e.g. HIPAA-compliant telehealth; no controlled substance claims in ads..."
                />
              </div>
              <div>
                <Label htmlFor="restricted-terms">Restricted Terms</Label>
                <p className="text-xs text-muted-foreground mb-1.5">
                  Comma-separated list of terms that should never appear in ads or keywords.
                </p>
                <Input
                  id="restricted-terms"
                  type="text"
                  value={restrictedTerms}
                  onChange={(e) => setRestrictedTerms(e.target.value)}
                  placeholder="e.g. cure, FDA approved, schedule II"
                />
              </div>
            </div>

            <Separator />

            {/* Save Profile */}
            <div className="p-6 flex items-center justify-end bg-muted/30">
              <Button
                variant="primary"
                onClick={() => profileMutation.mutate()}
                icon={<Save className="w-4 h-4" />}
                loading={profileMutation.isPending}
              >
                Save Profile
              </Button>
            </div>
          </CardContent>
        )}
      </Card>

      {/* About */}
      <Card className="bg-gradient-to-br from-indigo-50 via-violet-50 to-purple-50 dark:from-indigo-950 dark:via-violet-950 dark:to-purple-950 border-primary/20 animate-fade-in">
        <CardContent className="p-6">
          <div className="flex items-center gap-2.5 mb-3">
            <div className="p-1.5 bg-card/80 rounded-lg shadow-sm">
              <Sparkles className="w-4 h-4 text-primary" />
            </div>
            <h3 className="text-sm font-semibold text-foreground">About AdSpace Agent</h3>
          </div>
          <p className="text-sm text-muted-foreground leading-relaxed">
            AdSpace Agent is an AI-powered Google Ads management platform built
            with Google ADK, FastAPI, and Next.js. It provides campaign analytics,
            AI recommendations, and one-click optimizations.
          </p>
          <div className="mt-4 grid grid-cols-2 gap-3">
            {[
              { icon: Server, label: "Backend", value: "FastAPI + Google ADK" },
              { icon: Monitor, label: "Frontend", value: "Next.js + React" },
              { icon: Brain, label: "AI", value: "Gemini 3 Flash" },
              { icon: Database, label: "Database", value: "SQLite + SQLAlchemy" },
            ].map(({ icon: Icon, label, value }) => (
              <div key={label} className="flex items-center gap-2 text-xs text-muted-foreground">
                <Icon className="w-3.5 h-3.5" />
                <span className="font-medium text-foreground">{label}:</span>{" "}
                {value}
              </div>
            ))}
          </div>
        </CardContent>
      </Card>
    </div>
  );
}
