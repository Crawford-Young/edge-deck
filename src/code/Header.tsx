import { Badge } from '@crawfordyoung/ui'
import type { ReactNode } from 'react'
import { runningTime } from './format'
import type { HeaderData } from './types'
import { useNow } from './useNow'

const UNKNOWN = '—'

function Stat({ label, children }: { label: string; children: ReactNode }) {
  return (
    <div className="flex items-baseline gap-2.5">
      <span className="text-muted-foreground">{label}</span>
      <span className="font-mono font-semibold tabular-nums">{children}</span>
    </div>
  )
}

export function Header({ data }: { data: HeaderData | null }) {
  const now = useNow()
  return (
    <header className="flex h-14 shrink-0 items-center gap-10 rounded-xl border border-border bg-surface px-5 text-[22px]">
      {data === null ? (
        <span className="text-muted-foreground">No live session</span>
      ) : (
        <>
          <Stat label="Running">
            {data.startedAtMs === null ? UNKNOWN : runningTime(now - data.startedAtMs)}
          </Stat>
          <Stat label="This session">
            {data.sessionCostUsd === null ? UNKNOWN : `$${data.sessionCostUsd.toFixed(2)}`}
          </Stat>
          <Stat label="Today">{`$${data.todayUsd.toFixed(2)}`}</Stat>
          <Stat label="Live">{`${data.liveAgents} ${data.liveAgents === 1 ? 'agent' : 'agents'}`}</Stat>
          <div className="flex-1" />
          {data.needYou > 0 && (
            <Badge variant="warning" className="h-9 px-4 text-xl">
              {`${data.needYou} need you`}
            </Badge>
          )}
        </>
      )}
    </header>
  )
}
