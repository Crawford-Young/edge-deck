import type { ReactNode } from 'react'
import { Panel } from './Panel'

interface AgentsPanelProps {
  agentCount: number
  children?: ReactNode
}

export function AgentsPanel({ agentCount, children }: AgentsPanelProps) {
  return (
    <Panel title="Agents" count={<span>{agentCount}</span>}>
      {children ?? <p className="text-xl text-muted-foreground">No agents running</p>}
    </Panel>
  )
}
