import { Check, ChevronDown } from "lucide-react";
import { Select as SelectPrimitive } from "radix-ui";
import { useId } from "react";
import { cn } from "@/lib/cn";

export interface SelectOption<V extends string> {
  value: V;
  label: string;
}

export interface SelectProps<V extends string> {
  label: string;
  /** Hide the label visually; it still names the field. */
  hideLabel?: boolean;
  value: V | undefined;
  onValueChange: (value: V) => void;
  options: readonly SelectOption<V>[];
  placeholder?: string;
  disabled?: boolean;
  className?: string;
}

/** A single-choice picker (Radix Select: listbox semantics, typeahead). */
export function Select<V extends string>({
  label,
  hideLabel = false,
  value,
  onValueChange,
  options,
  placeholder = "Choose…",
  disabled,
  className,
}: SelectProps<V>) {
  const labelId = useId();
  return (
    <div className={cn("flex flex-col gap-1.5", className)}>
      <span
        id={labelId}
        className={cn("text-sm font-medium text-fg", hideLabel && "sr-only")}
      >
        {label}
      </span>
      <SelectPrimitive.Root
        value={value}
        onValueChange={(next) => {
          const chosen = options.find((option) => option.value === next);
          if (chosen) onValueChange(chosen.value);
        }}
        disabled={disabled}
      >
        <SelectPrimitive.Trigger
          aria-labelledby={labelId}
          className={cn(
            "inline-flex h-10 w-full items-center justify-between gap-2 rounded-md border border-line",
            "bg-surface px-3 text-sm text-fg shadow-raised data-placeholder:text-fg-subtle",
            "focus-visible:outline-2 focus-visible:outline-ring disabled:opacity-50",
          )}
        >
          <SelectPrimitive.Value placeholder={placeholder} />
          <SelectPrimitive.Icon>
            <ChevronDown aria-hidden className="size-4 text-fg-muted" />
          </SelectPrimitive.Icon>
        </SelectPrimitive.Trigger>
        <SelectPrimitive.Portal>
          <SelectPrimitive.Content
            position="popper"
            sideOffset={6}
            className={cn(
              "z-50 max-h-72 min-w-(--radix-select-trigger-width) overflow-hidden rounded-md border border-line",
              "bg-surface p-1 text-fg shadow-overlay animate-fade-in",
            )}
          >
            <SelectPrimitive.Viewport>
              {options.map((option) => (
                <SelectPrimitive.Item
                  key={option.value}
                  value={option.value}
                  className={cn(
                    "relative flex cursor-default select-none items-center rounded-sm py-2 pr-2 pl-8 text-sm outline-none",
                    "data-highlighted:bg-accent-soft data-highlighted:text-fg",
                  )}
                >
                  <SelectPrimitive.ItemIndicator className="absolute left-2 inline-flex">
                    <Check aria-hidden className="size-4 text-accent" />
                  </SelectPrimitive.ItemIndicator>
                  <SelectPrimitive.ItemText>
                    {option.label}
                  </SelectPrimitive.ItemText>
                </SelectPrimitive.Item>
              ))}
            </SelectPrimitive.Viewport>
          </SelectPrimitive.Content>
        </SelectPrimitive.Portal>
      </SelectPrimitive.Root>
    </div>
  );
}
