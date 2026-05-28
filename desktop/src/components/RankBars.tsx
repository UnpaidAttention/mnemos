import type { Explain } from "../api/types";

function Bar({ label, rank }: { label: string; rank: number | null }) {
  const present = rank != null;
  return (
    <span
      className="label flex items-center gap-1"
      title={present ? `rank ${rank}` : "not matched by this retriever"}
    >
      <span
        className="h-2 w-2 rounded-full"
        style={{
          background: present ? "var(--accent)" : "var(--border)",
        }}
      />
      {label}
      {present ? ` #${rank}` : ""}
    </span>
  );
}

export function RankBars({ explain }: { explain: Explain | null }) {
  if (!explain) return null;
  return (
    <div className="flex flex-wrap gap-3">
      <Bar label="BM25" rank={explain.bm25_rank} />
      <Bar label="Dense" rank={explain.dense_rank} />
      <Bar label="PPR" rank={explain.ppr_rank} />
      <span className="label mono">score {explain.final_score.toFixed(3)}</span>
    </div>
  );
}
