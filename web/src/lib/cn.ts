import { extendTailwindMerge } from "tailwind-merge";

// tailwind-merge resolves conflicting utilities (`p-2` then `p-4` keeps `p-4`),
// so a caller's `className` reliably overrides a component's defaults. It is
// told about the custom scales in styles/tokens.css, which it cannot infer.
const merge = extendTailwindMerge({
  extend: {
    theme: {
      shadow: ["raised", "overlay", "poster"],
      radius: ["poster"],
      text: ["2xs"],
    },
  },
});

export type ClassValue = string | false | null | undefined;

/** Joins class names, dropping falsy ones, with later utilities winning. */
export function cn(...classes: ClassValue[]): string {
  return merge(classes.filter(Boolean).join(" "));
}
