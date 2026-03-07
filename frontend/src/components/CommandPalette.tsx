"use client";

import { useState, useEffect, useRef, useCallback } from "react";
import { useRouter } from "next/navigation";
import {
  Search,
  LayoutDashboard,
  Megaphone,
  Layers,
  Key,
  FileText,
  Lightbulb,
  MessageSquare,
  History,
  Settings,
  Rocket,
  Command,
  ArrowRight,
} from "lucide-react";
import { cn } from "@/lib/utils";

interface CommandItem {
  id: string;
  label: string;
  description?: string;
  icon: React.ReactNode;
  action: () => void;
  keywords?: string[];
  group: string;
}

export default function CommandPalette() {
  const [open, setOpen] = useState(false);
  const [query, setQuery] = useState("");
  const [activeIdx, setActiveIdx] = useState(0);
  const inputRef = useRef<HTMLInputElement>(null);
  const router = useRouter();

  const items: CommandItem[] = [
    { id: "dashboard", label: "Dashboard", description: "Overview & KPIs", icon: <LayoutDashboard className="w-4 h-4" />, action: () => router.push("/"), keywords: ["home", "overview", "kpi"], group: "Navigate" },
    { id: "campaigns", label: "Campaigns", description: "Manage all campaigns", icon: <Megaphone className="w-4 h-4" />, action: () => router.push("/campaigns"), keywords: ["campaign"], group: "Navigate" },
    { id: "ad-groups", label: "Ad Groups", description: "Manage ad groups", icon: <Layers className="w-4 h-4" />, action: () => router.push("/ad-groups"), keywords: ["adgroup", "group"], group: "Navigate" },
    { id: "keywords", label: "Keywords", description: "Manage keywords", icon: <Key className="w-4 h-4" />, action: () => router.push("/keywords"), keywords: ["keyword", "search term"], group: "Navigate" },
    { id: "ads", label: "Ads", description: "Manage ads & creatives", icon: <FileText className="w-4 h-4" />, action: () => router.push("/ads"), keywords: ["ad", "creative", "rsa"], group: "Navigate" },
    { id: "recommendations", label: "Recommendations", description: "AI-powered insights", icon: <Lightbulb className="w-4 h-4" />, action: () => router.push("/recommendations"), keywords: ["recommend", "optimize", "insight"], group: "Navigate" },
    { id: "builder", label: "Campaign Builder", description: "Create new campaigns", icon: <Rocket className="w-4 h-4" />, action: () => router.push("/builder"), keywords: ["build", "create", "new"], group: "Navigate" },
    { id: "chat", label: "AI Chat", description: "Talk to your ads agent", icon: <MessageSquare className="w-4 h-4" />, action: () => router.push("/chat"), keywords: ["chat", "ask", "ai", "gemini"], group: "Navigate" },
    { id: "logs", label: "Activity Log", description: "Mutation audit trail", icon: <History className="w-4 h-4" />, action: () => router.push("/logs"), keywords: ["log", "history", "audit"], group: "Navigate" },
    { id: "settings", label: "Settings", description: "Account configuration", icon: <Settings className="w-4 h-4" />, action: () => router.push("/settings"), keywords: ["setting", "config", "profile"], group: "Navigate" },
  ];

  const filtered = query.trim()
    ? items.filter((item) => {
        const q = query.toLowerCase();
        return (
          item.label.toLowerCase().includes(q) ||
          item.description?.toLowerCase().includes(q) ||
          item.keywords?.some((k) => k.includes(q))
        );
      })
    : items;

  const groups = [...new Set(filtered.map((i) => i.group))];

  // Keyboard shortcut to open
  useEffect(() => {
    function handleKeyDown(e: KeyboardEvent) {
      if ((e.metaKey || e.ctrlKey) && e.key === "k") {
        e.preventDefault();
        setOpen((o) => !o);
      }
      if (e.key === "Escape") setOpen(false);
    }
    document.addEventListener("keydown", handleKeyDown);
    return () => document.removeEventListener("keydown", handleKeyDown);
  }, []);

  // Focus input when opening
  useEffect(() => {
    if (open) {
      setQuery("");
      setActiveIdx(0);
      setTimeout(() => inputRef.current?.focus(), 0);
    }
  }, [open]);

  // Arrow + enter navigation
  const handleInputKeyDown = useCallback(
    (e: React.KeyboardEvent) => {
      if (e.key === "ArrowDown") {
        e.preventDefault();
        setActiveIdx((i) => Math.min(i + 1, filtered.length - 1));
      } else if (e.key === "ArrowUp") {
        e.preventDefault();
        setActiveIdx((i) => Math.max(i - 1, 0));
      } else if (e.key === "Enter" && filtered[activeIdx]) {
        e.preventDefault();
        filtered[activeIdx].action();
        setOpen(false);
      }
    },
    [filtered, activeIdx]
  );

  if (!open) return null;

  return (
    <div className="fixed inset-0 z-[200] flex items-start justify-center pt-[15vh]">
      {/* Backdrop */}
      <div className="absolute inset-0 bg-black/40 backdrop-blur-sm" onClick={() => setOpen(false)} />

      {/* Dialog */}
      <div className="relative w-full max-w-lg bg-white rounded-2xl shadow-2xl border border-gray-200/50 overflow-hidden animate-scale-in">
        {/* Search Input */}
        <div className="flex items-center gap-3 px-4 py-3 border-b border-gray-100">
          <Search className="w-5 h-5 text-gray-400 shrink-0" />
          <input
            ref={inputRef}
            type="text"
            value={query}
            onChange={(e) => { setQuery(e.target.value); setActiveIdx(0); }}
            onKeyDown={handleInputKeyDown}
            placeholder="Search pages & actions..."
            className="flex-1 text-sm bg-transparent outline-none placeholder:text-gray-400"
          />
          <kbd className="hidden sm:inline-flex items-center gap-0.5 px-2 py-0.5 text-[10px] font-mono text-gray-400 bg-gray-100 border border-gray-200 rounded">
            ESC
          </kbd>
        </div>

        {/* Results */}
        <div className="max-h-80 overflow-y-auto py-2">
          {filtered.length === 0 ? (
            <div className="px-4 py-8 text-center text-sm text-gray-400">
              No results for &ldquo;{query}&rdquo;
            </div>
          ) : (
            groups.map((group) => (
              <div key={group}>
                <div className="px-4 py-1.5 text-[10px] font-semibold text-gray-400 uppercase tracking-wider">
                  {group}
                </div>
                {filtered
                  .filter((i) => i.group === group)
                  .map((item) => {
                    const flatIdx = filtered.indexOf(item);
                    return (
                      <button
                        key={item.id}
                        onClick={() => { item.action(); setOpen(false); }}
                        onMouseEnter={() => setActiveIdx(flatIdx)}
                        className={cn(
                          "w-full flex items-center gap-3 px-4 py-2.5 text-left transition-colors",
                          flatIdx === activeIdx ? "bg-indigo-50 text-indigo-700" : "text-gray-700 hover:bg-gray-50"
                        )}
                      >
                        <span className={cn("shrink-0", flatIdx === activeIdx ? "text-indigo-500" : "text-gray-400")}>
                          {item.icon}
                        </span>
                        <div className="flex-1 min-w-0">
                          <p className="text-sm font-medium truncate">{item.label}</p>
                          {item.description && (
                            <p className="text-xs text-gray-400 truncate">{item.description}</p>
                          )}
                        </div>
                        {flatIdx === activeIdx && (
                          <ArrowRight className="w-3.5 h-3.5 text-indigo-400 shrink-0" />
                        )}
                      </button>
                    );
                  })}
              </div>
            ))
          )}
        </div>

        {/* Footer hint */}
        <div className="flex items-center gap-4 px-4 py-2 border-t border-gray-100 bg-gray-50/50 text-[10px] text-gray-400">
          <span className="flex items-center gap-1"><kbd className="px-1 py-0.5 bg-gray-200 rounded text-[9px]">&uarr;</kbd><kbd className="px-1 py-0.5 bg-gray-200 rounded text-[9px]">&darr;</kbd> Navigate</span>
          <span className="flex items-center gap-1"><kbd className="px-1.5 py-0.5 bg-gray-200 rounded text-[9px]">Enter</kbd> Open</span>
          <span className="flex items-center gap-1"><kbd className="px-1.5 py-0.5 bg-gray-200 rounded text-[9px]">Esc</kbd> Close</span>
        </div>
      </div>
    </div>
  );
}
