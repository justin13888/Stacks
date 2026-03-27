interface InputProps {
  value: string;
  onChange: (v: string) => void;
  placeholder?: string;
  className?: string;
}

export default function Input({
  value,
  onChange,
  placeholder,
  className,
}: InputProps) {
  return (
    <input
      data-testid="input"
      value={value}
      onChange={(e) => onChange(e.target.value)}
      placeholder={placeholder}
      className={className}
    />
  );
}
