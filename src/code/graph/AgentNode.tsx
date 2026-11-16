import { AgentNodeRing, type AgentNodeState } from '@crawfordyoung/ui'
import { Handle, Position, useStore, type Node, type NodeProps } from '@xyflow/react'
import type { AgentState, GraphAgent } from '../types'
import { LABEL_FONT_PX, isLabelled, labelScale, nodeSizePx } from './rules'

export type AgentNodeData = { agent: GraphAgent; dense: boolean; childCount: number }

export type AgentFlowNode = Node<AgentNodeData, 'agent'>

const RING_STATE: Record<AgentState, AgentNodeState> = {
  working: 'working',
  needs_you: 'needs-you',
  idle: 'idle',
  done: 'done',
}

/** What a node is called: its role, else its agent name, else its kind. */
export const agentLabel = (agent: GraphAgent): string => agent.role ?? agent.name ?? agent.kind

// Both ends sit on the disc's centre, hidden, so a straight edge runs centre to centre.
const HANDLE_STYLE = { top: '50%' }

/**
 * One agent as a React Flow node. The node's box is the disc, so React Flow's
 * position (with a centred node origin) is the disc's centre; labels hang below.
 */
export function AgentNode({ data }: NodeProps<AgentFlowNode>) {
  const { agent, dense, childCount } = data
  // Only a labelled dense node scales its label; selecting the scale (not the zoom)
  // re-renders a node only when its scale changes.
  const labelled = dense && isLabelled(agent, childCount, dense)
  const scale = useStore((s) => (labelled ? labelScale(s.transform[2]) : 1))
  const size = nodeSizePx(agent.depth, dense)
  const name = agentLabel(agent)
  const ring = {
    name,
    state: RING_STATE[agent.state],
    charge: agent.chargeKey ?? undefined,
    cost: agent.costUsd === null ? undefined : `$${agent.costUsd.toFixed(2)}`,
    context: agent.contextPct ?? undefined,
  }
  return (
    <div className="relative flex items-start justify-center" style={{ width: size, height: size }}>
      <Handle
        type="target"
        position={Position.Top}
        isConnectable={false}
        className="invisible"
        style={HANDLE_STYLE}
      />
      {dense ? (
        <AgentNodeRing {...ring} size="dense" style={{ width: size, height: size }} />
      ) : (
        <AgentNodeRing {...ring} size={agent.depth === 0 ? 'root' : 'default'} />
      )}
      {labelled && (
        <span
          aria-hidden="true"
          className="absolute top-full mt-1 flex flex-col items-center font-semibold whitespace-nowrap text-foreground"
          style={{
            fontSize: LABEL_FONT_PX,
            transform: `scale(${scale})`,
            transformOrigin: 'top center',
          }}
        >
          <span>{name}</span>
          {agent.state === 'needs_you' && <span className="text-warning">needs you</span>}
        </span>
      )}
      <Handle
        type="source"
        position={Position.Top}
        isConnectable={false}
        className="invisible"
        style={HANDLE_STYLE}
      />
    </div>
  )
}
