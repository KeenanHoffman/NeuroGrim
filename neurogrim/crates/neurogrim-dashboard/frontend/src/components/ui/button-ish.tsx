import * as React from "react";
import { cn } from "@/lib/utils";

/**
 * Minimal shadcn-shaped Button. Named `button-ish` to make clear
 * this is a slim local primitive — not the full shadcn Button
 * (we don't pull the CVA variants graph for now). Variants:
 *
 * - `default` — solid primary surface
 * - `ghost`   — subtle hover-tint, no border
 *
 * Sizes:
 *
 * - `default` — h-9 px-3 (default for most actions)
 * - `sm`      — h-7 px-2 (toolbar / inline density)
 *
 * Used by the layout editor toolbar + per-widget controls. If
 * the surface grows beyond these two variants, swap for the full
 * shadcn Button.
 */

type Variant = "default" | "ghost";
type Size = "default" | "sm";

interface ButtonProps
  extends React.ButtonHTMLAttributes<HTMLButtonElement> {
  variant?: Variant;
  size?: Size;
}

export const Button = React.forwardRef<HTMLButtonElement, ButtonProps>(
  ({ className, variant = "default", size = "default", ...props }, ref) => {
    const variantClass = {
      default:
        "bg-secondary text-secondary-foreground hover:bg-secondary/80",
      ghost:
        "text-muted-foreground hover:text-foreground hover:bg-muted/50",
    }[variant];
    const sizeClass = {
      default: "h-9 px-3 text-sm",
      sm: "h-7 px-2 text-xs",
    }[size];
    return (
      <button
        ref={ref}
        className={cn(
          "inline-flex items-center justify-center rounded-md font-medium transition-colors",
          "focus:outline-none focus:ring-2 focus:ring-ring focus:ring-offset-2",
          "disabled:cursor-not-allowed disabled:opacity-50",
          variantClass,
          sizeClass,
          className
        )}
        {...props}
      />
    );
  }
);
Button.displayName = "Button";
