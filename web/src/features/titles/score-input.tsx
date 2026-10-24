import { Star, X } from "lucide-react";
import { type KeyboardEvent, useId, useState } from "react";
import { IconButton } from "@/components/ui/button";
import { inputClass } from "@/components/ui/input";
import { cn } from "@/lib/cn";
import {
  fromDisplayScore,
  maxScore,
  type ScoreFormat,
  toDisplayScore,
} from "@/lib/score-format";

export interface ScoreInputProps {
  /** The stored 0–100 score, or `null` when unscored. */
  score: number | null | undefined;
  format: ScoreFormat;
  /** Called with the new stored 0–100 score, or `null` to clear it. */
  onChange: (score: number | null) => void;
  disabled?: boolean;
}

const POINT_3 = [
  { value: 1, label: "Disliked", emoji: "🙁" },
  { value: 2, label: "Okay", emoji: "😐" },
  { value: 3, label: "Liked", emoji: "🙂" },
] as const;

/**
 * Score entry in the user's format: stars for 5-point, three faces for
 * 3-point, a number otherwise. It shows and takes the format's units and
 * reports the stored 0–100 value.
 */
export function ScoreInput({
  score,
  format,
  onChange,
  disabled,
}: ScoreInputProps) {
  const shown = score == null ? null : toDisplayScore(score, format);
  if (format === "point_5" || format === "point_3") {
    return (
      <ChoiceScore
        format={format}
        shown={shown}
        disabled={disabled}
        onPick={(value) =>
          onChange(value === null ? null : fromDisplayScore(value, format))
        }
      />
    );
  }
  return (
    <NumberScore
      // A new stored score (another device, a rollback) resets the field.
      key={String(score)}
      format={format}
      shown={shown}
      disabled={disabled}
      onCommit={(value) =>
        onChange(value === null ? null : fromDisplayScore(value, format))
      }
    />
  );
}

function NumberScore({
  format,
  shown,
  disabled,
  onCommit,
}: {
  format: ScoreFormat;
  shown: number | null;
  disabled?: boolean;
  onCommit: (value: number | null) => void;
}) {
  const max = maxScore(format);
  const step = format === "point_10_decimal" ? 0.1 : 1;
  const [text, setText] = useState(shown === null ? "" : String(shown));
  const [invalid, setInvalid] = useState(false);

  function commit() {
    const trimmed = text.trim();
    if (trimmed === "") {
      setInvalid(false);
      if (shown !== null) onCommit(null);
      return;
    }
    const value = Number(trimmed);
    if (!Number.isFinite(value) || value < 0 || value > max) {
      setInvalid(true);
      return;
    }
    setInvalid(false);
    const rounded =
      step === 1 ? Math.round(value) : Math.round(value * 10) / 10;
    if (rounded !== shown) onCommit(rounded);
  }

  function onKeyDown(event: KeyboardEvent<HTMLInputElement>) {
    if (event.key === "Enter") commit();
  }

  return (
    <div className="flex flex-col gap-1.5">
      <label className="flex items-center gap-2 text-sm font-medium text-fg">
        <span>Score</span>
        <span className="flex items-center gap-1.5">
          <input
            type="number"
            inputMode="decimal"
            min={0}
            max={max}
            step={step}
            value={text}
            disabled={disabled}
            aria-invalid={invalid || undefined}
            onChange={(event) => setText(event.target.value)}
            onBlur={commit}
            onKeyDown={onKeyDown}
            placeholder="–"
            className={cn(inputClass, "w-20 tabular-nums")}
          />
          <span className="font-normal text-fg-muted">/ {max}</span>
        </span>
      </label>
      {invalid && (
        <p role="alert" className="text-xs text-danger">
          Enter a score from 0 to {max}.
        </p>
      )}
    </div>
  );
}

function ChoiceScore({
  format,
  shown,
  disabled,
  onPick,
}: {
  format: "point_5" | "point_3";
  shown: number | null;
  disabled?: boolean;
  onPick: (value: number | null) => void;
}) {
  const choices =
    format === "point_5"
      ? [1, 2, 3, 4, 5].map((value) => ({
          value,
          label: `${value} star${value === 1 ? "" : "s"}`,
        }))
      : POINT_3.map(({ value, label }) => ({ value, label }));
  const name = useId();
  return (
    <div className="flex items-center gap-2">
      <fieldset className="flex items-center gap-2">
        <legend className="float-left mr-2 text-sm font-medium text-fg">
          Score
        </legend>
        <div className="flex items-center gap-0.5">
          {choices.map(({ value, label }) => {
            const checked = shown === value;
            const lit = shown !== null && value <= shown;
            return (
              <label
                key={value}
                className={cn(
                  "inline-flex size-9 cursor-pointer items-center justify-center rounded-md text-lg",
                  "transition-transform duration-(--duration-fast) hover:scale-110",
                  "has-focus-visible:outline-2 has-focus-visible:outline-ring",
                  format === "point_3" &&
                    (checked ? "bg-accent-soft" : "opacity-60"),
                )}
              >
                <input
                  type="radio"
                  name={name}
                  value={value}
                  checked={checked}
                  disabled={disabled}
                  onChange={() => onPick(value)}
                  className="sr-only"
                />
                <span className="sr-only">{label}</span>
                {format === "point_5" ? (
                  <Star
                    aria-hidden
                    className={cn(
                      "size-5",
                      lit ? "fill-warning text-warning" : "text-line-strong",
                    )}
                  />
                ) : (
                  <span aria-hidden>{POINT_3[value - 1]?.emoji}</span>
                )}
              </label>
            );
          })}
        </div>
      </fieldset>
      {shown !== null && (
        <IconButton
          label="Clear score"
          size="sm"
          icon={<X aria-hidden className="size-4" />}
          disabled={disabled}
          onClick={() => onPick(null)}
        />
      )}
    </div>
  );
}
