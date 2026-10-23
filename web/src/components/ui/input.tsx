import { type InputHTMLAttributes, type Ref, useId } from "react";
import { cn } from "@/lib/cn";

export interface InputProps extends InputHTMLAttributes<HTMLInputElement> {
  /** A visible label; omit only when an `aria-label` names the field. */
  label?: string;
  /** Help text shown under the field. */
  hint?: string;
  /** An error message; marks the field invalid. */
  error?: string;
  ref?: Ref<HTMLInputElement>;
}

export const inputClass =
  "h-10 w-full rounded-md border border-line bg-surface px-3 text-sm text-fg " +
  "placeholder:text-fg-subtle shadow-raised transition-colors duration-(--duration-fast) " +
  "focus-visible:border-accent focus-visible:outline-2 focus-visible:outline-ring " +
  "disabled:cursor-not-allowed disabled:opacity-50 aria-invalid:border-danger";

export function Input({
  label,
  hint,
  error,
  id,
  className,
  ...props
}: InputProps) {
  const generated = useId();
  const inputId = id ?? generated;
  const hintId = hint ? `${inputId}-hint` : undefined;
  const errorId = error ? `${inputId}-error` : undefined;
  const describedBy = [hintId, errorId].filter(Boolean).join(" ") || undefined;

  const input = (
    <input
      id={inputId}
      aria-invalid={error ? true : undefined}
      aria-describedby={describedBy}
      className={cn(inputClass, className)}
      {...props}
    />
  );

  if (!label && !hint && !error) return input;

  return (
    <div className="flex flex-col gap-1.5">
      {label && (
        <label htmlFor={inputId} className="text-sm font-medium text-fg">
          {label}
        </label>
      )}
      {input}
      {hint && (
        <p id={hintId} className="text-xs text-fg-muted">
          {hint}
        </p>
      )}
      {error && (
        <p id={errorId} className="text-xs text-danger">
          {error}
        </p>
      )}
    </div>
  );
}
