import { motion } from "motion/react";
import { toastVariants } from "@/lib/animation";

interface ToastProps {
  message: string;
  type?: "success" | "error" | "info";
  onDismiss: () => void;
}

export default function Toast({
  message,
  type = "info",
  onDismiss,
}: ToastProps) {
  return (
    <motion.div
      data-testid="toast"
      data-type={type}
      className={`toast toast--${type}`}
      variants={toastVariants}
      initial="initial"
      animate="animate"
      exit="exit"
    >
      {/* Toast stub */}
      <span>{message}</span>
      <button type="button" onClick={onDismiss} aria-label="Dismiss">
        ×
      </button>
    </motion.div>
  );
}
