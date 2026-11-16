import { describe, expect, it } from 'vitest'
import type { GraphAgent, Island } from '../types'
import {
  CHARGE_STRENGTH,
  DENSE_CHARGE_STRENGTH,
  DENSE_LINK_DISTANCE_PX,
  ISLAND_GAP_PX,
  LINK_DISTANCE_PX,
  layoutWeb,
} from './layout'
import type { IslandLayout, Point } from './layout'
import { nodeSizePx } from './rules'

const agent = (id: number, depth: number, parent: number | null): GraphAgent => ({
  id,
  parent,
  kind: depth === 0 ? 'orchestrator' : 'subagent',
  role: 'worker',
  chargeKey: null,
  name: null,
  state: 'working',
  costUsd: null,
  contextPct: null,
  depth,
  focus: null,
})

/** A tree of n agents; agent i's parent is the (i-2)/branch-th earlier agent. */
const tree = (sessionId: string, n: number, branch = 3): Island => {
  const agents: GraphAgent[] = []
  const edges: Island['edges'] = []
  const depthOf = new Map<number, number>()
  for (let i = 1; i <= n; i++) {
    const parent = i === 1 ? null : Math.floor((i - 2) / branch) + 1
    const depth = parent === null ? 0 : (depthOf.get(parent) ?? 0) + 1
    depthOf.set(i, depth)
    agents.push(agent(i, depth, parent))
    if (parent !== null) edges.push({ parent, child: i, kind: 'spawn' })
  }
  return { sessionId, startedAtMs: null, sessionCostUsd: null, agents, edges }
}

const lastPositions = (layouts: readonly IslandLayout[]): Map<string, Point> => {
  const all = new Map<string, Point>()
  for (const l of layouts) for (const [k, p] of l.positions) all.set(k, p)
  return all
}

const byId = (layouts: readonly IslandLayout[], sessionId: string): IslandLayout => {
  const found = layouts.find((l) => l.sessionId === sessionId)
  if (!found) throw new Error(`no layout for ${sessionId}`)
  return found
}

const right = (l: IslandLayout): number => l.bounds.x + l.bounds.width
const EPS = 1e-6

const expectBoundsClose = (got: IslandLayout['bounds'], want: IslandLayout['bounds']): void => {
  expect(got.x).toBeCloseTo(want.x, 6)
  expect(got.y).toBeCloseTo(want.y, 6)
  expect(got.width).toBeCloseTo(want.width, 6)
  expect(got.height).toBeCloseTo(want.height, 6)
}

