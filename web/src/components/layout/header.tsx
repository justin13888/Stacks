interface HeaderProps {
  onSearchClick: () => void;
}

export default function Header({ onSearchClick }: HeaderProps) {
  return (
    <header data-testid="header" className="header">
      {/* Header stub */}
      <div className="header-brand">Stacks</div>
      <button
        type="button"
        className="header-search-btn"
        onClick={onSearchClick}
        aria-label="Open search"
      >
        Search
      </button>
    </header>
  );
}
