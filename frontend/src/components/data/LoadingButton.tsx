"use client";

import * as React from "react";
import { type ReactNode, forwardRef } from "react";
import { Loader2 } from "lucide-react";
import { Button, buttonVariants } from "@/components/ui/button";
import { cn } from "@/lib/utils";
import { type VariantProps } from "class-variance-authority";

type ShadcnButtonProps = React.ComponentProps<"button"> &
  VariantProps<typeof buttonVariants> & { asChild?: boolean };

/** Map legacy variant names to shadcn button variants. */
const variantMap = {
  primary: "default",
  secondary: "outline",
  danger: "destructive",
  ghost: "ghost",
} as const;

/** Map legacy size names to shadcn button sizes. */
const sizeMap = {
  sm: "sm",
  md: "default",
  lg: "lg",
} as const;

type LegacyVariant = keyof typeof variantMap;
type LegacySize = keyof typeof sizeMap;

interface LoadingButtonProps extends Omit<ShadcnButtonProps, "variant" | "size"> {
  variant?: LegacyVariant | ShadcnButtonProps["variant"];
  size?: LegacySize | ShadcnButtonProps["size"];
  loading?: boolean;
  icon?: ReactNode;
}

const LoadingButton = forwardRef<HTMLButtonElement, LoadingButtonProps>(
  (
    {
      variant = "primary",
      size = "md",
      loading = false,
      icon,
      children,
      className,
      disabled,
      ...props
    },
    ref,
  ) => {
    const resolvedVariant =
      variant && variant in variantMap
        ? variantMap[variant as LegacyVariant]
        : ((variant ?? "default") as ShadcnButtonProps["variant"]);

    const resolvedSize =
      size && size in sizeMap
        ? sizeMap[size as LegacySize]
        : ((size ?? "default") as ShadcnButtonProps["size"]);

    return (
      <Button
        ref={ref}
        variant={resolvedVariant}
        size={resolvedSize}
        disabled={disabled || loading}
        className={cn("gap-2", className)}
        {...props}
      >
        {loading ? (
          <Loader2 className="size-4 animate-spin" />
        ) : icon ? (
          <span className="size-4 [&>svg]:size-full">{icon}</span>
        ) : null}
        {children}
      </Button>
    );
  },
);

LoadingButton.displayName = "LoadingButton";

export default LoadingButton;
export { LoadingButton };
export type { LoadingButtonProps };