describe('layoutWeb: island re-pack after growth', () => {
  it('grown LEFT island of two keeps ISLAND_GAP_PX to its right neighbour', () => {
    const first = layoutWeb([tree('a', 3), tree('b', 3)])
    const out = layoutWeb([tree('a', 15), tree('b', 3)], { previous: lastPositions(first) })
    const a = byId(out, 'a')
    const b = byId(out, 'b')
    expect(b.bounds.x - right(a)).toBeGreaterThanOrEqual(ISLAND_GAP_PX - EPS)
  })

  it('grown MIDDLE island of three: both gaps hold and the left island does not move', () => {
    const first = layoutWeb([tree('a', 3), tree('b', 3), tree('c', 3)])
    const before = byId(first, 'a')
    const out = layoutWeb([tree('a', 3), tree('b', 20), tree('c', 3)], {
      previous: lastPositions(first),
    })
    const [a, b, c] = [byId(out, 'a'), byId(out, 'b'), byId(out, 'c')]
    expect(b.bounds.x - right(a)).toBeGreaterThanOrEqual(ISLAND_GAP_PX - EPS)
    expect(c.bounds.x - right(b)).toBeGreaterThanOrEqual(ISLAND_GAP_PX - EPS)
    expectBoundsClose(a.bounds, before.bounds)
    for (const [k, p] of before.positions) expect(a.positions.get(k)).toEqual(p)
  })

  it('re-packs by x order, not input order', () => {
    const first = layoutWeb([tree('a', 3), tree('b', 3), tree('c', 3)])
    const out = layoutWeb([tree('c', 3), tree('b', 20), tree('a', 3)], {
      previous: lastPositions(first),
    })
    const [a, b, c] = [byId(out, 'a'), byId(out, 'b'), byId(out, 'c')]
    expect(b.bounds.x - right(a)).toBeGreaterThanOrEqual(ISLAND_GAP_PX - EPS)
    expect(c.bounds.x - right(b)).toBeGreaterThanOrEqual(ISLAND_GAP_PX - EPS)
    expectBoundsClose(a.bounds, byId(first, 'a').bounds)
  })

  it('an unchanged refetch moves no node and no bounds', () => {
    const islands = [tree('a', 6), tree('b', 4), tree('c', 5)]
    const first = layoutWeb(islands)
    const again = layoutWeb(islands, { previous: lastPositions(first) })
    expect(lastPositions(again)).toEqual(lastPositions(first))
    for (const l of first) expectBoundsClose(byId(again, l.sessionId).bounds, l.bounds)
  })

  it('a shifted island keeps its inner offsets and its bounds move with it', () => {
    const first = layoutWeb([tree('a', 3), tree('b', 4)])
    const prev = lastPositions(first)
    const out = layoutWeb([tree('a', 18), tree('b', 4)], { previous: prev })
    const bBefore = byId(first, 'b')
    const bAfter = byId(out, 'b')
    const dx = bAfter.bounds.x - bBefore.bounds.x
    expect(dx).toBeGreaterThan(0)
    for (const [k, p] of bBefore.positions) {
      const q = bAfter.positions.get(k)
      expect(q?.x).toBeCloseTo(p.x + dx, 6)
      expect(q?.y).toBeCloseTo(p.y, 6)
    }
    expect(bAfter.bounds.y).toBeCloseTo(bBefore.bounds.y, 6)
    expect(bAfter.bounds.width).toBeCloseTo(bBefore.bounds.width, 6)
  })

  it('a right island is not pulled left when the left island shrinks', () => {
    const first = layoutWeb([tree('a', 30), tree('b', 3)])
    const out = layoutWeb([tree('a', 5), tree('b', 3)], { previous: lastPositions(first) })
    const bBefore = byId(first, 'b')
    for (const [k, p] of bBefore.positions) expect(byId(out, 'b').positions.get(k)).toEqual(p)
    expectBoundsClose(byId(out, 'b').bounds, bBefore.bounds)
  })

  it('an all-new island goes right of every placed island, a gap away', () => {
    const first = layoutWeb([tree('a', 3), tree('b', 3)])
    const out = layoutWeb([tree('a', 12), tree('b', 3), tree('n', 4)], {
      previous: lastPositions(first),
    })
    const n = byId(out, 'n')
    for (const id of ['a', 'b']) {
      expect(n.bounds.x - right(byId(out, id))).toBeGreaterThanOrEqual(ISLAND_GAP_PX - EPS)
    }
  })
})

describe('layoutWeb: dense force constants', () => {
  it("are tighter than the normal look and keep the forces' signs", () => {
    expect(DENSE_LINK_DISTANCE_PX).toBeGreaterThan(0)
    expect(DENSE_LINK_DISTANCE_PX).toBeLessThan(LINK_DISTANCE_PX)
    expect(DENSE_CHARGE_STRENGTH).toBeLessThan(0)
    expect(DENSE_CHARGE_STRENGTH).toBeGreaterThan(CHARGE_STRENGTH)
  })

  const dense = layoutWeb([tree('a', 120)], { dense: true })

  it('no two discs overlap in a dense 120-agent web', () => {
    const island = tree('a', 120)
    const radii = new Map(island.agents.map((a) => [a.id, nodeSizePx(a.depth, true) / 2]))
    const layout = byId(dense, 'a')
    const pts = island.agents.map((a) => {
      const p = [...layout.positions.entries()].find(([k]) => k.endsWith(`:${a.id}`))?.[1]
      if (!p) throw new Error(`no position for ${a.id}`)
      return { p, r: radii.get(a.id) ?? 0 }
    })
    for (let i = 0; i < pts.length; i++) {
      for (let j = i + 1; j < pts.length; j++) {
        const a = pts[i]
        const b = pts[j]
        if (!a || !b) throw new Error(`no point at ${i}/${j}`)
        expect(Math.hypot(a.p.x - b.p.x, a.p.y - b.p.y)).toBeGreaterThanOrEqual(a.r + b.r)
      }
    }
  })

  it('fits the 1276x520 panel at zoom >= 0.4 (10% pad each side)', () => {
    const W = 1276
    const H = 520
    const PAD = 0.1
    const w = Math.max(...dense.map(right)) - Math.min(...dense.map((l) => l.bounds.x))
    const y0 = Math.min(...dense.map((l) => l.bounds.y))
    const h = Math.max(...dense.map((l) => l.bounds.y + l.bounds.height)) - y0
    const zoom = Math.min(W / (w * (1 + 2 * PAD)), H / (h * (1 + 2 * PAD)))
    expect(zoom).toBeGreaterThanOrEqual(0.4)
  })
})
