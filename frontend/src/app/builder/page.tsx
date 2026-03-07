"use client";

import { useQuery, useMutation, useQueryClient } from "@tanstack/react-query";
import {
  Rocket,
  Play,
  Trash2,
  CheckCircle,
  RefreshCw,
  ChevronRight,
  ArrowLeft,
  ClipboardList,
  Target,
  Palette,
  Wrench,
  Search,
  Plus,
  ShieldCheck,
  Upload,
  FileSpreadsheet,
  Download,
  AlertTriangle,
  CircleCheck,
  CircleX,
  Pencil,
  Save,
  RotateCcw,
  XCircle,
  Replace,
} from "lucide-react";
import { useAccount } from "@/components/AccountContext";
import StatusBadge from "@/components/data/StatusBadge";
import Button from "@/components/data/LoadingButton";
import LoadingSpinner from "@/components/feedback/LoadingSpinner";
import ErrorBanner from "@/components/feedback/ErrorBanner";
import { useToast } from "@/hooks/use-toast";
import {
  fetchBlueprints,
  fetchBlueprint,
  startBuild,
  selectStrategy,
  approveBlueprint,
  advanceBlueprint,
  deleteBlueprint,
  fetchAuditReport,
  parseCsv,
  parseSingleCsv,
  batchStartBuild,
} from "@/lib/api";
import type {
  BlueprintSummary,
  BlueprintDetail,
  BlueprintStatus,
  BuilderPhase,
  CsvRowPreview,
  CsvFileStatus,
} from "@/lib/types";
import { useState, useRef, useCallback } from "react";

// ── Constants ──────────────────────────────────────────────

const STATUS_COLORS: Record<BlueprintStatus, string> = {
  DRAFT: "bg-gray-100 text-gray-700",
  AUDIT_COMPLETE: "bg-blue-100 text-blue-700",
  STRATEGY_READY: "bg-indigo-100 text-indigo-700",
  CREATIVE_READY: "bg-purple-100 text-purple-700",
  BUILDING: "bg-yellow-100 text-yellow-700",
  BUILT: "bg-teal-100 text-teal-700",
  REVIEW_PASSED: "bg-green-100 text-green-700",
  DEPLOYED: "bg-emerald-100 text-emerald-800",
  FAILED: "bg-red-100 text-red-700",
  EXPIRED: "bg-gray-100 text-gray-500",
};

const PHASE_ICONS: Record<BuilderPhase, React.ReactNode> = {
  DATA_AUDIT: <ClipboardList className="w-4 h-4" />,
  DEEP_ANALYSIS: <Search className="w-4 h-4" />,
  STRATEGY_GENERATION: <Target className="w-4 h-4" />,
  CREATIVE_GENERATION: <Palette className="w-4 h-4" />,
  BUILD_DEPLOY: <Wrench className="w-4 h-4" />,
  REVIEW_OPTIMIZE: <CheckCircle className="w-4 h-4" />,
};

const PHASE_LABELS: Record<BuilderPhase, string> = {
  DATA_AUDIT: "Data Audit",
  DEEP_ANALYSIS: "Deep Analysis",
  STRATEGY_GENERATION: "Strategy Generation",
  CREATIVE_GENERATION: "Creative Generation",
  BUILD_DEPLOY: "Build & Deploy",
  REVIEW_OPTIMIZE: "Review & Optimize",
};

const PHASE_ORDER: BuilderPhase[] = [
  "DATA_AUDIT",
  "DEEP_ANALYSIS",
  "STRATEGY_GENERATION",
  "CREATIVE_GENERATION",
  "BUILD_DEPLOY",
  "REVIEW_OPTIMIZE",
];

const CAMPAIGN_TYPES = [
  { value: "SEARCH", label: "Search" },
  { value: "DISPLAY", label: "Display" },
  { value: "SHOPPING", label: "Shopping" },
  { value: "VIDEO", label: "Video" },
  { value: "PERFORMANCE_MAX", label: "Performance Max" },
];

const CSV_TEMPLATE_HEADER =
  "name,campaign_type,daily_budget,locations,languages,keywords";
const CSV_TEMPLATE_ROWS = [
  "Spring Sale Search,SEARCH,50.00,US|CA,en,spring sale|discount shoes|running shoes",
  "Brand Awareness Display,DISPLAY,30.00,US,en|es,brand awareness|display ads",
  "Summer PMax Campaign,PERFORMANCE_MAX,100.00,US|UK|AU,en,summer collection|new arrivals",
];
const CSV_TEMPLATE_CONTENT = [CSV_TEMPLATE_HEADER, ...CSV_TEMPLATE_ROWS].join(
  "\n"
);

// ── Component ──────────────────────────────────────────────

