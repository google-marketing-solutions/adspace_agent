"use client";

import Link from "next/link";
import { usePathname } from "next/navigation";
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
  ChevronLeft,
  ChevronRight,
  Rocket,
  Command,
  Zap,
} from "lucide-react";
import { cn } from "@/lib/utils";
import { useState } from "react";

const NAV_SECTIONS = [
  {
    label: "Overview",
    items: [
      { href: "/", label: "Dashboard", icon: LayoutDashboard },
    ],
  },
  {
    label: "Campaign Management",
    items: [
      { href: "/campaigns", label: "Campaigns", icon: Megaphone },
      { href: "/ad-groups", label: "Ad Groups", icon: Layers },
      { href: "/keywords", label: "Keywords", icon: Key },
      { href: "/ads", label: "Ads", icon: FileText },
    ],
  },
  {
    label: "AI & Automation",
    items: [
      { href: "/recommendations", label: "Recommendations", icon: Lightbulb },
      { href: "/builder", label: "Builder", icon: Rocket },
      { href: "/chat", label: "AI Chat", icon: MessageSquare },
    ],
  },
  {
    label: "System",
    items: [
      { href: "/logs", label: "Activity Log", icon: History },
      { href: "/settings", label: "Settings", icon: Settings },
    ],
  },
];

export default function Sidebar() {
  const pathname = usePathname();
  const [collapsed, setCollapsed] = useState(false);

  return (
    <aside
      className={cn(
        "flex flex-col h-screen bg-[#0f1729] text-gray-100 transition-all duration-300 ease-in-out border-r border-gray-800/50",
        collapsed ? "w-[68px]" : "w-64"
      )}
    >
      {/* Logo */}
      <div className="flex items-center gap-3 px-4 py-5 border-b border-gray-800/50">
        <div className="w-9 h-9 bg-gradient-to-br from-indigo-500 to-violet-600 rounded-xl flex items-center justify-center font-bold text-sm shadow-lg shadow-indigo-500/20 shrink-0">
          <Zap className="w-4.5 h-4.5 text-white" />
        </div>
        {!collapsed && (
          <div className="overflow-hidden">
            <span className="font-bold text-lg tracking-tight bg-gradient-to-r from-white to-gray-300 bg-clip-text text-transparent">
              AdSpace
            </span>
            <span className="block text-[10px] text-gray-500 font-medium -mt-0.5">Agent Platform</span>
          </div>
        )}
      </div>

      {/* Cmd+K shortcut hint */}
      {!collapsed && (
        <div className="mx-3 mt-4 mb-1">
          <button
            onClick={() => {
              const event = new KeyboardEvent("keydown", { key: "k", metaKey: true, ctrlKey: true, bubbles: true });
              document.dispatchEvent(event);
            }}
            className="w-full flex items-center gap-2.5 px-3 py-2 rounded-lg bg-gray-800/50 border border-gray-700/50 text-gray-400 text-xs hover:bg-gray-800 hover:text-gray-300 transition-colors"
          >
            <Command className="w-3.5 h-3.5" />
            <span className="flex-1 text-left">Quick search...</span>
            <kbd className="px-1.5 py-0.5 bg-gray-700/50 rounded text-[10px] font-mono text-gray-500">Ctrl K</kbd>
          </button>
        </div>
      )}

      {/* Navigation */}
      <nav className="flex-1 py-3 overflow-y-auto">
        {NAV_SECTIONS.map((section) => (
          <div key={section.label} className="mb-2">
            {!collapsed && (
              <div className="px-4 py-1.5 text-[10px] font-semibold text-gray-600 uppercase tracking-wider">
                {section.label}
              </div>
            )}
            <ul className="space-y-0.5 px-2">
              {section.items.map(({ href, label, icon: Icon }) => {
                const active = href === "/" ? pathname === "/" : pathname.startsWith(href);
                return (
                  <li key={href}>
                    <Link
                      href={href}
                      className={cn(
                        "flex items-center gap-3 px-3 py-2 rounded-lg text-[13px] font-medium transition-all duration-150",
                        active
                          ? "bg-indigo-500/15 text-indigo-400 shadow-sm shadow-indigo-500/5"
                          : "text-gray-400 hover:bg-gray-800/60 hover:text-gray-200"
                      )}
                      title={collapsed ? label : undefined}
                    >
                      <Icon className={cn("w-[18px] h-[18px] shrink-0", active && "text-indigo-400")} />
                      {!collapsed && <span>{label}</span>}
                      {!collapsed && active && (
                        <div className="ml-auto w-1.5 h-1.5 rounded-full bg-indigo-400" />
                      )}
                    </Link>
                  </li>
                );
              })}
            </ul>
          </div>
        ))}
      </nav>

      {/* Bottom: Collapse toggle */}
      <div className="border-t border-gray-800/50 p-2">
        <button
          onClick={() => setCollapsed(!collapsed)}
          className="w-full flex items-center justify-center gap-2 py-2.5 rounded-lg text-gray-500 hover:text-gray-300 hover:bg-gray-800/50 transition-colors text-xs"
        >
          {collapsed ? (
            <ChevronRight className="w-4 h-4" />
          ) : (
            <>
              <ChevronLeft className="w-4 h-4" />
              <span>Collapse</span>
            </>
          )}
        </button>
      </div>
    </aside>
  );
}
