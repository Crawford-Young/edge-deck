import { AgentWeb } from './graph/AgentWeb'
import { isDense, webCounts } from './graph/rules'
import { Panel } from './Panel'
import type { Island } from './types'

interface AgentsPanelProps {
  islands: readonly Island[]
  liveAgents: number
}

export function AgentsPanel({ islands, liveAgents }: AgentsPanelProps) {
  const { total, working, finished, depth } = webCounts(islands)
  const count = isDense(total)
    ? `${total} total · ${working} working · ${finished} finished · ${depth} deep`
    : String(liveAgents)
  return (
    <Panel title="Agents" count={<span>{count}</span>} scroll={false}>
      {total === 0 ? (
        <p className="text-xl text-muted-foreground">No agents running</p>
      ) : (
        <AgentWeb islands={islands} />
      )}
    </Panel>
  )
}
