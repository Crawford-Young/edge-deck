import { beforeEach, describe, expect, it, vi } from 'vitest'
import type { GraphAgent, Island } from '../types'
import { layoutWeb } from './layout'
import type { Point } from './layout'
import { nodeKey } from './rules'

const counter = vi.hoisted(() => ({ ticks: 0 }))

vi.mock('d3-force', async (importOriginal) => {
  const actual = await importOriginal<typeof import('d3-force')>()
  return {
    ...actual,
    forceSimulation: (...args: Parameters<typeof actual.forceSimulation>) => {
      const sim = actual.forceSimulation(...args)
      const realTick = sim.tick.bind(sim)
      sim.tick = (iterations?: number) => {
        counter.ticks += iterations ?? 1
        return realTick(iterations)
      }
      return sim
    },
  }
})

const agent = (id: number, parent: number | null): GraphAgent => ({
  id,
  parent,
  kind: parent === null ? 'orchestrator' : 'subagent',
  role: 'implementer',
  chargeKey: 'demo#1',
  name: null,
  state: 'working',
  costUsd: null,
  contextPct: null,
  depth: parent === null ? 0 : 1,
  focus: null,
})

const star = (sessionId: string, n: number): Island => ({
  sessionId,
  startedAtMs: null,
  sessionCostUsd: null,
  agents: Array.from({ length: n }, (_, i) => agent(i + 1, i === 0 ? null : 1)),
  edges: Array.from({ length: n - 1 }, (_, i) => ({ parent: 1, child: i + 2, kind: 'spawn' })),
})

describe('layoutWeb: settle ticks', () => {
  beforeEach(() => {
    counter.ticks = 0
  })

  it('ticks a fresh island (the spy sees the simulation)', () => {
    layoutWeb([star('s-1', 4)])
    expect(counter.ticks).toBeGreaterThan(0)
  })

  it('does not tick an island whose every node is in previous, and returns previous', () => {
    const islands = [star('s-1', 4), star('s-2', 3)]
    const fresh = layoutWeb(islands)
    const previous = new Map<string, Point>()
    for (const l of fresh) for (const [k, p] of l.positions) previous.set(k, p)
    counter.ticks = 0
    const out = layoutWeb(islands, { previous })
    expect(counter.ticks).toBe(0)
    for (const l of out) {
      for (const [k, p] of l.positions) expect(p).toEqual(previous.get(k))
    }
    expect(out[0]?.positions.get(nodeKey('s-1', 1))).toEqual(previous.get(nodeKey('s-1', 1)))
  })
})
