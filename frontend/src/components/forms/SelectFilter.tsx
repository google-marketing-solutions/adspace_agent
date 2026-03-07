"use client";

import { useState } from "react";
import { Check, ChevronsUpDown, X } from "lucide-react";
import { cn } from "@/lib/utils";
import { Button } from "@/components/ui/button";
import {
  Popover,
  PopoverContent,
  PopoverTrigger,
} from "@/components/ui/popover";
import {
  Command,
  CommandEmpty,
  CommandGroup,
  CommandInput,
  CommandItem,
  CommandList,
} from "@/components/ui/command";

interface SelectFilterProps {
  label: string;
  value: string;
  options: { value: string; label: string; count?: number }[];
  onChange: (value: string) => void;
  allLabel?: string;
}

export function SelectFilter({
  label,
  value,
  options,
  onChange,
  allLabel = "All",
}: SelectFilterProps) {
  const [open, setOpen] = useState(false);
  const active = value !== "";
  const selectedLabel =
    options.find((o) => o.value === value)?.label ?? allLabel;

  return (
    <Popover open={open} onOpenChange={setOpen}>
      <PopoverTrigger asChild>
        <Button
          variant="outline"
          size="sm"
          className={cn(
            "h-8 gap-1.5 text-xs font-medium",
            active &&
              "bg-primary/10 border-primary/30 text-primary hover:bg-primary/15 hover:text-primary",
          )}
        >
          <span className={cn(!active && "text-muted-foreground")}>
            {label}:
          </span>
          <span className="max-w-[120px] truncate font-semibold">
            {active ? selectedLabel : allLabel}
          </span>
          {active ? (
            <X
              className="size-3 ml-0.5 opacity-60 hover:opacity-100"
              onClick={(e) => {
                e.stopPropagation();
                onChange("");
              }}
            />
          ) : (
            <ChevronsUpDown className="size-3 opacity-50" />
          )}
        </Button>
      </PopoverTrigger>
      <PopoverContent className="w-[220px] p-0" align="start">
        <Command>
          <CommandInput placeholder={`Search ${label.toLowerCase()}…`} />
          <CommandList>
            <CommandEmpty>No results.</CommandEmpty>
            <CommandGroup>
              <CommandItem
                value="__all__"
                onSelect={() => {
                  onChange("");
                  setOpen(false);
                }}
                className="text-sm"
              >
                <Check
                  className={cn(
                    "mr-2 size-4",
                    value === "" ? "opacity-100" : "opacity-0",
                  )}
                />
                {allLabel}
              </CommandItem>
              {options.map((opt) => (
                <CommandItem
                  key={opt.value}
                  value={opt.value}
                  onSelect={() => {
                    onChange(opt.value === value ? "" : opt.value);
                    setOpen(false);
                  }}
                  className="text-sm"
                >
                  <Check
                    className={cn(
                      "mr-2 size-4",
                      value === opt.value ? "opacity-100" : "opacity-0",
                    )}
                  />
                  <span className="flex-1 truncate">{opt.label}</span>
                  {opt.count !== undefined && (
                    <span className="text-xs text-muted-foreground ml-auto pl-2">
                      {opt.count}
                    </span>
                  )}
                </CommandItem>
              ))}
            </CommandGroup>
          </CommandList>
        </Command>
      </PopoverContent>
    </Popover>
  );
}
