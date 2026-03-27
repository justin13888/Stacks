interface SelectOption {
  value: string;
  label: string;
}

interface SelectProps {
  value: string;
  onChange: (v: string) => void;
  options: SelectOption[];
  className?: string;
}

export default function Select({
  value,
  onChange,
  options,
  className,
}: SelectProps) {
  return (
    <select
      data-testid="select"
      value={value}
      onChange={(e) => onChange(e.target.value)}
      className={className}
    >
      {/* Select stub */}
      {options.map((opt) => (
        <option key={opt.value} value={opt.value}>
          {opt.label}
        </option>
      ))}
    </select>
  );
}
