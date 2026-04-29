import { clsx, type ClassValue } from "clsx";
import { twMerge } from "tailwind-merge";

/**
 * Merge Tailwind classes safely. The shadcn-canonical helper —
 * concatenates `clsx` arguments and lets `tailwind-merge` resolve
 * conflicting Tailwind classes (later wins). Used in every shadcn
 * component's `className` composition.
 */
export function cn(...inputs: ClassValue[]) {
  return twMerge(clsx(inputs));
}
