import { useMemo, useState } from "react";
import { scaleTime } from "@visx/scale";
import { useQuery } from "@tanstack/react-query";
import { client } from "../api/client";
import { useUiStore } from "../store/ui";
import { TIER_COLOR_VAR } from "../design/theme";
import { Skeleton } from "../design/primitives";

// SVG layout constants
const W = 900;
const ROW = 28;
const LABEL_PAD = 130;
const TOP_PAD = 16;
const AXIS_H = 32;

export function Timeline() {
  const { data, isLoading, isError } = useQuery({
    queryKey: ["timeline"],
    queryFn: () => client.listMemories({ include_invalid: true, limit: 200 }),
  });
  const setAsOf = useUiStore((s) => s.setAsOf);
  const select = useUiStore((s) => s.select);
  const [cursor, setCursor] = useState<number>(Date.now());

  const { scale, height, rangeMin, rangeMax } = useMemo(() => {
    const mems = data ?? [];
    const times = mems.flatMap((m) => [
      new Date(m.valid_at).getTime(),
      m.invalid_at ? new Date(m.invalid_at).getTime() : Date.now(),
    ]);
    const min = times.length ? Math.min(...times) : Date.now() - 86_400_000;
    const max = times.length ? Math.max(...times, Date.now()) : Date.now();
    const sc = scaleTime({ domain: [new Date(min), new Date(max)], range: [LABEL_PAD, W - 20] });
    const svgH = Math.max(120, mems.length * ROW + TOP_PAD + AXIS_H);
    return { scale: sc, height: svgH, rangeMin: min, rangeMax: max };
  }, [data]);

  if (isLoading) {
    return (
      <div className="p-6">
        <Skeleton className="h-8 w-48 mb-4" />
        <Skeleton className="h-64 w-full" />
      </div>
    );
  }

  if (isError) {
    return (
      <div className="p-6 text-tier-procedural">
        Could not load memories. Is the daemon running?
      </div>
    );
  }

  const mems = data ?? [];

  if (!mems.length) {
    return (
      <div className="p-6">
        <h1 className="display text-xl mb-3">Timeline</h1>
        <p className="text-text-muted">
          No memories yet. Add memories to see their bi-temporal extent here.
        </p>
      </div>
    );
  }

  const cursorX = scale(new Date(cursor));

  // Build a few readable axis ticks
  const tickCount = 5;
  const ticks: Date[] = [];
  for (let i = 0; i <= tickCount; i++) {
    ticks.push(new Date(rangeMin + ((rangeMax - rangeMin) * i) / tickCount));
  }

  return (
    <div className="p-6 space-y-4 overflow-x-auto">
      <h1 className="display text-xl">Timeline</h1>
      <p className="label text-text-muted">
        Drag the cursor to time-travel — the top bar shows the active date.
      </p>

      <div className="overflow-x-auto">
        <svg
          width={W}
          height={height}
          role="img"
          aria-label="bi-temporal memory timeline"
          className="block"
        >
          {/* Axis ticks */}
          {ticks.map((t) => {
            const x = scale(t);
            return (
              <g key={t.getTime()}>
                <line x1={x} x2={x} y1={TOP_PAD} y2={height - AXIS_H + 4} stroke="var(--border)" strokeWidth={1} />
                <text
                  x={x}
                  y={height - 6}
                  fontSize={10}
                  textAnchor="middle"
                  fill="var(--text-muted)"
                  className="mono"
                >
                  {t.toISOString().slice(0, 10)}
                </text>
              </g>
            );
          })}

          {/* Memory bars */}
          {mems.map((m, i) => {
            const x1 = scale(new Date(m.valid_at));
            const rawX2 = m.invalid_at
              ? scale(new Date(m.invalid_at))
              : scale(new Date(rangeMax));
            const x2 = Math.max(x1 + 2, rawX2);
            const y = TOP_PAD + i * ROW;
            const invalid = !!m.invalid_at;
            return (
              <g key={m.id} onClick={() => select(m.id)} style={{ cursor: "pointer" }}>
                {/* Label */}
                <text
                  x={4}
                  y={y + ROW / 2 + 4}
                  fontSize={11}
                  fill="var(--text-muted)"
                  className="mono"
                >
                  {m.title.slice(0, 16)}
                </text>
                {/* Bar */}
                <rect
                  x={x1}
                  y={y + 4}
                  width={x2 - x1}
                  height={ROW - 10}
                  rx={3}
                  fill={TIER_COLOR_VAR[m.tier]}
                  opacity={invalid ? 0.35 : 0.82}
                  strokeDasharray={invalid ? "4 3" : undefined}
                  stroke={invalid ? "var(--text-muted)" : "none"}
                  strokeWidth={invalid ? 1 : 0}
                />
                {/* Hover target */}
                <title>{m.title}{invalid ? " (invalidated)" : ""} · {m.tier}</title>
              </g>
            );
          })}

          {/* Time-travel cursor */}
          <line
            x1={cursorX}
            x2={cursorX}
            y1={TOP_PAD}
            y2={height - AXIS_H}
            stroke="var(--accent)"
            strokeWidth={2}
          />
          <circle cx={cursorX} cy={TOP_PAD} r={5} fill="var(--accent)" />
        </svg>
      </div>

      {/* Draggable range slider */}
      <input
        type="range"
        min={rangeMin}
        max={rangeMax}
        value={cursor}
        step={Math.max(1, Math.floor((rangeMax - rangeMin) / 1000))}
        onChange={(e) => {
          const t = Number(e.target.value);
          setCursor(t);
          setAsOf(new Date(t).toISOString());
        }}
        className="w-full accent-accent"
        style={{ maxWidth: W }}
        aria-label="time-travel cursor"
      />
      <p className="label text-text-muted">
        Cursor: {new Date(cursor).toISOString().slice(0, 16).replace("T", " ")} UTC
      </p>
    </div>
  );
}
