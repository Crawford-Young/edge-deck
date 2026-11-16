import { describe, expect, it } from 'vitest'
import type { Island } from '../types'
import { layoutWeb } from './layout'
import type { Point } from './layout'
import { nodeKey } from './rules'

const agent = (id: number, depth: number, parent: number | null): Island['agents'][number] => ({
  id,
  parent,
  kind: depth === 0 ? 'orchestrator' : 'subagent',
  role: 'implementer',
  chargeKey: 'demo#1',
  name: null,
  state: 'working',
  costUsd: null,
  contextPct: null,
  depth,
  focus: null,
})

describe('layoutWeb: coincident previous points', () => {
  it('stays deterministic and finite when two linked nodes share a previous point', () => {
    const island: Island = {
      sessionId: 's-1',
      startedAtMs: null,
      sessionCostUsd: null,
      agents: [agent(1, 0, null), agent(2, 1, 1)],
      edges: [{ parent: 1, child: 2, kind: 'spawn' }],
    }
    const same: Point = { x: 5, y: 5 }
    const previous = new Map<string, Point>([
      [nodeKey('s-1', 1), same],
      [nodeKey('s-1', 2), same],
    ])
    const first = layoutWeb([island], { previous })
    expect(layoutWeb([island], { previous })).toEqual(first)
    expect(first[0]?.positions.get(nodeKey('s-1', 2))).toEqual(same)
    expect(Number.isFinite(first[0]?.bounds.width)).toBe(true)
  })
})
