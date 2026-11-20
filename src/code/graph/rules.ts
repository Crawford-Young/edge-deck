import type { GraphAgent, Island } from '../types'
import type { Point } from './layout'

/** The web is dense when the total agent count is strictly greater than this. */
export const DENSE_NODE_COUNT = 24
/** An agent with at least this many children is a hub. */
export const HUB_CHILD_COUNT = 3
/** Smallest touch target, px. */
export const MIN_TAP_PX = 96
/** A press that stays within this distance of where it began is a tap, not a pan, px. */
export const TAP_SLOP_PX = 8
/** Root node diameter, px; matches AgentNodeRing. */
export const ROOT_NODE_PX = 120
/** Every non-root node diameter in the normal look, px. */
export const DEFAULT_NODE_PX = 96
/** Dense look: the root's diameter, px. */
export const DENSE_ROOT_NODE_PX = 84
/** Dense look: diameter lost per depth step, px. */
export const DENSE_SHRINK_PER_DEPTH_PX = 12
/** Dense look: no node is smaller than this, px. */
export const DENSE_MIN_NODE_PX = 48
/** Dense look: a label's font size in flow units, px. */
export const LABEL_FONT_PX = 18
/** Dense look: a label is never drawn smaller than this on screen, px. */
export const MIN_LABEL_SCREEN_PX = 16

export interface WebCounts {
  readonly total: number
  readonly working: number
  readonly finished: number
  readonly depth: number
}

export const isDense = (total: number): boolean => total > DENSE_NODE_COUNT

/** Agent id -> number of children, within the island; unknown ids are ignored. */
export const childCounts = (island: Island): ReadonlyMap<number, number> => {
  const counts = new Map<number, number>(island.agents.map((a) => [a.id, 0]))
  for (const edge of island.edges) {
    const n = counts.get(edge.parent)
    if (n !== undefined && counts.has(edge.child)) counts.set(edge.parent, n + 1)
  }
  return counts
}

/** Whether a pointer that began at `from` and is now at `to` is still within a tap's slop. */
export const isTapJitter = (from: Point, to: Point): boolean =>
  Math.hypot(to.x - from.x, to.y - from.y) < TAP_SLOP_PX

export const isTappable = (sizePx: number, zoom: number): boolean => sizePx * zoom >= MIN_TAP_PX

/** Label scale at `zoom`: 1 when the label is legible, else exactly enough to reach the minimum. */
export const labelScale = (zoom: number): number =>
  zoom >= MIN_LABEL_SCREEN_PX / LABEL_FONT_PX ? 1 : MIN_LABEL_SCREEN_PX / (LABEL_FONT_PX * zoom)

export const nodeSizePx = (depth: number, dense: boolean): number => {
  if (!dense) return depth === 0 ? ROOT_NODE_PX : DEFAULT_NODE_PX
  return Math.max(DENSE_MIN_NODE_PX, DENSE_ROOT_NODE_PX - depth * DENSE_SHRINK_PER_DEPTH_PX)
}

export const isLabelled = (agent: GraphAgent, childCount: number, dense: boolean): boolean =>
  !dense || agent.depth === 0 || childCount >= HUB_CHILD_COUNT || agent.state === 'needs_you'

export const webCounts = (islands: readonly Island[]): WebCounts => {
  let total = 0
  let working = 0
  let finished = 0
  let depth = 0
  for (const island of islands) {
    for (const agent of island.agents) {
      total += 1
      if (agent.state === 'working') working += 1
      if (agent.state === 'done') finished += 1
      depth = Math.max(depth, agent.depth)
    }
  }
  return { total, working, finished, depth }
}

/** Unique across islands; the length prefix keeps the join unambiguous. */
export const nodeKey = (sessionId: string, agentId: number): string =>
  `${sessionId.length}:${sessionId}:${agentId}`
