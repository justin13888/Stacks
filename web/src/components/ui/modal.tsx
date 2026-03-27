import { AnimatePresence, motion } from "motion/react";
import { modalVariants } from "@/lib/animation";

interface ModalProps {
  open: boolean;
  onClose: () => void;
  children: React.ReactNode;
  title?: string;
}

export default function Modal({ open, onClose, children, title }: ModalProps) {
  return (
    <AnimatePresence>
      {open && (
        // biome-ignore lint/a11y/noStaticElementInteractions: overlay dismisses modal on click
        // biome-ignore lint/a11y/useKeyWithClickEvents: overlay is a visual affordance, not keyboard-navigable
        <div data-testid="modal" className="modal-overlay" onClick={onClose}>
          <motion.div
            className="modal-content"
            variants={modalVariants}
            initial="initial"
            animate="animate"
            exit="exit"
            onClick={(e) => e.stopPropagation()}
          >
            {/* Modal stub */}
            {title && <h2 className="modal-title">{title}</h2>}
            {children}
          </motion.div>
        </div>
      )}
    </AnimatePresence>
  );
}
