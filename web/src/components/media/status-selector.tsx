const STATUS_OPTIONS = [
  { value: "watching", label: "Watching" },
  { value: "completed", label: "Completed" },
  { value: "on_hold", label: "On Hold" },
  { value: "dropped", label: "Dropped" },
  { value: "plan_to_watch", label: "Plan to Watch" },
  { value: "rewatching", label: "Rewatching" },
] as const;

interface StatusSelectorProps {
  value: string | null;
  onChange: (status: string) => void;
}

export default function StatusSelector({
  value,
  onChange,
}: StatusSelectorProps) {
  return (
    <div data-testid="status-selector" className="status-selector">
      <select
        value={value ?? ""}
        onChange={(e) => onChange(e.target.value)}
        className="status-selector-select"
      >
        <option value="">— No Status —</option>
        {STATUS_OPTIONS.map((opt) => (
          <option key={opt.value} value={opt.value}>
            {opt.label}
          </option>
        ))}
      </select>
    </div>
  );
}
