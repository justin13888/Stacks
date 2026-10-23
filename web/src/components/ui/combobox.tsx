import { Search } from "lucide-react";
import {
  type KeyboardEvent,
  type ReactNode,
  type Ref,
  useEffect,
  useId,
  useRef,
  useState,
} from "react";
import { cn } from "@/lib/cn";

export interface ComboboxItemState {
  active: boolean;
}

export interface ComboboxProps<T> {
  /** Names the text field and the listbox. */
  label: string;
  hideLabel?: boolean;
  inputValue: string;
  onInputValueChange: (value: string) => void;
  items: readonly T[];
  getKey: (item: T) => string;
  /** The option's accessible name, read by screen readers. */
  getLabel: (item: T) => string;
  renderItem: (item: T, state: ComboboxItemState) => ReactNode;
  onSelect: (item: T) => void;
  /**
   * Extra shortcuts on the active option (e.g. Alt+P). Return `true` when the
   * key was handled, so the combobox does not also act on it.
   */
  onItemKeyDown?: (event: KeyboardEvent<HTMLInputElement>, item: T) => boolean;
  placeholder?: string;
  /** Shown in place of the list when there are no items (loading, empty, error). */
  status?: ReactNode;
  inputRef?: Ref<HTMLInputElement>;
  className?: string;
}

/**
 * A text field that filters a list of options (the WAI-ARIA 1.2 combobox
 * pattern with a listbox popup). Focus stays in the field; Up and Down move the
 * active option, announced through `aria-activedescendant`, and Enter picks it.
 */
export function Combobox<T>({
  label,
  hideLabel = false,
  inputValue,
  onInputValueChange,
  items,
  getKey,
  getLabel,
  renderItem,
  onSelect,
  onItemKeyDown,
  placeholder,
  status,
  inputRef,
  className,
}: ComboboxProps<T>) {
  const id = useId();
  const listboxId = `${id}-listbox`;
  const labelId = `${id}-label`;
  const [activeIndex, setActiveIndex] = useState(0);
  const listRef = useRef<HTMLDivElement>(null);

  // A new result set starts at its first option (state adjusted during
  // render, as React recommends, rather than in an effect).
  const itemKeys = items.map(getKey).join("\u0000");
  const [seenKeys, setSeenKeys] = useState(itemKeys);
  if (seenKeys !== itemKeys) {
    setSeenKeys(itemKeys);
    setActiveIndex(0);
  }

  const active =
    items.length > 0 ? Math.min(activeIndex, items.length - 1) : -1;
  const activeItem = active >= 0 ? items[active] : undefined;
  const optionId = (index: number) => `${id}-option-${index}`;

  useEffect(() => {
    if (active < 0) return;
    const element = listRef.current?.querySelector<HTMLElement>(
      `[data-index="${active}"]`,
    );
    element?.scrollIntoView?.({ block: "nearest" });
  }, [active]);

  function onKeyDown(event: KeyboardEvent<HTMLInputElement>) {
    if (activeItem !== undefined && onItemKeyDown?.(event, activeItem)) {
      event.preventDefault();
      return;
    }
    switch (event.key) {
      case "ArrowDown":
        if (items.length === 0) return;
        event.preventDefault();
        setActiveIndex((active + 1) % items.length);
        break;
      case "ArrowUp":
        if (items.length === 0) return;
        event.preventDefault();
        setActiveIndex((active - 1 + items.length) % items.length);
        break;
      case "Enter":
        if (activeItem === undefined) return;
        event.preventDefault();
        onSelect(activeItem);
        break;
    }
  }

  const expanded = items.length > 0;

  return (
    <div className={cn("flex min-h-0 flex-col", className)}>
      <label
        id={labelId}
        htmlFor={`${id}-input`}
        className={cn("text-sm font-medium text-fg", hideLabel && "sr-only")}
      >
        {label}
      </label>
      <div className="relative flex items-center">
        <Search
          aria-hidden
          className="pointer-events-none absolute left-4 size-5 text-fg-subtle"
        />
        <input
          ref={inputRef}
          id={`${id}-input`}
          type="text"
          role="combobox"
          aria-expanded={expanded}
          aria-controls={listboxId}
          aria-autocomplete="list"
          aria-activedescendant={active >= 0 ? optionId(active) : undefined}
          autoComplete="off"
          autoCorrect="off"
          spellCheck={false}
          placeholder={placeholder}
          value={inputValue}
          onChange={(event) => onInputValueChange(event.target.value)}
          onKeyDown={onKeyDown}
          className={cn(
            "h-14 w-full bg-transparent pr-4 pl-12 text-base text-fg outline-none",
            "placeholder:text-fg-subtle",
          )}
        />
      </div>
      <div
        ref={listRef}
        id={listboxId}
        role="listbox"
        aria-labelledby={labelId}
        className="min-h-0 overflow-y-auto overscroll-contain"
      >
        {items.map((item, index) => {
          const isActive = index === active;
          return (
            <div
              key={getKey(item)}
              id={optionId(index)}
              role="option"
              // Options are reached through aria-activedescendant, never Tab.
              tabIndex={-1}
              aria-selected={isActive}
              aria-label={getLabel(item)}
              data-index={index}
              data-active={isActive || undefined}
              // Keep focus in the field; the click still picks the option.
              onMouseDown={(event) => event.preventDefault()}
              onMouseMove={() => {
                if (!isActive) setActiveIndex(index);
              }}
              onClick={() => onSelect(item)}
              onKeyDown={(event) => {
                if (event.key === "Enter") onSelect(item);
              }}
              className="cursor-pointer outline-none"
            >
              {renderItem(item, { active: isActive })}
            </div>
          );
        })}
      </div>
      {!expanded && status}
    </div>
  );
}
