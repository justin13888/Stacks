interface ButtonProps {
  children: React.ReactNode;
  onClick?: () => void;
  variant?: "primary" | "secondary" | "ghost";
  disabled?: boolean;
  className?: string;
}

export default function Button({
  children,
  onClick,
  variant = "primary",
  disabled = false,
  className,
}: ButtonProps) {
  return (
    <button
      type="button"
      data-testid="button"
      data-variant={variant}
      onClick={onClick}
      disabled={disabled}
      className={className}
    >
      {/* Button stub */}
      {children}
    </button>
  );
}
