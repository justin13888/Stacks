import { animate } from "motion/react";
import { useEffect, useState } from "react";
import { usePrefersReducedMotion } from "@/lib/motion";

/**
 * A number that counts up to `value` when it appears or changes. With
 * reduced motion it is simply the number.
 */
export function CountUp({
  value,
  format = (n) => Math.round(n).toLocaleString(),
  durationSeconds = 0.9,
}: {
  value: number;
  format?: (n: number) => string;
  durationSeconds?: number;
}) {
  const reduced = usePrefersReducedMotion();
  const [shown, setShown] = useState(reduced ? value : 0);
  useEffect(() => {
    if (reduced) {
      setShown(value);
      return;
    }
    const controls = animate(0, value, {
      duration: durationSeconds,
      ease: [0.22, 1, 0.36, 1],
      onUpdate: setShown,
    });
    return () => controls.stop();
  }, [value, reduced, durationSeconds]);
  // Assistive technology reads the final figure, not the animation.
  return (
    <>
      <span aria-hidden>{format(shown)}</span>
      <span className="sr-only">{format(value)}</span>
    </>
  );
}
