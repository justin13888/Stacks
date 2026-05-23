import { useRef, useState } from "react";

interface ImportFormProps {
  onSubmit: (file: File, source: string, conflict: string) => void;
}

const SOURCES = [
  { value: "mal_xml", label: "MyAnimeList (XML)" },
  { value: "mal_json", label: "MyAnimeList (JSON)" },
  { value: "tmdb_json", label: "TMDB / Stacks (JSON)" },
] as const;
const CONFLICT_STRATEGIES = ["skip", "overwrite", "merge"] as const;

export default function ImportForm({ onSubmit }: ImportFormProps) {
  const fileRef = useRef<HTMLInputElement>(null);
  const [source, setSource] = useState<string>(SOURCES[0].value);
  const [conflict, setConflict] = useState<string>(CONFLICT_STRATEGIES[0]);

  const handleSubmit = (e: React.FormEvent) => {
    e.preventDefault();
    const file = fileRef.current?.files?.[0];
    if (!file) return;
    onSubmit(file, source, conflict);
  };

  return (
    <form
      data-testid="import-form"
      className="import-form"
      onSubmit={handleSubmit}
    >
      <label htmlFor="import-file">File</label>
      <input id="import-file" type="file" ref={fileRef} required />

      <label htmlFor="import-format">Format</label>
      <select
        id="import-format"
        value={source}
        onChange={(e) => setSource(e.target.value)}
      >
        {SOURCES.map((s) => (
          <option key={s.value} value={s.value}>
            {s.label}
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
