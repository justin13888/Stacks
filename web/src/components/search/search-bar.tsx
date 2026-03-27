import { motion } from "motion/react";
import { useState } from "react";

interface SearchBarProps {
  value: string;
  onChange: (v: string) => void;
  onSubmit: () => void;
}

export default function SearchBar({
  value,
  onChange,
  onSubmit,
}: SearchBarProps) {
  const [focused, setFocused] = useState(false);

  const handleKeyDown = (e: React.KeyboardEvent<HTMLInputElement>) => {
    if (e.key === "Enter") onSubmit();
  };

  return (
    <motion.div
      data-testid="search-bar"
      className="search-bar"
      animate={{ width: focused ? "100%" : "60%" }}
      transition={{ duration: 0.25, ease: "easeOut" }}
    >
      {/* Search bar stub — expands on focus */}
      <input
        type="search"
        className="search-bar-input"
        value={value}
        onChange={(e) => onChange(e.target.value)}
        onFocus={() => setFocused(true)}
        onBlur={() => setFocused(false)}
        onKeyDown={handleKeyDown}
        placeholder="Search…"
        aria-label="Search"
      />
    </motion.div>
  );
}
