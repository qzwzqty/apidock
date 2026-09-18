import * as React from "react";
import { cn } from "@/lib/utils";

type Variant = "default" | "ghost" | "outline" | "danger";
type Size = "sm" | "md" | "icon";

const variantClass: Record<Variant, string> = {
  default:
    "bg-accent text-accent-foreground hover:opacity-90 disabled:opacity-50",
  ghost: "text-muted-foreground hover:bg-muted hover:text-foreground",
  outline: "border border-border hover:bg-muted",
  danger: "bg-red-600/80 text-white hover:bg-red-600",
};

const sizeClass: Record<Size, string> = {
  sm: "h-7 px-2 text-xs",
  md: "h-8 px-3 text-sm",
  icon: "h-6 w-6",
};

export function Button({
  variant = "default",
  size = "md",
  className,
  ...props
}: React.ButtonHTMLAttributes<HTMLButtonElement> & {
  variant?: Variant;
  size?: Size;
}) {
  return (
    <button
      className={cn(
        "inline-flex items-center justify-center gap-1.5 rounded-md font-medium transition-colors outline-none focus-visible:ring-2 focus-visible:ring-ring cursor-pointer select-none disabled:cursor-not-allowed",
        variantClass[variant],
        sizeClass[size],
        className,
      )}
      {...props}
    />
  );
}