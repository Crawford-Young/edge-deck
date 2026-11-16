import { useState } from 'react'
import { focusTerminal } from '../api'
import { age } from './format'
import { Panel } from './Panel'
import type { Wait } from './types'
import { useNow } from './useNow'

const UNKNOWN = '—'

const LAST = Number.MAX_SAFE_INTEGER

/** Longest wait first; a wait with no start time last. */
function longestFirst(a: Wait, b: Wait): number {
  return (a.sinceMs ?? LAST) - (b.sinceMs ?? LAST)
}

const ROW =
  'flex min-h-24 w-full flex-col justify-center gap-2 rounded-lg border border-border bg-background p-4 text-left focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring'

function Row({ wait, now }: { wait: Wait; now: number }) {
  return (
    <>
      <div className="flex items-baseline justify-between gap-3">
        <span className="text-[26px] font-semibold">{wait.label}</span>
        <span className="font-mono text-2xl font-semibold tabular-nums text-warning">
          {wait.sinceMs === null ? UNKNOWN : age(now - wait.sinceMs)}
        </span>
      </div>
      <span className="text-[22px] text-muted-foreground">{wait.waitingFor ?? UNKNOWN}</span>
    </>
  )
}

export function NeedsYouPanel({ waits }: { waits: readonly Wait[] }) {
  const now = useNow()
  const [failed, setFailed] = useState(false)
  const sorted = [...waits].sort(longestFirst)

  const tap = (focus: NonNullable<Wait['focus']>) => {
    focusTerminal(focus).then(
      () => setFailed(false),
      () => setFailed(true),
    )
  }

  return (
    <Panel title="Needs you" count={<span>{waits.length}</span>}>
      {sorted.length === 0 ? (
        <p className="text-xl text-muted-foreground">Nothing needs you</p>
      ) : (
        <ul className="flex flex-col gap-3">
          {sorted.map((w, i) => {
            const focus = w.focus
            return (
              <li key={`${i}-${w.label}`}>
                {focus === null ? (
                  <div className={ROW}>
                    <Row wait={w} now={now} />
                  </div>
                ) : (
                  <button type="button" className={ROW} onClick={() => tap(focus)}>
                    <Row wait={w} now={now} />
                  </button>
                )}
              </li>
            )
          })}
        </ul>
      )}
      {failed && (
        <p role="alert" className="mt-3 text-xl text-destructive">
          Could not bring that terminal forward
        </p>
      )}
    </Panel>
  )
}
