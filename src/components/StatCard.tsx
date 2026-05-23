import type { ReactNode } from "react";

interface StatCardProps {
  label: string;
  value: string;
  hint?: string;
  icon?: ReactNode;
}

export function StatCard({ label, value, hint, icon }: StatCardProps) {
  return (
    <section className="rounded-lg border border-line bg-white/80 p-4 shadow-panel">
      <div className="flex items-start justify-between gap-4">
        <div>
          <p className="text-xs font-semibold uppercase tracking-[0.16em] text-ink/50">{label}</p>
          <p className="mt-3 text-3xl font-semibold text-ink">{value}</p>
          {hint ? <p className="mt-2 text-sm text-ink/60">{hint}</p> : null}
        </div>
        {icon ? <div className="rounded-md border border-line bg-paper p-2 text-moss">{icon}</div> : null}
      </div>
    </section>
  );
}
