import { useRef, useState } from "react";
import { client } from "../api/client";
import { Button, Card } from "../design/primitives";

export function VaultIO() {
  const fileRef = useRef<HTMLInputElement | null>(null);
  const [busy, setBusy] = useState<"export" | "import" | null>(null);
  const [msg, setMsg] = useState<string | null>(null);

  const onExport = async () => {
    setBusy("export");
    try {
      const blob = await client.vaultExport();
      const url = URL.createObjectURL(blob);
      const a = document.createElement("a");
      a.href = url;
      a.download = "mnemos-vault.zip";
      a.click();
      URL.revokeObjectURL(url);
      setMsg("Export downloaded.");
    } finally {
      setBusy(null);
    }
  };

  const onImport = async (file: File) => {
    setBusy("import");
    try {
      const r = await client.vaultImport(file);
      setMsg(`Imported ${r.files_imported} files.`);
    } finally {
      setBusy(null);
    }
  };

  return (
    <Card className="p-4 space-y-3">
      <div>
        <div className="display text-base">Vault</div>
        <p className="label text-text-muted">
          Back up or restore the entire vault as a zip. The database is rebuilt from files on import.
        </p>
      </div>
      <div className="flex items-center gap-2">
        <Button variant="ghost" onClick={onExport} disabled={busy !== null}>
          {busy === "export" ? "Exporting…" : "Export…"}
        </Button>
        <Button variant="ghost" onClick={() => fileRef.current?.click()} disabled={busy !== null}>
          {busy === "import" ? "Importing…" : "Import…"}
        </Button>
        <input
          ref={fileRef}
          type="file"
          accept=".zip,application/zip"
          className="hidden"
          onChange={(e) => {
            const f = e.target.files?.[0];
            if (f) void onImport(f);
          }}
        />
        {msg && <span className="label text-text-muted">{msg}</span>}
      </div>
    </Card>
  );
}
