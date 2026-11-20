import { describe, expect, it } from 'vitest'
import type { AgentState, GraphAgent, Island } from '../types'
import {
  DEFAULT_NODE_PX,
  DENSE_MIN_NODE_PX,
  DENSE_NODE_COUNT,
  HUB_CHILD_COUNT,
  MIN_TAP_PX,
  ROOT_NODE_PX,
  TAP_SLOP_PX,
  childCounts,
  isDense,
  isLabelled,
  isTapJitter,
  isTappable,
  nodeKey,
  nodeSizePx,
  webCounts,
} from './rules'

const agent = (
  id: number,
  depth = 0,
  state: AgentState = 'working',
  parent: number | null = null,
): GraphAgent => ({
  id,
  parent,
  kind: depth === 0 ? 'orchestrator' : 'subagent',
  role: 'implementer',
  chargeKey: 'demo#1',
  name: null,
  state,
  costUsd: null,
  contextPct: null,
  depth,
  focus: null,
})

const island = (
  sessionId: string,
  agents: GraphAgent[],
  edges: [number, number][] = [],
): Island => ({
  sessionId,
  startedAtMs: null,
  sessionCostUsd: null,
  agents,
  edges: edges.map(([parent, child]) => ({ parent, child, kind: 'spawn' })),
})

describe('isDense', () => {
  it('is false at exactly the threshold and true one above', () => {
    expect(isDense(DENSE_NODE_COUNT)).toBe(false)
    expect(isDense(DENSE_NODE_COUNT + 1)).toBe(true)
    expect(isDense(0)).toBe(false)
  })
})

describe('childCounts', () => {
  it('counts children per parent, with 0 for childless agents', () => {
    const isl = island(
      's-1',
      [agent(1), agent(2, 1), agent(3, 1), agent(4, 2)],
      [
        [1, 2],
        [1, 3],
        [2, 4],
      ],
    )
    const counts = childCounts(isl)
    expect(counts.get(1)).toBe(2)
    expect(counts.get(2)).toBe(1)
    expect(counts.get(3)).toBe(0)
    expect(counts.get(4)).toBe(0)
    expect(counts.size).toBe(4)
  })

  it('ignores edges naming an id that is not in the island', () => {
    const isl = island(
      's-1',
      [agent(1), agent(2, 1)],
      [
        [1, 2],
        [99, 2],
        [1, 98],
      ],
    )
    const counts = childCounts(isl)
    expect(counts.has(99)).toBe(false)
    expect(counts.get(1)).toBe(1)
    expect(counts.size).toBe(2)
  })

  it('is empty for an island without agents', () => {
    expect(childCounts(island('s-1', [])).size).toBe(0)
  })
})

describe('isTappable', () => {
  it('a pointer within TAP_SLOP_PX of where it began is jitter, at or past it is a pan, in any direction', () => {
    expect(TAP_SLOP_PX).toBe(8)
    const from = { x: 100, y: 100 }
    expect(isTapJitter(from, from)).toBe(true)
    expect(isTapJitter(from, { x: 102, y: 99 })).toBe(true)
    expect(isTapJitter(from, { x: 100 - (TAP_SLOP_PX - 1), y: 100 })).toBe(true)
    expect(isTapJitter(from, { x: 100, y: 100 + TAP_SLOP_PX })).toBe(false)
    expect(isTapJitter(from, { x: 100 + 6, y: 100 + 6 })).toBe(false)
  })

  it('is true when size times zoom reaches MIN_TAP_PX, false below', () => {
    expect(MIN_TAP_PX).toBe(96)
    expect(isTappable(MIN_TAP_PX, 1)).toBe(true)
    expect(isTappable(MIN_TAP_PX / 2, 2)).toBe(true)
    expect(isTappable(MIN_TAP_PX, 0.99)).toBe(false)
    expect(isTappable(MIN_TAP_PX - 1, 1)).toBe(false)
  })
})

describe('nodeSizePx', () => {
  it('normal look: root is ROOT_NODE_PX, every other depth DEFAULT_NODE_PX', () => {
    expect(ROOT_NODE_PX).toBe(120)
    expect(DEFAULT_NODE_PX).toBe(96)
    expect(nodeSizePx(0, false)).toBe(ROOT_NODE_PX)
    for (const d of [1, 2, 3, 4, 7]) expect(nodeSizePx(d, false)).toBe(DEFAULT_NODE_PX)
  })

  it('dense look: non-increasing, root larger than depth 3, floored, below default', () => {
    let prev = Infinity
    for (let d = 0; d <= 10; d++) {
      const size = nodeSizePx(d, true)
      expect(size).toBeLessThanOrEqual(prev)
      expect(size).toBeGreaterThanOrEqual(DENSE_MIN_NODE_PX)
      expect(size).toBeLessThan(DEFAULT_NODE_PX)
      prev = size
    }
    expect(nodeSizePx(0, true)).toBeGreaterThan(nodeSizePx(3, true))
  })
})

describe('isLabelled', () => {
  it('normal look labels everything', () => {
    expect(isLabelled(agent(2, 3, 'idle'), 0, false)).toBe(true)
  })

  it('dense look labels only roots, hubs and needs_you agents', () => {
    expect(isLabelled(agent(1, 0, 'idle'), 0, true)).toBe(true)
    expect(isLabelled(agent(2, 2, 'idle'), HUB_CHILD_COUNT, true)).toBe(true)
    expect(isLabelled(agent(3, 2, 'needs_you'), 0, true)).toBe(true)
    expect(isLabelled(agent(4, 2, 'working'), HUB_CHILD_COUNT - 1, true)).toBe(false)
    expect(isLabelled(agent(5, 2, 'done'), 0, true)).toBe(false)
  })
})

describe('webCounts', () => {
  it('totals agents, working, done and max depth across islands', () => {
    const counts = webCounts([
      island('s-1', [agent(1, 0, 'working'), agent(2, 1, 'done'), agent(3, 2, 'needs_you')]),
      island('s-2', [agent(1, 0, 'idle'), agent(2, 4, 'working'), agent(3, 1, 'done')]),
    ])
    expect(counts).toEqual({ total: 6, working: 2, finished: 2, depth: 4 })
  })

  it('is all zero for no islands or empty islands', () => {
    const zero = { total: 0, working: 0, finished: 0, depth: 0 }
    expect(webCounts([])).toEqual(zero)
    expect(webCounts([island('s-1', [])])).toEqual(zero)
  })
})

describe('nodeKey', () => {
  it('is unique across islands that reuse an agent id, and stable', () => {
    expect(nodeKey('s-1', 1)).not.toBe(nodeKey('s-2', 1))
    expect(nodeKey('s-1', 1)).not.toBe(nodeKey('s-1', 2))
    expect(nodeKey('s-1', 1)).toBe(nodeKey('s-1', 1))
  })

  it('does not collide when the join could be ambiguous', () => {
    expect(nodeKey('s-1', 12)).not.toBe(nodeKey('s-11', 2))
  })
})