export default function BuilderPage() {
  const { customerId } = useAccount();
  const queryClient = useQueryClient();
  const { addToast } = useToast();

  // View state
  const [selectedId, setSelectedId] = useState<number | null>(null);
  const [showNewForm, setShowNewForm] = useState(false);
  const [showCsvImport, setShowCsvImport] = useState(false);
  const [confirmDelete, setConfirmDelete] = useState<number | null>(null);
  const [confirmApprove, setConfirmApprove] = useState<number | null>(null);

  // New build form state
  const [formName, setFormName] = useState("");
  const [formType, setFormType] = useState("SEARCH");
  const [formBudget, setFormBudget] = useState("");
  const [formLocations, setFormLocations] = useState("");
  const [formLanguages, setFormLanguages] = useState("en");
  const [formKeywords, setFormKeywords] = useState("");

  // CSV import state
  const fileInputRef = useRef<HTMLInputElement>(null);
  const replaceInputRef = useRef<HTMLInputElement>(null);
  const [csvFiles, setCsvFiles] = useState<File[]>([]);
  const [csvRows, setCsvRows] = useState<CsvRowPreview[]>([]);
  const [csvValidCount, setCsvValidCount] = useState(0);
  const [csvErrorCount, setCsvErrorCount] = useState(0);
  const [csvParseError, setCsvParseError] = useState<string | null>(null);
  const [csvParsing, setCsvParsing] = useState(false);
  const [csvCreating, setCsvCreating] = useState(false);
  const [dragOver, setDragOver] = useState(false);
  const [csvFileStatuses, setCsvFileStatuses] = useState<CsvFileStatus[]>([]);
  const [editingRowIdx, setEditingRowIdx] = useState<number | null>(null);
  const [editBuffer, setEditBuffer] = useState<Partial<CsvRowPreview> | null>(null);
  const [replacingFile, setReplacingFile] = useState<string | null>(null);

  // ── Queries ─────────────────────────────────────────────

  const blueprintsQ = useQuery({
    queryKey: ["blueprints", customerId],
    queryFn: () => fetchBlueprints(customerId),
    enabled: !!customerId,
    refetchInterval: 10_000,
  });

  const detailQ = useQuery({
    queryKey: ["blueprint", customerId, selectedId],
    queryFn: () => fetchBlueprint(customerId, selectedId!),
    enabled: !!customerId && selectedId !== null,
    refetchInterval: 5_000,
  });

  const auditQ = useQuery({
    queryKey: ["audit", customerId, selectedId],
    queryFn: () => fetchAuditReport(customerId, selectedId!),
    enabled: !!customerId && selectedId !== null,
  });

  // ── Mutations ───────────────────────────────────────────

  const startMut = useMutation({
    mutationFn: () =>
      startBuild(customerId, {
        name: formName.trim(),
        campaign_type: formType,
        daily_budget_micros: formBudget
          ? Math.round(parseFloat(formBudget) * 1_000_000)
          : undefined,
        target_locations: formLocations
          ? formLocations.split(",").map((s) => s.trim())
          : undefined,
        target_languages: formLanguages
          ? formLanguages.split(",").map((s) => s.trim())
          : undefined,
        keyword_themes: formKeywords
          ? formKeywords.split(",").map((s) => s.trim())
          : undefined,
      }),
    onSuccess: (data) => {
      addToast("success", "Build started!", "The AI pipeline is running.");
      queryClient.invalidateQueries({ queryKey: ["blueprints"] });
      setSelectedId(data.blueprint.id);
      setShowNewForm(false);
      resetForm();
    },
    onError: () => addToast("error", "Failed to start build."),
  });

  const strategyMut = useMutation({
    mutationFn: ({ bpId, idx }: { bpId: number; idx: number }) =>
      selectStrategy(customerId, bpId, idx),
    onSuccess: () => {
      addToast("success", "Strategy selected.");
      queryClient.invalidateQueries({ queryKey: ["blueprint"] });
    },
    onError: () => addToast("error", "Failed to select strategy."),
  });

  const approveMut = useMutation({
    mutationFn: (bpId: number) => approveBlueprint(customerId, bpId),
    onSuccess: () => {
      addToast("success", "Blueprint approved & deployed!");
      queryClient.invalidateQueries({ queryKey: ["blueprints"] });
      queryClient.invalidateQueries({ queryKey: ["blueprint"] });
      setConfirmApprove(null);
    },
    onError: () => {
      addToast("error", "Failed to approve blueprint.");
      setConfirmApprove(null);
    },
  });

  const deleteMut = useMutation({
    mutationFn: (bpId: number) => deleteBlueprint(customerId, bpId),
    onSuccess: () => {
      addToast("info", "Blueprint deleted.");
      queryClient.invalidateQueries({ queryKey: ["blueprints"] });
      if (selectedId === confirmDelete) setSelectedId(null);
      setConfirmDelete(null);
    },
    onError: () => {
      addToast("error", "Failed to delete blueprint.");
      setConfirmDelete(null);
    },
  });

  const [confirmAdvance, setConfirmAdvance] = useState<number | null>(null);

  const advanceMut = useMutation({
    mutationFn: (bpId: number) => advanceBlueprint(customerId, bpId),
    onSuccess: () => {
      addToast("success", "Phase advanced successfully.");
      queryClient.invalidateQueries({ queryKey: ["blueprints"] });
      queryClient.invalidateQueries({ queryKey: ["blueprint"] });
      setConfirmAdvance(null);
    },
    onError: () => {
      addToast("error", "Failed to advance phase.");
      setConfirmAdvance(null);
    },
  });

  function resetForm() {
    setFormName("");
    setFormType("SEARCH");
    setFormBudget("");
    setFormLocations("");
    setFormLanguages("en");
    setFormKeywords("");
  }

  function resetCsvState() {
    setCsvFiles([]);
    setCsvRows([]);
    setCsvValidCount(0);
    setCsvErrorCount(0);
    setCsvParseError(null);
    setCsvParsing(false);
    setCsvCreating(false);
    setDragOver(false);
    setCsvFileStatuses([]);
    setEditingRowIdx(null);
    setEditBuffer(null);
    setReplacingFile(null);
    if (fileInputRef.current) fileInputRef.current.value = "";
    if (replaceInputRef.current) replaceInputRef.current.value = "";
  }

  const handleCsvFile = useCallback(
    async (files: File[]) => {
      setCsvFiles(files);
      setCsvParseError(null);
      setCsvParsing(true);
      try {
        const result = await parseCsv(customerId, files);
        if (result.error) {
          setCsvParseError(result.error);
          setCsvRows([]);
          setCsvValidCount(0);
          setCsvErrorCount(0);
        } else {
          setCsvRows(result.rows);
          setCsvValidCount(result.valid_count);
          setCsvErrorCount(result.error_count);
        }
        setCsvFileStatuses(result.file_statuses ?? []);
      } catch (err) {
        setCsvParseError(
          err instanceof Error ? err.message : "Failed to parse CSV"
        );
      } finally {
        setCsvParsing(false);
      }
    },
    [customerId]
  );

  const handleDrop = useCallback(
    (e: React.DragEvent) => {
      e.preventDefault();
      setDragOver(false);
      const droppedFiles = Array.from(e.dataTransfer.files);
      const csvs = droppedFiles.filter((f) =>
        f.name.toLowerCase().endsWith(".csv")
      );
      if (csvs.length === 0) {
        setCsvParseError("Please drop one or more .csv files.");
      } else {
        if (csvs.length < droppedFiles.length) {
          // Some non-csv files were dropped alongside csvs — still parse the csvs
        }
        handleCsvFile(csvs);
      }
    },
    [handleCsvFile]
  );

  const handleFileSelect = useCallback(
    (e: React.ChangeEvent<HTMLInputElement>) => {
      const selected = e.target.files;
      if (selected && selected.length > 0) {
        handleCsvFile(Array.from(selected));
      }
    },
    [handleCsvFile]
  );

  async function handleBatchCreate() {
    setCsvCreating(true);
    try {
      const validRows = csvRows.filter((r) => r.valid);
      const campaigns = validRows.map((r) => ({
        name: r.name,
        campaign_type: r.campaign_type,
        daily_budget_micros: r.daily_budget
          ? Math.round(r.daily_budget * 1_000_000)
          : undefined,
        target_locations: r.locations.length ? r.locations : undefined,
        target_languages: r.languages.length ? r.languages : undefined,
        keyword_themes: r.keywords.length ? r.keywords : undefined,
      }));
      const result = await batchStartBuild(customerId, campaigns);
      addToast(
        "success",
        `Created ${result.created} campaign blueprint${result.created !== 1 ? "s" : ""}!`,
        "The AI pipeline will process each one."
      );
      queryClient.invalidateQueries({ queryKey: ["blueprints"] });
      setShowCsvImport(false);
      resetCsvState();
    } catch (err) {
      addToast(
        "error",
        "Batch create failed.",
        err instanceof Error ? err.message : undefined
      );
    } finally {
      setCsvCreating(false);
    }
  }

  function downloadCsvTemplate() {
    const blob = new Blob([CSV_TEMPLATE_CONTENT], { type: "text/csv" });
    const url = URL.createObjectURL(blob);
    const a = document.createElement("a");
    a.href = url;
    a.download = "campaign_template.csv";
    a.click();
    URL.revokeObjectURL(url);
  }

  // ── CSV file management helpers ─────────────────────────

  const VALID_TYPES = new Set(CAMPAIGN_TYPES.map((t) => t.value));

  function recomputeCsvCounts(rows: CsvRowPreview[]) {
    setCsvValidCount(rows.filter((r) => r.valid).length);
    setCsvErrorCount(rows.filter((r) => !r.valid).length);
  }

  function removeFileRows(filename: string) {
    const remaining = csvRows.filter((r) => r.source_file !== filename);
    // Re-number rows sequentially
    const renumbered = remaining.map((r, i) => ({ ...r, row_number: i + 1 }));
    setCsvRows(renumbered);
    recomputeCsvCounts(renumbered);
    setCsvFileStatuses((prev) => prev.filter((f) => f.filename !== filename));
    setCsvFiles((prev) => prev.filter((f) => f.name !== filename));
  }

  async function handleReplaceFile(oldFilename: string, newFile: File) {
    setCsvParsing(true);
    setReplacingFile(null);
    try {
      const result = await parseSingleCsv(customerId, newFile);
      const newStatuses = result.file_statuses ?? [];
      const newStatus = newStatuses[0];

      // Replace old file status with new
      setCsvFileStatuses((prev) => [
        ...prev.filter((f) => f.filename !== oldFilename),
        ...(newStatus ? [newStatus] : []),
      ]);

      if (newStatus?.status === "ok" && result.rows.length > 0) {
        // Remove old file rows, append new file rows, re-number
        const kept = csvRows.filter((r) => r.source_file !== oldFilename);
        const merged = [...kept, ...result.rows];
        const renumbered = merged.map((r, i) => ({ ...r, row_number: i + 1 }));
        setCsvRows(renumbered);
        recomputeCsvCounts(renumbered);
        setCsvFiles((prev) => [
          ...prev.filter((f) => f.name !== oldFilename),
          newFile,
        ]);
        addToast("success", `Replaced "${oldFilename}" with "${newFile.name}".`);
      } else {
        // New file also had errors — keep old rows removed if any
        const kept = csvRows.filter((r) => r.source_file !== oldFilename);
        const renumbered = kept.map((r, i) => ({ ...r, row_number: i + 1 }));
        setCsvRows(renumbered);
        recomputeCsvCounts(renumbered);
        setCsvFiles((prev) => [
          ...prev.filter((f) => f.name !== oldFilename),
          newFile,
        ]);
        addToast("error", `Replacement file "${newFile.name}" has errors.`);
      }
    } catch (err) {
      addToast(
        "error",
        "Replace failed",
        err instanceof Error ? err.message : undefined
      );
    } finally {
      setCsvParsing(false);
    }
  }

  function handleReplaceFileSelect(e: React.ChangeEvent<HTMLInputElement>) {
    const file = e.target.files?.[0];
    if (file && replacingFile) {
      handleReplaceFile(replacingFile, file);
    }
    if (replaceInputRef.current) replaceInputRef.current.value = "";
  }

  function validateRow(row: CsvRowPreview): CsvRowPreview {
    const errors: string[] = [];
    if (!row.name.trim()) errors.push("Name is required");
    if (!VALID_TYPES.has(row.campaign_type))
      errors.push(`Invalid campaign type: ${row.campaign_type}`);
    if (row.daily_budget != null && row.daily_budget <= 0)
      errors.push("Budget must be positive");
    return { ...row, errors, valid: errors.length === 0 };
  }

  function startEditRow(idx: number) {
    setEditingRowIdx(idx);
    setEditBuffer({ ...csvRows[idx] });
  }

  function cancelEditRow() {
    setEditingRowIdx(null);
    setEditBuffer(null);
  }

  function saveEditRow() {
    if (editingRowIdx === null || !editBuffer) return;
    const updated = validateRow({ ...csvRows[editingRowIdx], ...editBuffer } as CsvRowPreview);
    const newRows = [...csvRows];
    newRows[editingRowIdx] = updated;
    setCsvRows(newRows);
    recomputeCsvCounts(newRows);
    setEditingRowIdx(null);
    setEditBuffer(null);
  }

  // ── Derived ─────────────────────────────────────────────

  const blueprints: BlueprintSummary[] =
    blueprintsQ.data?.blueprints ?? [];
  const detail: BlueprintDetail | null = detailQ.data?.blueprint ?? null;
  const isRunning =
    detail &&
    !["DEPLOYED", "FAILED", "EXPIRED", "REVIEW_PASSED"].includes(
      detail.status
    );

  // ── Render ──────────────────────────────────────────────

  if (!customerId) {
    return (
      <div className="p-6 text-gray-500">
        Select an account to use the Campaign Builder.
      </div>
    );
  }

  return (
    <div className="space-y-6">


      {/* Header */}
      <div className="flex items-center justify-between">
        <div className="flex items-center gap-3">
          {selectedId !== null && (
            <button
              onClick={() => setSelectedId(null)}
              className="p-1.5 rounded-lg hover:bg-gray-200 transition"
            >
              <ArrowLeft className="w-5 h-5" />
            </button>
          )}
          <Rocket className="w-6 h-6 text-blue-600" />
          <h1 className="text-2xl font-bold text-gray-900">
            {selectedId !== null ? "Blueprint Detail" : "Campaign Builder"}
          </h1>
        </div>
        <div className="flex items-center gap-2">
          <Button
            onClick={() => blueprintsQ.refetch()}
            variant="secondary"
            size="sm"
          >
            <RefreshCw
              className={`w-4 h-4 mr-1 ${
                blueprintsQ.isFetching ? "animate-spin" : ""
              }`}
            />
            Refresh
          </Button>
          {selectedId === null && (
            <>
              <Button
                onClick={() => {
                  setShowCsvImport(true);
                  setShowNewForm(false);
                  resetCsvState();
                }}
                variant="secondary"
                size="sm"
              >
                <Upload className="w-4 h-4 mr-1" />
                Import CSV
              </Button>
              <Button
                onClick={() => {
                  setShowNewForm(true);
                  setShowCsvImport(false);
                }}
                size="sm"
              >
                <Plus className="w-4 h-4 mr-1" />
                New Build
              </Button>
            </>
          )}
        </div>
      </div>

      {/* Error */}
      {blueprintsQ.isError && (
        <ErrorBanner message="Failed to load blueprints." />
      )}

      {/* Loading */}
      {blueprintsQ.isLoading && <LoadingSpinner />}

      {/* ── NEW BUILD FORM ─────────────────────────────── */}
      {showNewForm && selectedId === null && (
        <div className="bg-white rounded-xl shadow-sm border border-gray-200 p-6 space-y-4">
          <h2 className="text-lg font-semibold text-gray-900">
            Start a New Campaign Build
          </h2>
          <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
            <div>
              <label className="block text-sm font-medium text-gray-700 mb-1">
                Campaign Name *
              </label>
              <input
                value={formName}
                onChange={(e) => setFormName(e.target.value)}
                placeholder="e.g. Spring Sale 2026"
                className="w-full border border-gray-300 rounded-lg px-3 py-2 text-sm focus:ring-2 focus:ring-blue-500 focus:border-blue-500"
              />
            </div>
            <div>
              <label className="block text-sm font-medium text-gray-700 mb-1">
                Campaign Type *
              </label>
              <select
                value={formType}
                onChange={(e) => setFormType(e.target.value)}
                className="w-full border border-gray-300 rounded-lg px-3 py-2 text-sm focus:ring-2 focus:ring-blue-500 focus:border-blue-500"
              >
                {CAMPAIGN_TYPES.map((t) => (
                  <option key={t.value} value={t.value}>
                    {t.label}
                  </option>
                ))}
              </select>
            </div>
            <div>
              <label className="block text-sm font-medium text-gray-700 mb-1">
                Daily Budget (USD)
              </label>
              <input
                value={formBudget}
                onChange={(e) => setFormBudget(e.target.value)}
                placeholder="e.g. 50.00"
                type="number"
                step="0.01"
                min="0"
                className="w-full border border-gray-300 rounded-lg px-3 py-2 text-sm focus:ring-2 focus:ring-blue-500 focus:border-blue-500"
              />
            </div>
            <div>
              <label className="block text-sm font-medium text-gray-700 mb-1">
                Target Locations
              </label>
              <input
                value={formLocations}
                onChange={(e) => setFormLocations(e.target.value)}
                placeholder="e.g. US, CA, UK"
                className="w-full border border-gray-300 rounded-lg px-3 py-2 text-sm focus:ring-2 focus:ring-blue-500 focus:border-blue-500"
              />
            </div>
            <div>
              <label className="block text-sm font-medium text-gray-700 mb-1">
                Languages
              </label>
              <input
                value={formLanguages}
                onChange={(e) => setFormLanguages(e.target.value)}
                placeholder="e.g. en, es"
                className="w-full border border-gray-300 rounded-lg px-3 py-2 text-sm focus:ring-2 focus:ring-blue-500 focus:border-blue-500"
              />
            </div>
            <div>
              <label className="block text-sm font-medium text-gray-700 mb-1">
                Keyword Themes
              </label>
              <input
                value={formKeywords}
                onChange={(e) => setFormKeywords(e.target.value)}
                placeholder="e.g. telehealth, virtual care"
                className="w-full border border-gray-300 rounded-lg px-3 py-2 text-sm focus:ring-2 focus:ring-blue-500 focus:border-blue-500"
              />
            </div>
          </div>
          <div className="flex gap-2 pt-2">
            <Button
              onClick={() => startMut.mutate()}
              disabled={!formName.trim() || startMut.isPending}
            >
              {startMut.isPending ? (
                <>
                  <RefreshCw className="w-4 h-4 mr-1 animate-spin" />
                  Starting...
                </>
              ) : (
                <>
                  <Play className="w-4 h-4 mr-1" />
                  Start Build
                </>
              )}
            </Button>
            <Button
              variant="secondary"
              onClick={() => {
                setShowNewForm(false);
                resetForm();
              }}
            >
              Cancel
            </Button>
          </div>
        </div>
      )}

      {/* ── CSV IMPORT SECTION ─────────────────────────── */}
      {showCsvImport && selectedId === null && (
        <div className="bg-white rounded-xl shadow-sm border border-gray-200 p-6 space-y-5">
          <div className="flex items-center justify-between">
            <div className="flex items-center gap-2">
              <FileSpreadsheet className="w-5 h-5 text-green-600" />
              <h2 className="text-lg font-semibold text-gray-900">
                Import Campaigns from CSV
              </h2>
            </div>
            <button
              onClick={downloadCsvTemplate}
              className="flex items-center gap-1 text-sm text-blue-600 hover:text-blue-800 transition"
            >
              <Download className="w-4 h-4" />
              Download Template
            </button>
          </div>

          <p className="text-sm text-gray-500">
            Upload one or more CSV files with columns:{" "}
            <code className="px-1 py-0.5 bg-gray-100 rounded text-xs">
              name, campaign_type, daily_budget, locations, languages, keywords
            </code>
            . Use pipes (<code className="px-1 py-0.5 bg-gray-100 rounded text-xs">|</code>) to
            separate multiple values in a column. Rows from all files are merged together.
          </p>

          {/* Drop zone */}
          <div
            onDrop={handleDrop}
            onDragOver={(e) => {
              e.preventDefault();
              setDragOver(true);
            }}
            onDragLeave={() => setDragOver(false)}
            onClick={() => fileInputRef.current?.click()}
            className={`relative flex flex-col items-center justify-center gap-2 border-2 border-dashed rounded-xl p-8 cursor-pointer transition ${
              dragOver
                ? "border-blue-500 bg-blue-50"
                : csvFiles.length > 0
                  ? "border-green-300 bg-green-50"
                  : "border-gray-300 hover:border-gray-400 bg-gray-50"
            }`}
          >
            <input
              ref={fileInputRef}
              type="file"
              accept=".csv"
              multiple
              className="hidden"
              onChange={handleFileSelect}
            />
            {csvParsing ? (
              <RefreshCw className="w-8 h-8 text-blue-500 animate-spin" />
            ) : csvFiles.length > 0 ? (
              <CircleCheck className="w-8 h-8 text-green-500" />
            ) : (
              <Upload className="w-8 h-8 text-gray-400" />
            )}
            <span className="text-sm font-medium text-gray-700">
              {csvParsing
                ? "Parsing…"
                : csvFiles.length > 0
                  ? csvFiles.length === 1
                    ? csvFiles[0].name
                    : `${csvFiles.length} files: ${csvFiles.map((f) => f.name).join(", ")}`
                  : "Drag & drop CSV files, or click to browse"}
            </span>
            {csvFiles.length > 0 && !csvParsing && (
              <span className="text-xs text-gray-500">
                {csvValidCount} valid · {csvErrorCount} with errors ·{" "}
                {csvRows.length} total rows
                {csvFiles.length > 1 && ` from ${csvFiles.length} files`}
              </span>
            )}
          </div>

          {/* Global parse error */}
          {csvParseError && (
            <div className="flex items-start gap-2 text-sm text-red-700 bg-red-50 border border-red-200 rounded-lg p-3">
              <AlertTriangle className="w-4 h-4 mt-0.5 shrink-0" />
              <span>{csvParseError}</span>
            </div>
          )}

          {/* Hidden input for replacing a single file */}
          <input
            ref={replaceInputRef}
            type="file"
            accept=".csv"
            className="hidden"
            onChange={handleReplaceFileSelect}
          />

          {/* Per-file status cards */}
          {csvFileStatuses.length > 0 && (
            <div className="space-y-2">
              <h3 className="text-xs font-semibold text-gray-500 uppercase tracking-wide">
                Files
              </h3>
              <div className="grid gap-2 sm:grid-cols-2">
                {csvFileStatuses.map((fs) => (
                  <div
                    key={fs.filename}
                    className={`flex items-center justify-between rounded-lg border p-3 text-sm ${
                      fs.status === "ok"
                        ? "border-green-200 bg-green-50"
                        : "border-red-200 bg-red-50"
                    }`}
                  >
                    <div className="flex items-center gap-2 min-w-0">
                      {fs.status === "ok" ? (
                        <CircleCheck className="w-4 h-4 text-green-600 shrink-0" />
                      ) : (
                        <CircleX className="w-4 h-4 text-red-600 shrink-0" />
                      )}
                      <div className="min-w-0">
                        <div className="font-medium text-gray-900 truncate">
                          {fs.filename}
                        </div>
                        {fs.status === "ok" ? (
                          <div className="text-xs text-green-700">
                            {fs.row_count} row{fs.row_count !== 1 ? "s" : ""} parsed
                          </div>
                        ) : (
                          <div className="text-xs text-red-700 line-clamp-2">
                            {fs.error}
                          </div>
                        )}
                      </div>
                    </div>
                    <div className="flex items-center gap-1 shrink-0 ml-2">
                      {fs.status === "error" && (
                        <button
                          title="Replace with a corrected file"
                          onClick={(e) => {
                            e.stopPropagation();
                            setReplacingFile(fs.filename);
                            replaceInputRef.current?.click();
                          }}
                          className="p-1 rounded hover:bg-red-100 text-red-600 transition"
                        >
                          <Replace className="w-3.5 h-3.5" />
                        </button>
                      )}
                      <button
                        title="Remove this file"
                        onClick={(e) => {
                          e.stopPropagation();
                          removeFileRows(fs.filename);
                        }}
                        className={`p-1 rounded transition ${
                          fs.status === "ok"
                            ? "hover:bg-green-100 text-green-700"
                            : "hover:bg-red-100 text-red-700"
                        }`}
                      >
                        <XCircle className="w-3.5 h-3.5" />
                      </button>
                    </div>
                  </div>
                ))}
              </div>
            </div>
          )}

          {/* Preview table */}
          {csvRows.length > 0 && (
            <div className="border border-gray-200 rounded-lg overflow-auto max-h-96">
              <table className="w-full text-sm">
                <thead className="bg-gray-50 sticky top-0">
                  <tr>
                    <th className="px-3 py-2 text-left text-xs font-medium text-gray-500 uppercase">
                      #
                    </th>
                    <th className="px-3 py-2 text-left text-xs font-medium text-gray-500 uppercase">
                      Name
                    </th>
                    <th className="px-3 py-2 text-left text-xs font-medium text-gray-500 uppercase">
                      Type
                    </th>
                    <th className="px-3 py-2 text-left text-xs font-medium text-gray-500 uppercase">
                      Budget
                    </th>
                    <th className="px-3 py-2 text-left text-xs font-medium text-gray-500 uppercase">
                      Locations
                    </th>
                    <th className="px-3 py-2 text-left text-xs font-medium text-gray-500 uppercase">
                      Languages
                    </th>
                    <th className="px-3 py-2 text-left text-xs font-medium text-gray-500 uppercase">
                      Keywords
                    </th>
                    <th className="px-3 py-2 text-center text-xs font-medium text-gray-500 uppercase">
                      Status
                    </th>
                    <th className="px-3 py-2 text-center text-xs font-medium text-gray-500 uppercase">
                      Actions
                    </th>
                  </tr>
                </thead>
                <tbody className="divide-y divide-gray-100">
                  {csvRows.map((row, idx) => {
                    const isEditing = editingRowIdx === idx;
                    return (
                      <tr
                        key={`${row.source_file}-${row.row_number}`}
                        className={
                          row.valid
                            ? isEditing
                              ? "bg-blue-50/50"
                              : ""
                            : "bg-red-50/50"
                        }
                      >
                        <td className="px-3 py-2 text-gray-500">
                          {row.row_number}
                        </td>

                        {/* Name */}
                        <td className="px-3 py-2 font-medium text-gray-900 max-w-[200px]">
                          {isEditing ? (
                            <input
                              type="text"
                              value={editBuffer?.name ?? ""}
                              onChange={(e) =>
                                setEditBuffer((b) => ({ ...b, name: e.target.value }))
                              }
                              className="w-full px-1.5 py-0.5 border border-blue-300 rounded text-sm focus:outline-none focus:ring-1 focus:ring-blue-400"
                            />
                          ) : (
                            <span className="truncate block">{row.name || "—"}</span>
                          )}
                        </td>

                        {/* Type */}
                        <td className="px-3 py-2 text-gray-600">
                          {isEditing ? (
                            <select
                              value={editBuffer?.campaign_type ?? ""}
                              onChange={(e) =>
                                setEditBuffer((b) => ({
                                  ...b,
                                  campaign_type: e.target.value,
                                }))
                              }
                              className="px-1.5 py-0.5 border border-blue-300 rounded text-sm focus:outline-none focus:ring-1 focus:ring-blue-400"
                            >
                              {CAMPAIGN_TYPES.map((ct) => (
                                <option key={ct.value} value={ct.value}>
                                  {ct.label}
                                </option>
                              ))}
                            </select>
                          ) : (
                            row.campaign_type
                          )}
                        </td>

                        {/* Budget */}
                        <td className="px-3 py-2 text-gray-600">
                          {isEditing ? (
                            <input
                              type="number"
                              step="0.01"
                              min="0"
                              value={editBuffer?.daily_budget ?? ""}
                              onChange={(e) =>
                                setEditBuffer((b) => ({
                                  ...b,
                                  daily_budget: e.target.value
                                    ? parseFloat(e.target.value)
                                    : null,
                                }))
                              }
                              className="w-20 px-1.5 py-0.5 border border-blue-300 rounded text-sm focus:outline-none focus:ring-1 focus:ring-blue-400"
                            />
                          ) : row.daily_budget != null ? (
                            `$${row.daily_budget.toFixed(2)}`
                          ) : (
                            "—"
                          )}
                        </td>

                        {/* Locations */}
                        <td className="px-3 py-2 text-gray-600 max-w-[140px]">
                          {isEditing ? (
                            <input
                              type="text"
                              value={(editBuffer?.locations ?? []).join(", ")}
                              onChange={(e) =>
                                setEditBuffer((b) => ({
                                  ...b,
                                  locations: e.target.value
                                    .split(",")
                                    .map((s) => s.trim())
                                    .filter(Boolean),
                                }))
                              }
                              className="w-full px-1.5 py-0.5 border border-blue-300 rounded text-sm focus:outline-none focus:ring-1 focus:ring-blue-400"
                              placeholder="US, CA"
                            />
                          ) : (
                            <span className="truncate block">
                              {row.locations.join(", ") || "—"}
                            </span>
                          )}
                        </td>

                        {/* Languages */}
                        <td className="px-3 py-2 text-gray-600">
                          {isEditing ? (
                            <input
                              type="text"
                              value={(editBuffer?.languages ?? []).join(", ")}
                              onChange={(e) =>
                                setEditBuffer((b) => ({
                                  ...b,
                                  languages: e.target.value
                                    .split(",")
                                    .map((s) => s.trim())
                                    .filter(Boolean),
                                }))
                              }
                              className="w-full px-1.5 py-0.5 border border-blue-300 rounded text-sm focus:outline-none focus:ring-1 focus:ring-blue-400"
                              placeholder="en, es"
                            />
                          ) : (
                            row.languages.join(", ") || "—"
                          )}
                        </td>

                        {/* Keywords */}
                        <td className="px-3 py-2 text-gray-600 max-w-[180px]">
                          {isEditing ? (
                            <input
                              type="text"
                              value={(editBuffer?.keywords ?? []).join(", ")}
                              onChange={(e) =>
                                setEditBuffer((b) => ({
                                  ...b,
                                  keywords: e.target.value
                                    .split(",")
                                    .map((s) => s.trim())
                                    .filter(Boolean),
                                }))
                              }
                              className="w-full px-1.5 py-0.5 border border-blue-300 rounded text-sm focus:outline-none focus:ring-1 focus:ring-blue-400"
                              placeholder="keyword1, keyword2"
                            />
                          ) : (
                            <span className="truncate block">
                              {row.keywords.join(", ") || "—"}
                            </span>
                          )}
                        </td>

                        {/* Status */}
                        <td className="px-3 py-2 text-center">
                          {row.valid ? (
                            <CircleCheck className="w-4 h-4 text-green-500 mx-auto" />
                          ) : (
                            <span
                              title={row.errors.join("; ")}
                              className="inline-flex items-center"
                            >
                              <CircleX className="w-4 h-4 text-red-500" />
                            </span>
                          )}
                        </td>

                        {/* Actions */}
                        <td className="px-3 py-2 text-center">
                          {isEditing ? (
                            <div className="flex items-center justify-center gap-1">
                              <button
                                onClick={saveEditRow}
                                title="Save"
                                className="p-1 rounded hover:bg-green-100 text-green-700 transition"
                              >
                                <Save className="w-3.5 h-3.5" />
                              </button>
                              <button
                                onClick={cancelEditRow}
                                title="Cancel"
                                className="p-1 rounded hover:bg-gray-200 text-gray-500 transition"
                              >
                                <RotateCcw className="w-3.5 h-3.5" />
                              </button>
                            </div>
                          ) : (
                            <button
                              onClick={() => startEditRow(idx)}
                              title="Edit row"
                              className="p-1 rounded hover:bg-gray-100 text-gray-500 transition"
                            >
                              <Pencil className="w-3.5 h-3.5" />
                            </button>
                          )}
                        </td>
                      </tr>
                    );
                  })}
                </tbody>
              </table>
            </div>
          )}

          {/* Error rows detail */}
          {csvErrorCount > 0 && (
            <div className="text-sm text-amber-700 bg-amber-50 border border-amber-200 rounded-lg p-3 space-y-1">
              <div className="font-medium flex items-center gap-1">
                <AlertTriangle className="w-4 h-4" />
                {csvErrorCount} row{csvErrorCount !== 1 ? "s" : ""} with errors
                — click <Pencil className="w-3 h-3 inline" /> to fix inline:
              </div>
              <ul className="list-disc list-inside text-xs space-y-0.5">
                {csvRows
                  .filter((r) => !r.valid)
                  .map((r, _i) => {
                    const globalIdx = csvRows.findIndex(
                      (cr) =>
                        cr.row_number === r.row_number &&
                        cr.source_file === r.source_file
                    );
                    return (
                      <li key={`err-${r.source_file}-${r.row_number}`}>
                        <button
                          onClick={() => startEditRow(globalIdx)}
                          className="text-amber-800 underline underline-offset-2 hover:text-amber-950 transition"
                        >
                          Row {r.row_number}
                          {r.source_file ? ` (${r.source_file})` : ""}
                        </button>
                        : {r.errors.join("; ")}
                      </li>
                    );
                  })}
              </ul>
            </div>
          )}

          {/* Actions */}
          <div className="flex gap-2 pt-1">
            <Button
              onClick={handleBatchCreate}
              disabled={csvValidCount === 0 || csvCreating || csvParsing}
            >
              {csvCreating ? (
                <>
                  <RefreshCw className="w-4 h-4 mr-1 animate-spin" />
                  Creating…
                </>
              ) : (
                <>
                  <Rocket className="w-4 h-4 mr-1" />
                  Create {csvValidCount} Campaign
                  {csvValidCount !== 1 ? "s" : ""}
                </>
              )}
            </Button>
            <Button
              variant="secondary"
              onClick={() => {
                setShowCsvImport(false);
                resetCsvState();
              }}
            >
              Cancel
            </Button>
          </div>
        </div>
      )}

      {/* ── BLUEPRINT LIST ─────────────────────────────── */}
      {selectedId === null && !blueprintsQ.isLoading && (
        <div className="space-y-3">
          {blueprints.length === 0 && !showNewForm && (
            <div className="bg-white rounded-xl shadow-sm border border-gray-200 p-12 text-center">
              <Rocket className="w-12 h-12 text-gray-300 mx-auto mb-4" />
              <h3 className="text-lg font-medium text-gray-700 mb-1">
                No blueprints yet
              </h3>
              <p className="text-sm text-gray-500 mb-4">
                Start a new AI-powered campaign build to get started.
              </p>
              <Button onClick={() => setShowNewForm(true)}>
                <Plus className="w-4 h-4 mr-1" />
                New Build
              </Button>
            </div>
          )}

          {blueprints.map((bp) => (
            <div
              key={bp.id}
              className="bg-white rounded-xl shadow-sm border border-gray-200 p-4 flex items-center justify-between hover:border-blue-300 transition cursor-pointer"
              onClick={() => setSelectedId(bp.id)}
            >
              <div className="flex items-center gap-4">
                <div className="p-2 bg-blue-50 rounded-lg">
                  <Rocket className="w-5 h-5 text-blue-600" />
                </div>
                <div>
                  <h3 className="font-medium text-gray-900">{bp.name}</h3>
                  <div className="flex items-center gap-2 mt-1 text-sm text-gray-500">
                    <span>{bp.campaign_type ?? "—"}</span>
                    <span>·</span>
                    <span>
                      {bp.created_at
                        ? new Date(bp.created_at).toLocaleDateString()
                        : "—"}
                    </span>
                    {bp.current_phase && (
                      <>
                        <span>·</span>
                        <span className="flex items-center gap-1">
                          {PHASE_ICONS[bp.current_phase]}
                          {PHASE_LABELS[bp.current_phase]}
                        </span>
                      </>
                    )}
                  </div>
                </div>
              </div>
              <div className="flex items-center gap-3">
                <span
                  className={`px-2.5 py-0.5 rounded-full text-xs font-medium ${
                    STATUS_COLORS[bp.status] ?? "bg-gray-100 text-gray-600"
                  }`}
                >
                  {bp.status.replace(/_/g, " ")}
                </span>
                <button
                  onClick={(e) => {
                    e.stopPropagation();
                    setConfirmDelete(bp.id);
                  }}
                  className="p-1 text-gray-400 hover:text-red-500 transition"
                  title="Delete"
                >
                  <Trash2 className="w-4 h-4" />
                </button>
                <ChevronRight className="w-4 h-4 text-gray-400" />
              </div>
            </div>
          ))}
        </div>
      )}

      {/* ── BLUEPRINT DETAIL ───────────────────────────── */}
      {selectedId !== null && detail && (
        <div className="space-y-6">
          {/* Status Header */}
          <div className="bg-white rounded-xl shadow-sm border border-gray-200 p-6">
            <div className="flex items-center justify-between mb-4">
              <h2 className="text-xl font-semibold text-gray-900">
                {detail.name}
              </h2>
              <span
                className={`px-3 py-1 rounded-full text-sm font-medium ${
                  STATUS_COLORS[detail.status] ?? "bg-gray-100 text-gray-600"
                }`}
              >
                {detail.status.replace(/_/g, " ")}
              </span>
            </div>

            {/* Phase Progress */}
            <div className="flex items-center gap-1">
              {PHASE_ORDER.map((phase, i) => {
                const currentIdx = detail.current_phase
                  ? PHASE_ORDER.indexOf(detail.current_phase)
                  : -1;
                const isDone = i < currentIdx;
                const isCurrent = i === currentIdx;
                return (
                  <div key={phase} className="flex-1 flex flex-col items-center gap-1">
                    <div
                      className={`w-full h-2 rounded-full ${
                        isDone
                          ? "bg-green-500"
                          : isCurrent
                          ? "bg-blue-500 animate-pulse"
                          : "bg-gray-200"
                      }`}
                    />
                    <span
                      className={`text-[10px] leading-tight text-center ${
                        isCurrent
                          ? "text-blue-600 font-semibold"
                          : isDone
                          ? "text-green-600"
                          : "text-gray-400"
                      }`}
                    >
                      {PHASE_LABELS[phase]}
                    </span>
                  </div>
                );
              })}
            </div>

            {/* Error message */}
            {detail.error_message && (
              <div className="mt-4 p-3 bg-red-50 border border-red-200 rounded-lg text-sm text-red-700">
                <strong>Error:</strong> {detail.error_message}
              </div>
            )}

            {/* Campaign resource link */}
            {detail.campaign_resource_name && (
              <div className="mt-4 p-3 bg-emerald-50 border border-emerald-200 rounded-lg text-sm text-emerald-700">
                <strong>Campaign Resource:</strong>{" "}
                <code className="text-xs">{detail.campaign_resource_name}</code>
              </div>
            )}

            {/* Info grid */}
            <div className="grid grid-cols-2 md:grid-cols-4 gap-4 mt-4 text-sm">
              <div>
                <span className="text-gray-500">Type</span>
                <p className="font-medium">{detail.campaign_type ?? "—"}</p>
              </div>
              <div>
                <span className="text-gray-500">Budget</span>
                <p className="font-medium">
                  {detail.daily_budget_micros
                    ? `$${(detail.daily_budget_micros / 1_000_000).toFixed(2)}/day`
                    : "—"}
                </p>
              </div>
              <div>
                <span className="text-gray-500">Bidding</span>
                <p className="font-medium">
                  {detail.bidding_strategy?.replace(/_/g, " ") ?? "—"}
                </p>
              </div>
              <div>
                <span className="text-gray-500">Created</span>
                <p className="font-medium">
                  {detail.created_at
                    ? new Date(detail.created_at).toLocaleString()
                    : "—"}
                </p>
              </div>
            </div>

            {/* Locations & Languages & Keywords */}
            <div className="flex flex-wrap gap-4 mt-4 text-sm">
              {detail.target_locations && detail.target_locations.length > 0 && (
                <div>
                  <span className="text-gray-500">Locations:</span>{" "}
                  {detail.target_locations.join(", ")}
                </div>
              )}
              {detail.target_languages && detail.target_languages.length > 0 && (
                <div>
                  <span className="text-gray-500">Languages:</span>{" "}
                  {detail.target_languages.join(", ")}
                </div>
              )}
              {detail.keyword_themes && detail.keyword_themes.length > 0 && (
                <div>
                  <span className="text-gray-500">Keyword Themes:</span>{" "}
                  {detail.keyword_themes.join(", ")}
                </div>
              )}
            </div>
          </div>

          {/* Strategy Selection */}
          {detail.strategies && detail.strategies.length > 0 && (
            <div className="bg-white rounded-xl shadow-sm border border-gray-200 p-6">
              <h3 className="text-lg font-semibold text-gray-900 mb-4">
                Strategies
                {detail.selected_strategy_index !== null && (
                  <span className="ml-2 text-sm font-normal text-green-600">
                    (Strategy {detail.selected_strategy_index + 1} selected)
                  </span>
                )}
              </h3>
              <div className="grid grid-cols-1 lg:grid-cols-3 gap-4">
                {detail.strategies.map((strategy, idx) => {
                  const isSelected =
                    detail.selected_strategy_index === idx;
                  return (
                    <div
                      key={idx}
                      className={`border rounded-lg p-4 ${
                        isSelected
                          ? "border-blue-500 bg-blue-50"
                          : "border-gray-200 hover:border-blue-300"
                      }`}
                    >
                      <div className="flex items-center justify-between mb-2">
                        <h4 className="font-medium text-gray-900">
                          Strategy {idx + 1}
                        </h4>
                        {isSelected && (
                          <CheckCircle className="w-5 h-5 text-blue-600" />
                        )}
                      </div>
                      <pre className="text-xs text-gray-600 whitespace-pre-wrap max-h-40 overflow-y-auto mb-3">
                        {typeof strategy === "string"
                          ? strategy
                          : JSON.stringify(strategy, null, 2)}
                      </pre>
                      {!isSelected &&
                        detail.status === "STRATEGY_READY" && (
                          <Button
                            size="sm"
                            onClick={() =>
                              strategyMut.mutate({
                                bpId: detail.id,
                                idx,
                              })
                            }
                            disabled={strategyMut.isPending}
                          >
                            Select
                          </Button>
                        )}
                    </div>
                  );
                })}
              </div>
            </div>
          )}

          {/* Audit Summary */}
          {detail.audit_data && (
            <CollapsibleSection title="Audit Data">
              <pre className="text-xs text-gray-600 whitespace-pre-wrap max-h-60 overflow-y-auto">
                {JSON.stringify(detail.audit_data, null, 2)}
              </pre>
            </CollapsibleSection>
          )}

          {/* Analysis Summary */}
          {detail.analysis_summary && (
            <CollapsibleSection title="Analysis Summary">
              <pre className="text-xs text-gray-600 whitespace-pre-wrap max-h-60 overflow-y-auto">
                {JSON.stringify(detail.analysis_summary, null, 2)}
              </pre>
            </CollapsibleSection>
          )}

          {/* Creative Assets */}
          {detail.creative_assets && (
            <CollapsibleSection title="Creative Assets">
              <pre className="text-xs text-gray-600 whitespace-pre-wrap max-h-60 overflow-y-auto">
                {JSON.stringify(detail.creative_assets, null, 2)}
              </pre>
            </CollapsibleSection>
          )}

          {/* Build Result */}
          {detail.build_result && (
            <CollapsibleSection title="Build Result">
              <pre className="text-xs text-gray-600 whitespace-pre-wrap max-h-60 overflow-y-auto">
                {JSON.stringify(detail.build_result, null, 2)}
              </pre>
            </CollapsibleSection>
          )}

          {/* Review Report */}
          {detail.review_report && (
            <CollapsibleSection title="Review Report">
              <pre className="text-xs text-gray-600 whitespace-pre-wrap max-h-60 overflow-y-auto">
                {JSON.stringify(detail.review_report, null, 2)}
              </pre>
            </CollapsibleSection>
          )}

          {/* Safety Banner */}
          <div className="flex items-start gap-3 rounded-lg border border-blue-200 bg-blue-50 p-3">
            <ShieldCheck className="w-5 h-5 text-blue-600 flex-shrink-0 mt-0.5" />
            <p className="text-sm text-blue-800">
              No changes are made to your Google Ads account until you explicitly
              approve the final deployment. Each step requires your confirmation.
            </p>
          </div>

          {/* Phase Actions */}
          <div className="flex flex-wrap gap-2">
            {detail.status === "DRAFT" && (
              <Button onClick={() => setConfirmAdvance(detail.id)} loading={advanceMut.isPending}>
                <Play className="w-4 h-4 mr-1" />
                Run AI Audit
              </Button>
            )}
            {detail.status === "AUDIT_COMPLETE" && (
              <Button onClick={() => setConfirmAdvance(detail.id)} loading={advanceMut.isPending}>
                <Target className="w-4 h-4 mr-1" />
                Generate Strategies
              </Button>
            )}
            {detail.status === "STRATEGY_READY" && detail.selected_strategy_index != null && (
              <Button onClick={() => setConfirmAdvance(detail.id)} loading={advanceMut.isPending}>
                <Palette className="w-4 h-4 mr-1" />
                Generate Creatives
              </Button>
            )}
            {detail.status === "STRATEGY_READY" && detail.selected_strategy_index == null && (
              <p className="text-sm text-amber-600 self-center">
                Select a strategy above before continuing.
              </p>
            )}
            {detail.status === "CREATIVE_READY" && (
              <Button onClick={() => setConfirmAdvance(detail.id)} loading={advanceMut.isPending}>
                <Wrench className="w-4 h-4 mr-1" />
                Build Campaign Preview
              </Button>
            )}
            {detail.status === "BUILT" && (
              <Button onClick={() => setConfirmAdvance(detail.id)} loading={advanceMut.isPending}>
                <Search className="w-4 h-4 mr-1" />
                Run Compliance Review
              </Button>
            )}
            {detail.status === "REVIEW_PASSED" && (
              <Button onClick={() => setConfirmApprove(detail.id)}>
                <Rocket className="w-4 h-4 mr-1" />
                Approve &amp; Deploy
              </Button>
            )}
            {detail.status === "DEPLOYED" && (
              <span className="inline-flex items-center gap-1 text-sm text-green-700 font-medium">
                <CheckCircle className="w-4 h-4" /> Campaign deployed
              </span>
            )}
            <Button
              variant="secondary"
              onClick={() => setConfirmDelete(detail.id)}
            >
              <Trash2 className="w-4 h-4 mr-1" />
              Delete
            </Button>
          </div>
        </div>
      )}

      {selectedId !== null && detailQ.isLoading && <LoadingSpinner />}
      {selectedId !== null && detailQ.isError && (
        <ErrorBanner message="Failed to load blueprint details." />
      )}

      {/* ── MODALS ─────────────────────────────────────── */}
      {confirmDelete !== null && (
        <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/40">
          <div className="bg-white rounded-xl shadow-xl p-6 max-w-md w-full space-y-4">
            <h2 className="text-lg font-semibold text-gray-900">Delete Blueprint</h2>
            <p className="text-sm text-gray-600">Are you sure you want to permanently delete this blueprint?</p>
            <div className="flex justify-end gap-3 pt-2">
              <Button variant="secondary" onClick={() => setConfirmDelete(null)} disabled={deleteMut.isPending}>Cancel</Button>
              <Button variant="danger" onClick={() => deleteMut.mutate(confirmDelete)} disabled={deleteMut.isPending}>
                {deleteMut.isPending ? "Deleting…" : "Delete"}
              </Button>
            </div>
          </div>
        </div>
      )}

      {confirmApprove !== null && (
        <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/40">
          <div className="bg-white rounded-xl shadow-xl p-6 max-w-md w-full space-y-4">
            <h2 className="text-lg font-semibold text-gray-900">Approve &amp; Deploy</h2>
            <p className="text-sm text-gray-600">This will deploy the campaign to Google Ads. This action cannot be undone.</p>
            <div className="flex justify-end gap-3 pt-2">
              <Button variant="secondary" onClick={() => setConfirmApprove(null)} disabled={approveMut.isPending}>Cancel</Button>
              <Button variant="primary" onClick={() => approveMut.mutate(confirmApprove)} disabled={approveMut.isPending}>
                {approveMut.isPending ? "Deploying…" : "Deploy"}
              </Button>
            </div>
          </div>
        </div>
      )}

      {confirmAdvance !== null && (
        <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/40">
          <div className="bg-white rounded-xl shadow-xl p-6 max-w-md w-full space-y-4">
            <h2 className="text-lg font-semibold text-gray-900">Advance to Next Phase</h2>
            <p className="text-sm text-gray-600">
              This will run the next AI pipeline step on this blueprint. No
              changes are made to your Google Ads account until you approve the
              final deployment.
            </p>
            <div className="flex justify-end gap-3 pt-2">
              <Button variant="secondary" onClick={() => setConfirmAdvance(null)} disabled={advanceMut.isPending}>Cancel</Button>
              <Button variant="primary" onClick={() => advanceMut.mutate(confirmAdvance)} disabled={advanceMut.isPending}>
                {advanceMut.isPending ? "Running…" : "Confirm & Continue"}
              </Button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}

// ── Collapsible Section ─────────────────────────────────────

function CollapsibleSection({
  title,
  children,
}: {
  title: string;
  children: React.ReactNode;
}) {
  const [open, setOpen] = useState(false);
  return (
    <div className="bg-white rounded-xl shadow-sm border border-gray-200 overflow-hidden">
      <button
        onClick={() => setOpen(!open)}
        className="w-full flex items-center justify-between p-4 text-left hover:bg-gray-50 transition"
      >
        <h3 className="text-sm font-semibold text-gray-900">{title}</h3>
        <ChevronRight
          className={`w-4 h-4 text-gray-400 transition-transform ${
            open ? "rotate-90" : ""
          }`}
        />
      </button>
      {open && <div className="px-4 pb-4">{children}</div>}
    </div>
  );
}
