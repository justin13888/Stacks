import { AnimatePresence, motion } from "motion/react";
import { dropdownVariants } from "@/lib/animation";

interface DropdownProps {
  trigger: React.ReactNode;
  children: React.ReactNode;
  open: boolean;
  onClose: () => void;
}

export default function Dropdown({
  trigger,
  children,
  open,
  onClose,
}: DropdownProps) {
  return (
    <div data-testid="dropdown" className="dropdown-wrapper">
      {/* Dropdown stub */}
      <div className="dropdown-trigger">{trigger}</div>
      <AnimatePresence>
        {open && (
          <>
            {/* biome-ignore lint/a11y/noStaticElementInteractions: backdrop dismisses dropdown on click */}
            {/* biome-ignore lint/a11y/useKeyWithClickEvents: backdrop is a visual affordance, not keyboard-navigable */}
            <div className="dropdown-backdrop" onClick={onClose} />
            <motion.div
              className="dropdown-menu"
              variants={dropdownVariants}
              initial="initial"
              animate="animate"
              exit="exit"
            >
              {children}
            </motion.div>
          </>
        )}
      </AnimatePresence>
    </div>
  );
}
