// Motion preferences and shared transitions.
//
// `usePrefersReducedMotion` follows the OS `prefers-reduced-motion` setting
// live. The app shell feeds it to <MotionConfig>, which turns every `motion`
// transform and layout animation into an instant change; global.css does the
// same for CSS transitions. Opacity fades stay, as they carry no motion.

import type { Transition } from "motion/react";
import { useSyncExternalStore } from "react";

const QUERY = "(prefers-reduced-motion: reduce)";

function mediaQuery(): MediaQueryList | null {
  if (typeof window === "undefined" || !window.matchMedia) return null;
  return window.matchMedia(QUERY);
}

function subscribe(onChange: () => void): () => void {
  const query = mediaQuery();
  query?.addEventListener("change", onChange);
  return () => query?.removeEventListener("change", onChange);
}

export function prefersReducedMotion(): boolean {
  return mediaQuery()?.matches ?? false;
}

export function usePrefersReducedMotion(): boolean {
  return useSyncExternalStore(subscribe, prefersReducedMotion, () => false);
}

/** The standard entrance: a short ease-out. */
export const enterTransition: Transition = {
  duration: 0.22,
  ease: [0.22, 1, 0.36, 1],
};
