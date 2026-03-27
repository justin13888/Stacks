import { motion } from "motion/react";

interface AvailabilityBadgeProps {
  available: boolean;
  serverName?: string;
}

export default function AvailabilityBadge({
  available,
  serverName,
}: AvailabilityBadgeProps) {
  if (!available) {
    return (
      <div
        data-testid="availability-badge"
        data-available="false"
        className="availability-badge availability-badge--unavailable"
      >
        {/* Not available */}
        Not Available
      </div>
    );
  }

  return (
    <motion.div
      data-testid="availability-badge"
      data-available="true"
      className="availability-badge availability-badge--available"
      initial={{ scale: 1 }}
      animate={{ scale: [1, 1.1, 1] }}
      transition={{ duration: 0.4, ease: "easeOut" }}
    >
      {/* Available — pulses once on mount */}
      Available{serverName ? ` on ${serverName}` : ""}
    </motion.div>
  );
}
