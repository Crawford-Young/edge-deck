import type { FocusTarget } from '../api'

/** An agent's state, as strate-core serialises it. */
export type AgentState = 'working' | 'needs_you' | 'idle' | 'done'

/** One wait on the user; null fields are unknown, never zero. */
export interface Wait {
  label: string
  waitingFor: string | null
  sinceMs: number | null
  focus: FocusTarget | null
}

/** The oldest live root's start and $, plus totals. */
export interface HeaderData {
  startedAtMs: number | null
  sessionCostUsd: number | null
  todayUsd: number
  liveAgents: number
  needYou: number
}

export interface GraphAgent {
  id: number
  parent: number | null
  /** orchestrator | subagent | teammate */
  kind: string
  role: string | null
  chargeKey: string | null
  name: string | null
  state: AgentState
  costUsd: number | null
  contextPct: number | null
  /** Steps from the island's root orchestrator, which is 0. */
  depth: number
  focus: FocusTarget | null
}

export interface GraphEdge {
  parent: number
  child: number
  kind: string
}

/** One live root session's running tree. */
export interface Island {
  sessionId: string
  startedAtMs: number | null
  sessionCostUsd: number | null
  agents: GraphAgent[]
  edges: GraphEdge[]
}

/** The `code_snapshot` command's document. */
export interface CodeSnapshot {
  /** One island per live root session, oldest first. */
  graphs: Island[]
  /** Every wait on the user, longest first. */
  needsYou: Wait[]
  header: HeaderData
}
