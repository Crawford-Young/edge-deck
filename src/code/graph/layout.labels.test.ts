import { describe, expect, it } from 'vitest'
import type { GraphAgent, Island } from '../types'
import { layoutWeb } from './layout'
import type { IslandLayout, Point } from './layout'
import { nodeKey, nodeSizePx } from './rules'

// The label block hangs 103 px below the disc and 55 px past each side of it (measured live).
const BELOW = 103
const SIDE = 55

const agent = (id: number, depth: number): GraphAgent => ({
  id,
  parent: null,
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

/** An island from `[id, depth, parent]` rows. */
const build = (sessionId: string, rows: [number, number, number | null][]): Island => ({
  sessionId,
  startedAtMs: null,
  sessionCostUsd: null,
  agents: rows.map(([id, depth]) => agent(id, depth)),
  edges: rows.flatMap(([id, , parent]) =>
    parent === null ? [] : [{ parent, child: id, kind: 'spawn' as const }],
  ),
})

const star = (sessionId: string, children: number): Island =>
  build(sessionId, [
    [1, 0, null],
    ...Array.from({ length: children }, (_, i): [number, number, number] => [i + 2, 1, 1]),
  ])

interface Box {
  id: string
  left: number
  right: number
  top: number
  bottom: number
}

const boxOf = (sessionId: string, a: GraphAgent, layout: IslandLayout): Box => {
  const p = layout.positions.get(nodeKey(sessionId, a.id)) as Point
  const r = nodeSizePx(a.depth, false) / 2
  return {
    id: `${sessionId}/${a.id}`,
    left: p.x - r - SIDE,
    right: p.x + r + SIDE,
    top: p.y - r,
    bottom: p.y + r + BELOW,
  }
}

const boxes = (islands: Island[]): Box[] => {
  const layouts = layoutWeb(islands)
  return islands.flatMap((island, i) =>
    island.agents.map((a) => boxOf(island.sessionId, a, layouts[i] as IslandLayout)),
  )
}

const hit = (a: Box, b: Box): boolean =>
  a.left < b.right && b.left < a.right && a.top < b.bottom && b.top < a.bottom

const overlapping = (all: Box[]): string[] =>
  all.flatMap((a, i) =>
    all
      .slice(i + 1)
      .filter((b) => hit(a, b))
      .map((b) => `${a.id} x ${b.id}`),
  )

describe('layoutWeb label footprints (normal look)', () => {
  it('keeps a root and one child apart', () => {
    expect(overlapping(boxes([star('a', 1)]))).toEqual([])
  })

  it('keeps a root and five children apart', () => {
    expect(overlapping(boxes([star('a', 5)]))).toEqual([])
  })

  it('keeps a 3-deep chain apart', () => {
    const chain = build('a', [
      [1, 0, null],
      [2, 1, 1],
      [3, 2, 2],
    ])
    expect(overlapping(boxes([chain]))).toEqual([])
  })

  it('keeps the nodes of two islands apart', () => {
    expect(overlapping(boxes([star('a', 3), star('b', 4)]))).toEqual([])
  })

  it('keeps new nodes clear of pinned survivors when growing', () => {
    const first = layoutWeb([star('a', 2)])[0] as IslandLayout
    const grown = star('a', 4)
    const next = (
      layoutWeb([grown], { previous: first.positions }) as IslandLayout[]
    )[0] as IslandLayout
    expect(next.positions.get(nodeKey('a', 1))).toEqual(first.positions.get(nodeKey('a', 1)))
    const all = grown.agents.map((a) => boxOf('a', a, next))
    const fresh = all.filter((b) => b.id === 'a/4' || b.id === 'a/5')
    const clash = fresh.flatMap((a) => all.filter((b) => b.id !== a.id && hit(a, b)))
    expect(clash).toEqual([])
  })

  it('stays deterministic', () => {
    expect(boxes([star('a', 5)])).toEqual(boxes([star('a', 5)]))
  })
})
