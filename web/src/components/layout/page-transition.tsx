import { motion } from "motion/react";
import { pageVariants, pageTransition } from "@/lib/animation";

interface PageTransitionProps {
  children: React.ReactNode;
}

export default function PageTransition({ children }: PageTransitionProps) {
  return (
    <motion.div
      data-testid="page-transition"
      className="page-transition"
      variants={pageVariants}
      initial="initial"
      animate="animate"
      exit="exit"
      transition={pageTransition}
    >
      {/* Page transition wrapper */}
      {children}
    </motion.div>
  );
}
