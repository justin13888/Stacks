import { motion } from "motion/react";
import type { ReactNode } from "react";
import { enterTransition, usePrefersReducedMotion } from "@/lib/motion";

/**
 * Fades and lifts a page in when the route changes. Under
 * prefers-reduced-motion the page appears at once, with no animation at all.
 */
export function PageTransition({
  routeKey,
  children,
}: {
  /** Changes per page, so each navigation animates in. */
  routeKey: string;
  children: ReactNode;
}) {
  const reduced = usePrefersReducedMotion();
  if (reduced) {
    return <div data-motion="off">{children}</div>;
  }
  return (
    <motion.div
      key={routeKey}
      data-motion="on"
      initial={{ opacity: 0, y: 8 }}
      animate={{ opacity: 1, y: 0 }}
      transition={enterTransition}
    >
      {children}
    </motion.div>
  );
}
