interface SmartListBuilderProps {
  value: object | null;
  onChange: (filter: object) => void;
}

export default function SmartListBuilder({
  value,
  onChange,
}: SmartListBuilderProps) {
  return (
    <div data-testid="smart-list-builder" className="smart-list-builder">
      {/* Smart list filter builder stub */}
      <p className="smart-list-builder-placeholder">
        Filter builder — {value ? "filter configured" : "no filter set"}
      </p>
      <button type="button" onClick={() => onChange({ genre: "Action" })}>
        Add Genre Filter (stub)
      </button>
    </div>
  );
}
