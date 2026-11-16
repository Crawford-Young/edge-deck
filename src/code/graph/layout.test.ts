import { describe, expect, it } from 'vitest'
import type { GraphAgent, Island } from '../types'
import { ISLAND_GAP_PX, LINK_DISTANCE_PX, SETTLE_TICKS, layoutWeb } from './layout'
import type { IslandLayout, Point } from './layout'
import { DENSE_NODE_COUNT, nodeKey, nodeSizePx } from './rules'

const agent = (id: number, depth: number, parent: number | null): GraphAgent => ({
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

/** Root 1 with `n - 1` children at depth 1. */
const star = (sessionId: string, n: number): Island =>
  island(
    sessionId,
    Array.from({ length: n }, (_, i) => agent(i + 1, i === 0 ? 0 : 1, i === 0 ? null : 1)),
    Array.from({ length: n - 1 }, (_, i) => [1, i + 2] as [number, number]),
  )

/** A chain at depths 0..4, then enough depth-4 leaves to exceed DENSE_NODE_COUNT. */
const denseIsland = (sessionId: string): Island => {
  const total = DENSE_NODE_COUNT + 6
  const agents: GraphAgent[] = []
  const edges: [number, number][] = []
  for (let i = 1; i <= total; i++) {
    const depth = Math.min(i - 1, 4)
    const parent = i === 1 ? null : i <= 5 ? i - 1 : 4
    agents.push(agent(i, depth, parent))
    if (parent !== null) edges.push([parent, i])
  }
  return island(sessionId, agents, edges)
}

const radius = (a: GraphAgent, dense: boolean): number => nodeSizePx(a.depth, dense) / 2

const pos = (layout: IslandLayout, sessionId: string, id: number): Point => {
  const p = layout.positions.get(nodeKey(sessionId, id))
  if (!p) throw new Error(`no position for ${sessionId}/${id}`)
  return p
}

/** Element `i` of `xs`, failing loudly when absent (narrows under noUncheckedIndexedAccess). */
const at = <T>(xs: readonly T[], i: number): T => {
  const x = xs[i]
  if (x === undefined) throw new Error(`no element at index ${i}`)
  return x
}

const right = (l: IslandLayout): number => l.bounds.x + l.bounds.width

describe('constants', () => {
  it('names a positive settle tick count and island gap', () => {
    expect(SETTLE_TICKS).toBeGreaterThan(0)
    expect(ISLAND_GAP_PX).toBeGreaterThan(0)
  })
})

describe('layoutWeb: shape', () => {
  it('returns one entry per island in input order, keyed by sessionId', () => {
    const islands = [star('s-1', 3), star('s-2', 2), star('s-3', 4)]
    const out = layoutWeb(islands)
    expect(out.map((l) => l.sessionId)).toEqual(['s-1', 's-2', 's-3'])
  })

  it('returns [] for no islands', () => {
    expect(layoutWeb([])).toEqual([])
  })

  it('is deterministic: a second call deep-equals the first', () => {
    const islands = [star('s-1', 6), denseIsland('s-2')]
    expect(layoutWeb(islands)).toEqual(layoutWeb(islands))
    expect(layoutWeb(islands, { dense: true })).toEqual(layoutWeb(islands, { dense: true }))
  })

  it('gives every agent of every island a finite position, keyed by nodeKey', () => {
    const islands = [star('s-1', 5), star('s-2', 5)]
    const out = layoutWeb(islands)
    islands.forEach((isl, i) => {
      expect(at(out, i).positions.size).toBe(isl.agents.length)
      for (const a of isl.agents) {
        const p = pos(at(out, i), isl.sessionId, a.id)
        expect(Number.isFinite(p.x)).toBe(true)
        expect(Number.isFinite(p.y)).toBe(true)
      }
    })
  })

  it('keeps two islands that reuse agent ids apart in their own maps', () => {
    const out = layoutWeb([star('s-1', 2), star('s-2', 2)])
    expect(at(out, 0).positions.has(nodeKey('s-2', 1))).toBe(false)
    expect(at(out, 1).positions.has(nodeKey('s-1', 1))).toBe(false)
  })

  it('handles an island with no agents: empty map, finite bounds, no throw', () => {
    const out = layoutWeb([island('s-1', [])])
    expect(out).toHaveLength(1)
    expect(at(out, 0).positions.size).toBe(0)
    for (const v of Object.values(at(out, 0).bounds)) expect(Number.isFinite(v)).toBe(true)
  })

  it('handles a single-agent island', () => {
    const out = layoutWeb([island('s-1', [agent(1, 0, null)])])
    expect(at(out, 0).positions.size).toBe(1)
  })

  it('ignores edges naming ids that are not in the island', () => {
    const isl = island(
      's-1',
      [agent(1, 0, null), agent(2, 1, 1)],
      [
        [1, 2],
        [99, 2],
        [1, 98],
      ],
    )
    const out = layoutWeb([isl])
    expect(at(out, 0).positions.size).toBe(2)
    for (const p of at(out, 0).positions.values()) {
      expect(Number.isFinite(p.x)).toBe(true)
      expect(Number.isFinite(p.y)).toBe(true)
    }
  })
})

describe.each([
  ['normal', false],
  ['dense', true],
])('layoutWeb: geometry (%s look)', (_name, dense) => {
  const islands = [star('s-1', 7), denseIsland('s-2'), star('s-3', 3)]
  const out = layoutWeb(islands, { dense })

  it("bounds contain every node's full disc", () => {
    islands.forEach((isl, i) => {
      const b = at(out, i).bounds
      for (const a of isl.agents) {
        const p = pos(at(out, i), isl.sessionId, a.id)
        const r = radius(a, dense)
        expect(p.x - r).toBeGreaterThanOrEqual(b.x - 1e-6)
        expect(p.y - r).toBeGreaterThanOrEqual(b.y - 1e-6)
        expect(p.x + r).toBeLessThanOrEqual(b.x + b.width + 1e-6)
        expect(p.y + r).toBeLessThanOrEqual(b.y + b.height + 1e-6)
      }
    })
  })

  it('no two nodes in one island are closer than 0.9 x the sum of their radii', () => {
    islands.forEach((isl, i) => {
      for (let a = 0; a < isl.agents.length; a++) {
        for (let b = a + 1; b < isl.agents.length; b++) {
          const aa = at(isl.agents, a)
          const ab = at(isl.agents, b)
          const pa = pos(at(out, i), isl.sessionId, aa.id)
          const pb = pos(at(out, i), isl.sessionId, ab.id)
          const min = 0.9 * (radius(aa, dense) + radius(ab, dense))
          expect(Math.hypot(pa.x - pb.x, pa.y - pb.y)).toBeGreaterThanOrEqual(min)
        }
      }
    })
  })

  it('places islands left to right with at least ISLAND_GAP_PX between bounds', () => {
    for (let i = 1; i < out.length; i++) {
      expect(at(out, i).bounds.x).toBeGreaterThanOrEqual(
        right(at(out, i - 1)) + ISLAND_GAP_PX - 1e-6,
      )
    }
  })
})

describe('layoutWeb: previous positions', () => {
  const islands = [star('s-1', 4), star('s-2', 3)]
  const fresh = layoutWeb(islands)

  it('keeps every node whose key is in previous at exactly that point', () => {
    // Islands pinned far enough apart that no re-pack shift is needed: s-2's pin sits a
    // full ISLAND_GAP_PX plus both islands' widths (twice over, for slack) right of s-1's.
    const x1 = 500
    const x2 = x1 + 2 * (at(fresh, 0).bounds.width + at(fresh, 1).bounds.width + ISLAND_GAP_PX)
    const previous = new Map<string, Point>([
      [nodeKey('s-1', 1), { x: x1, y: -300 }],
      [nodeKey('s-2', 3), { x: x2, y: 12.5 }],
    ])
    const out = layoutWeb(islands, { previous })
    expect(pos(at(out, 0), 's-1', 1)).toEqual({ x: x1, y: -300 })
    expect(pos(at(out, 1), 's-2', 3)).toEqual({ x: x2, y: 12.5 })
  })

  it('gives new nodes in an island with survivors finite positions', () => {
    const previous = new Map<string, Point>([[nodeKey('s-1', 1), { x: 10, y: 20 }]])
    const out = layoutWeb(islands, { previous })
    expect(at(out, 0).positions.size).toBe(4)
    for (const p of at(out, 0).positions.values()) {
      expect(Number.isFinite(p.x)).toBe(true)
      expect(Number.isFinite(p.y)).toBe(true)
    }
  })

  it('reproduces a full previous layout unchanged', () => {
    const previous = new Map<string, Point>()
    for (const l of fresh) for (const [k, p] of l.positions) previous.set(k, p)
    const out = layoutWeb(islands, { previous })
    expect(out.map((l) => [...l.positions])).toEqual(fresh.map((l) => [...l.positions]))
  })

  it('places an island with no survivors right of every other island, without overlap', () => {
    const previous = new Map<string, Point>()
    for (const [k, p] of at(fresh, 0).positions) previous.set(k, p)
    const withNew = [...islands, star('s-3', 3)]
    const out = layoutWeb(withNew, { previous })
    const others = out.slice(0, 2)
    const added = at(out, 2)
    const maxRight = Math.max(...others.map(right))
    expect(added.bounds.x).toBeGreaterThanOrEqual(maxRight - 1e-6)
    for (const o of others) {
      const disjoint = added.bounds.x >= right(o) - 1e-6 || right(added) <= o.bounds.x + 1e-6
      expect(disjoint).toBe(true)
    }
  })

  it('places an unseen island first in input order right of later survivors too', () => {
    const previous = new Map<string, Point>()
    for (const [k, p] of at(fresh, 1).positions) previous.set(k, p)
    const out = layoutWeb([star('s-0', 3), at(islands, 1)], { previous })
    expect(out.map((l) => l.sessionId)).toEqual(['s-0', 's-2'])
    expect(at(out, 0).bounds.x).toBeGreaterThanOrEqual(right(at(out, 1)) - 1e-6)
  })
})

describe('layoutWeb: new nodes land near their survivors', () => {
  it('puts a new child within 2 x LINK_DISTANCE_PX of its pinned parent', () => {
    const parentAt = { x: 2000, y: 1500 }
    const previous = new Map<string, Point>([[nodeKey('s-1', 1), parentAt]])
    const out = layoutWeb([star('s-1', 4)], { previous })
    for (const id of [2, 3, 4]) {
      const p = pos(at(out, 0), 's-1', id)
      expect(Math.hypot(p.x - parentAt.x, p.y - parentAt.y)).toBeLessThanOrEqual(
        2 * LINK_DISTANCE_PX,
      )
    }
  })

  it('puts a new node with no surviving parent inside the survivors box grown by 2 x LINK_DISTANCE_PX', () => {
    // 1 -> 2, 1 -> 3, 3 -> 4; only 1 and 2 survive, so 4's parent is new.
    const isl = island(
      's-1',
      [agent(1, 0, null), agent(2, 1, 1), agent(3, 1, 1), agent(4, 2, 3)],
      [
        [1, 2],
        [1, 3],
        [3, 4],
      ],
    )
    const a = { x: 2000, y: 1500 }
    const b = { x: 2150, y: 1560 }
    const previous = new Map<string, Point>([
      [nodeKey('s-1', 1), a],
      [nodeKey('s-1', 2), b],
    ])
    const out = layoutWeb([isl], { previous })
    const margin = 2 * LINK_DISTANCE_PX
    const p = pos(at(out, 0), 's-1', 4)
    expect(p.x).toBeGreaterThanOrEqual(Math.min(a.x, b.x) - margin)
    expect(p.x).toBeLessThanOrEqual(Math.max(a.x, b.x) + margin)
    expect(p.y).toBeGreaterThanOrEqual(Math.min(a.y, b.y) - margin)
    expect(p.y).toBeLessThanOrEqual(Math.max(a.y, b.y) + margin)
  })

  it('does not make two islands overlap when one grows by two children of a survivor', () => {
    const fresh = layoutWeb([star('s-1', 4), star('s-2', 3)])
    const previous = new Map<string, Point>()
    for (const l of fresh) for (const [k, p] of l.positions) previous.set(k, p)
    const out = layoutWeb([star('s-1', 4), star('s-2', 5)], { previous })
    const [first, second] = [at(out, 0).bounds, at(out, 1).bounds]
    const disjoint =
      first.x + first.width <= second.x + 1e-6 ||
      second.x + second.width <= first.x + 1e-6 ||
      first.y + first.height <= second.y + 1e-6 ||
      second.y + second.height <= first.y + 1e-6
    expect(disjoint).toBe(true)
  })
})
