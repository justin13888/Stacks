import { motion } from "motion/react";

interface BadgeProps {
  children: React.ReactNode;
  variant?: "default" | "availability";
  className?: string;
}

export default function Badge({
  children,
  variant = "default",
  className,
}: BadgeProps) {
  if (variant === "availability") {
    return (
      <motion.div
        data-testid="badge"
        data-variant="availability"
        className={`badge badge--availability ${className ?? ""}`}
        initial={{ scale: 1 }}
        animate={{ scale: [1, 1.08, 1] }}
        transition={{ duration: 0.4, ease: "easeOut" }}
      >
        {/* Availability badge — pulses once on mount */}
        {children}
      </motion.div>
    );
  }

  return (
    <div
      data-testid="badge"
      data-variant={variant}
      className={`badge badge--${variant} ${className ?? ""}`}
    >
      {/* Badge stub */}
      {children}
    </div>
  );
}
