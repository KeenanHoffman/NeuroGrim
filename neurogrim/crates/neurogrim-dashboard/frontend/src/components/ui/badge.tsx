import * as React from "react";
import { cva, type VariantProps } from "class-variance-authority";
import { cn } from "@/lib/utils";

/**
 * shadcn-canonical Badge primitive with three variants (default,
 * secondary, destructive, outline). Used for domain count chips,
 * trajectory direction tags, federation peer counts, and
 * recommendation gate labels.
 *
 * Source: https://ui.shadcn.com/docs/components/badge.
 */
const badgeVariants = cva(
  "inline-flex items-center rounded-md border px-2.5 py-0.5 text-xs font-semibold transition-colors focus:outline-none focus:ring-2 focus:ring-ring focus:ring-offset-2",
  {
    variants: {
      variant: {
        default:
          "border-transparent bg-primary text-primary-foreground hover:bg-primary/80",
        secondary:
          "border-transparent bg-secondary text-secondary-foreground hover:bg-secondary/80",
        destructive:
          "border-transparent bg-destructive text-destructive-foreground hover:bg-destructive/80",
        outline: "text-foreground",
        // Custom variants for our score/trajectory traffic-light UX:
        success:
          "border-transparent bg-emerald-500/15 text-emerald-400 ring-1 ring-emerald-500/30",
        warning:
          "border-transparent bg-amber-500/15 text-amber-400 ring-1 ring-amber-500/30",
        danger:
          "border-transparent bg-red-500/15 text-red-400 ring-1 ring-red-500/30",
      },
    },
    defaultVariants: {
      variant: "default",
    },
  }
);

export interface BadgeProps
  extends React.HTMLAttributes<HTMLDivElement>,
    VariantProps<typeof badgeVariants> {}

function Badge({ className, variant, ...props }: BadgeProps) {
  return (
    <div className={cn(badgeVariants({ variant }), className)} {...props} />
  );
}

export { Badge, badgeVariants };
