import { useRef, useState } from "react";

interface ImportFormProps {
  onSubmit: (file: File, format: string, conflict: string) => void;
}

const FORMATS = ["AniList", "MyAnimeList", "Trakt", "Letterboxd"] as const;
const CONFLICT_STRATEGIES = ["skip", "overwrite", "merge"] as const;

export default function ImportForm({ onSubmit }: ImportFormProps) {
  const fileRef = useRef<HTMLInputElement>(null);
  const [format, setFormat] = useState<string>(FORMATS[0]);
  const [conflict, setConflict] = useState<string>(CONFLICT_STRATEGIES[0]);

  const handleSubmit = (e: React.FormEvent) => {
    e.preventDefault();
    const file = fileRef.current?.files?.[0];
    if (!file) return;
    onSubmit(file, format, conflict);
  };

  return (
    <form
      data-testid="import-form"
      className="import-form"
      onSubmit={handleSubmit}
    >
      {/* Import form stub */}
      <label htmlFor="import-file">File</label>
      <input id="import-file" type="file" ref={fileRef} required />

      <label htmlFor="import-format">Format</label>
      <select
        id="import-format"
        value={format}
        onChange={(e) => setFormat(e.target.value)}
      >
        {FORMATS.map((f) => (
          <option key={f} value={f}>
            {f}
          </option>
        ))}
      </select>

      <label htmlFor="import-conflict">On Conflict</label>
      <select
        id="import-conflict"
        value={conflict}
        onChange={(e) => setConflict(e.target.value)}
      >
        {CONFLICT_STRATEGIES.map((s) => (
          <option key={s} value={s}>
            {s}
          </option>
        ))}
      </select>

      <button type="submit">Import</button>
    </form>
  );
}
