"use client";

import { useEffect, useState } from "react";
import { useRouter } from "next/navigation";
import {
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
} from "lucide-react";
import {
  CommandDialog,
  CommandInput,
  CommandList,
  CommandEmpty,
  CommandGroup,
  CommandItem,
  CommandShortcut,
} from "@/components/ui/command";

const PAGES = [
  { href: "/", label: "Dashboard", description: "Overview & KPIs", icon: LayoutDashboard, shortcut: "1", keywords: ["home", "overview", "kpi"] },
  { href: "/campaigns", label: "Campaigns", description: "Manage all campaigns", icon: Megaphone, shortcut: "2", keywords: ["campaign"] },
  { href: "/ad-groups", label: "Ad Groups", description: "Manage ad groups", icon: Layers, shortcut: "3", keywords: ["adgroup", "group"] },
  { href: "/keywords", label: "Keywords", description: "Manage keywords", icon: Key, shortcut: "4", keywords: ["keyword", "search term"] },
  { href: "/ads", label: "Ads", description: "Manage ads & creatives", icon: FileText, shortcut: "5", keywords: ["ad", "creative", "rsa"] },
  { href: "/recommendations", label: "Recommendations", description: "AI-powered insights", icon: Lightbulb, shortcut: "6", keywords: ["recommend", "optimize", "insight"] },
  { href: "/builder", label: "Campaign Builder", description: "Create new campaigns", icon: Rocket, shortcut: "7", keywords: ["build", "create", "new"] },
  { href: "/chat", label: "AI Chat", description: "Talk to your ads agent", icon: MessageSquare, shortcut: "8", keywords: ["chat", "ask", "ai", "gemini"] },
  { href: "/logs", label: "Activity Log", description: "Mutation audit trail", icon: History, shortcut: "9", keywords: ["log", "history", "audit"] },
  { href: "/settings", label: "Settings", description: "Account configuration", icon: Settings, shortcut: "0", keywords: ["setting", "config", "profile"] },
];

export function CommandPalette() {
  const [open, setOpen] = useState(false);
  const router = useRouter();

  useEffect(() => {
    function onKeyDown(e: KeyboardEvent) {
      if ((e.metaKey || e.ctrlKey) && e.key === "k") {
        e.preventDefault();
        setOpen((o) => !o);
      }
    }
    document.addEventListener("keydown", onKeyDown);
    return () => document.removeEventListener("keydown", onKeyDown);
  }, []);

  const navigate = (href: string) => {
    router.push(href);
    setOpen(false);
  };

  return (
    <CommandDialog open={open} onOpenChange={setOpen}>
      <CommandInput placeholder="Search pages & actions..." />
      <CommandList>
        <CommandEmpty>No results found.</CommandEmpty>
        <CommandGroup heading="Navigate">
          {PAGES.map((page) => {
            const Icon = page.icon;
            return (
              <CommandItem
                key={page.href}
                value={[page.label, ...page.keywords].join(" ")}
                onSelect={() => navigate(page.href)}
              >
                <Icon className="mr-2 size-4" />
                <div className="flex-1">
                  <span>{page.label}</span>
                  {page.description && (
                    <span className="ml-2 text-muted-foreground text-xs">
                      {page.description}
                    </span>
                  )}
                </div>
                <CommandShortcut>Ctrl+{page.shortcut}</CommandShortcut>
              </CommandItem>
            );
          })}
        </CommandGroup>
      </CommandList>
    </CommandDialog>
  );
}
