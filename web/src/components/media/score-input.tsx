import { motion, AnimatePresence } from "motion/react";
import type { ScoreFormat } from "@/lib/score-format";
import { formatScore, maxScore } from "@/lib/score-format";

interface ScoreInputProps {
  value: number | null;
  onChange: (score: number) => void;
  format: ScoreFormat;
}

export default function ScoreInput({
  value,
  onChange,
  format,
}: ScoreInputProps) {
  const displayed = value !== null ? formatScore(value, format) : null;
  const max = maxScore(format);

  return (
    <div data-testid="score-input" className="score-input">
      {/* Inline editable score — number morph animation on change */}
      <AnimatePresence mode="popLayout">
        <motion.span
          key={String(displayed)}
          className="score-input-value"
          initial={{ opacity: 0, y: -4 }}
          animate={{ opacity: 1, y: 0 }}
          exit={{ opacity: 0, y: 4 }}
          transition={{ duration: 0.15 }}
        >
          {displayed ?? "—"}
        </motion.span>
      </AnimatePresence>
      <span className="score-input-max">/ {max}</span>
      <input
        type="number"
        className="score-input-hidden"
        min={0}
        max={max}
        value={displayed ?? ""}
        onChange={(e) => onChange(Number(e.target.value))}
        aria-label="Score"
      />
    </div>
  );
}
