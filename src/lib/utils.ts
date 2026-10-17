import { clsx, type ClassValue } from "clsx";
import { twMerge } from "tailwind-merge";

/** Tailwind classes, conditionally joined, with later ones winning a conflict (shadcn's helper). */
export function cn(...inputs: ClassValue[]): string {
  return twMerge(clsx(inputs));
}
