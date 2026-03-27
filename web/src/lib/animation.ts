/**
 * Shared motion variants and transition configs.
 * Used by Framer Motion (motion) components throughout the app.
 */
import type { Transition, Variants } from "motion/react";

/** Page transition: crossfade + subtle scale. */
export const pageVariants: Variants = {
  initial: { opacity: 0, scale: 1.0 },
  animate: { opacity: 1, scale: 1.0 },
  exit: { opacity: 0, scale: 0.98 },
};

export const pageTransition: Transition = {
  duration: 0.2,
  ease: "easeOut",
};

/** Modal: scale from 0.95 + fade. */
export const modalVariants: Variants = {
  initial: { opacity: 0, scale: 0.95 },
  animate: {
    opacity: 1,
    scale: 1,
    transition: { type: "spring", stiffness: 80, damping: 10 },
  },
  exit: { opacity: 0, scale: 0.95, transition: { duration: 0.15 } },
};

/** Dropdown: scale from 0.95 + fade with spring overshoot. */
export const dropdownVariants: Variants = {
  initial: { opacity: 0, scale: 0.95, y: -4 },
  animate: {
    opacity: 1,
    scale: 1,
    y: 0,
    transition: { type: "spring", stiffness: 80, damping: 10 },
  },
  exit: { opacity: 0, scale: 0.95, y: -4, transition: { duration: 0.15 } },
};

/** Toast: slide in from top-right. */
export const toastVariants: Variants = {
  initial: { opacity: 0, x: 60, y: -8 },
  animate: {
    opacity: 1,
    x: 0,
    y: 0,
    transition: { duration: 0.3, ease: "easeOut" },
  },
  exit: { opacity: 0, x: 60, transition: { duration: 0.2 } },
};

/** Poster card hover: scale + shadow. */
export const posterCardHover = {
  scale: 1.05,
  transition: { duration: 0.15, ease: "easeOut" },
};

/** Staggered search results: each card delays 30ms. */
export function staggeredFadeIn(index: number): Variants {
  return {
    initial: { opacity: 0, y: 8 },
    animate: {
      opacity: 1,
      y: 0,
      transition: { duration: 0.2, delay: index * 0.03 },
    },
  };
}
