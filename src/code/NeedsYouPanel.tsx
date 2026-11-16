import { age } from './format'
import { Panel } from './Panel'
import type { Wait } from './types'
import { useNow } from './useNow'

export function NeedsYouPanel({ waits }: { waits: readonly Wait[] }) {
  const now = useNow()
  const sorted = [...waits].sort((a, b) => a.since - b.since)
  return (
    <Panel title="Needs you" count={<span>{waits.length}</span>}>
      {sorted.length === 0 ? (
        <p className="text-xl text-muted-foreground">Nothing needs you</p>
      ) : (
        <ul className="flex flex-col gap-3">
          {sorted.map((w) => (
            <li
              key={w.id}
              className="flex min-h-24 flex-col justify-center gap-2 rounded-lg border border-border bg-background p-4"
            >
              <div className="flex items-baseline justify-between gap-3">
                <span className="text-[26px] font-semibold">{w.label}</span>
                <span className="font-mono text-2xl font-semibold tabular-nums text-warning">
                  {age(now - w.since)}
                </span>
              </div>
              <span className="text-[22px] text-muted-foreground">{w.waitsFor}</span>
            </li>
          ))}
        </ul>
      )}
    </Panel>
  )
}
