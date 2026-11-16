import { forceCollide, forceLink, forceManyBody, forceSimulation } from 'd3-force'
import type { SimulationLinkDatum, SimulationNodeDatum } from 'd3-force'
import type { Island } from '../types'
import { DEFAULT_NODE_PX, ROOT_NODE_PX, nodeKey, nodeSizePx } from './rules'

/** Synchronous simulation ticks per island. */
export const SETTLE_TICKS = 300
/** Minimum horizontal space between two islands' bounds, px. */
export const ISLAND_GAP_PX = 160
/** Extra clearance around each node's disc in the collide force, px. */
export const COLLIDE_GAP_PX = 24
/** Extra link length beyond the two radii, px. */
export const LINK_SLACK_PX = 48
/** Link length: the largest pair of radii (root + child) plus the slack, px. */
export const LINK_DISTANCE_PX = ROOT_NODE_PX / 2 + DEFAULT_NODE_PX / 2 + LINK_SLACK_PX
/** Many-body repulsion strength (negative repels). */
export const CHARGE_STRENGTH = -400
/** Dense look: link length, px; shorter than the collide discs so links pull tight. */
export const DENSE_LINK_DISTANCE_PX = 40
/** Dense look: many-body repulsion strength; weak so a big web stays compact. */
export const DENSE_CHARGE_STRENGTH = -60
/** Collide force iterations per tick. */
export const COLLIDE_ITERATIONS = 4
/** Spiral spacing of the deterministic start positions, px. */
export const SPIRAL_STEP_PX = 60
/** Golden angle, radians; spreads start positions evenly. */
export const GOLDEN_ANGLE = Math.PI * (3 - Math.sqrt(5))
/** Offset of a new node from its seed (surviving parent or survivors' centroid), px. */
export const SEED_OFFSET_PX = LINK_DISTANCE_PX / 2
/** Seeded generator constants (LCG, Numerical Recipes). */
const LCG_MULTIPLIER = 1664525
const LCG_INCREMENT = 1013904223
const LCG_MODULUS = 2 ** 32
const RANDOM_SEED = 1

export interface Point {
  readonly x: number
  readonly y: number
}

export interface Bounds {
  readonly x: number
  readonly y: number
  readonly width: number
  readonly height: number
}

export interface IslandLayout {
  readonly sessionId: string
  readonly positions: ReadonlyMap<string, Point>
  readonly bounds: Bounds
}

export interface LayoutOptions {
  previous?: ReadonlyMap<string, Point>
  dense?: boolean
}

interface SimNode extends SimulationNodeDatum {
  id: number
  radius: number
  key: string
  x: number
  y: number
}

const seededRandom = (): (() => number) => {
  let state = RANDOM_SEED
  return () => {
    state = (LCG_MULTIPLIER * state + LCG_INCREMENT) % LCG_MODULUS
    return state / LCG_MODULUS
  }
}

const boundsOf = (nodes: readonly SimNode[]): Bounds => {
  if (nodes.length === 0) return { x: 0, y: 0, width: 0, height: 0 }
  let minX = Infinity
  let minY = Infinity
  let maxX = -Infinity
  let maxY = -Infinity
  for (const n of nodes) {
    minX = Math.min(minX, n.x - n.radius)
    minY = Math.min(minY, n.y - n.radius)
    maxX = Math.max(maxX, n.x + n.radius)
    maxY = Math.max(maxY, n.y + n.radius)
  }
  return { x: minX, y: minY, width: maxX - minX, height: maxY - minY }
}

/** Settle one island; survivors in `previous` are pinned at their old point. */
const settle = (
  island: Island,
  dense: boolean,
  previous: ReadonlyMap<string, Point> | undefined,
): { nodes: SimNode[]; positions: Map<string, Point>; survivors: number } => {
  const nodes: SimNode[] = island.agents.map((agent, i) => {
    const key = nodeKey(island.sessionId, agent.id)
    const spiral = SPIRAL_STEP_PX * Math.sqrt(i)
    const node: SimNode = {
      id: agent.id,
      key,
      radius: nodeSizePx(agent.depth, dense) / 2,
      x: spiral * Math.cos(i * GOLDEN_ANGLE),
      y: spiral * Math.sin(i * GOLDEN_ANGLE),
    }
    const old = previous?.get(key)
    if (old) {
      node.x = old.x
      node.y = old.y
      node.fx = old.x
      node.fy = old.y
    }
    return node
  })
  const pinned = nodes.filter((n) => n.fx !== undefined)
  const survivors = pinned.length
  const byId = new Map(nodes.map((n) => [n.id, n]))

  // New nodes in an island with survivors start beside their surviving parent,
  // else beside the survivors' centroid, never near world (0,0).
  if (survivors > 0) {
    const parentOf = new Map(island.edges.map((e) => [e.child, byId.get(e.parent)]))
    const centroid = {
      x: pinned.reduce((sum, n) => sum + n.x, 0) / survivors,
      y: pinned.reduce((sum, n) => sum + n.y, 0) / survivors,
    }
    let seeded = 0
    for (const node of nodes) {
      if (node.fx !== undefined) continue
      const parent = parentOf.get(node.id)
      const anchor = parent?.fx === undefined ? centroid : parent
      const angle = seeded * GOLDEN_ANGLE
      node.x = anchor.x + SEED_OFFSET_PX * Math.cos(angle)
      node.y = anchor.y + SEED_OFFSET_PX * Math.sin(angle)
      seeded += 1
    }
  }

  // Nothing can move when every node is pinned.
  if (survivors < nodes.length) {
    const links: SimulationLinkDatum<SimNode>[] = island.edges
      .filter((e) => byId.has(e.parent) && byId.has(e.child))
      .map((e) => ({ source: e.parent, target: e.child }))
    forceSimulation<SimNode>(nodes)
      .randomSource(seededRandom())
      .force(
        'link',
        forceLink<SimNode, SimulationLinkDatum<SimNode>>(links)
          .id((n) => n.id)
          .distance(dense ? DENSE_LINK_DISTANCE_PX : LINK_DISTANCE_PX),
      )
      .force(
        'charge',
        forceManyBody<SimNode>().strength(dense ? DENSE_CHARGE_STRENGTH : CHARGE_STRENGTH),
      )
      .force(
        'collide',
        forceCollide<SimNode>((n) => n.radius + COLLIDE_GAP_PX / 2).iterations(COLLIDE_ITERATIONS),
      )
      .stop()
      .tick(SETTLE_TICKS)
  }

  const positions = new Map<string, Point>()
  for (const n of nodes) positions.set(n.key, { x: n.x, y: n.y })
  return { nodes, positions, survivors }
}

/** A gap short of ISLAND_GAP_PX by no more than this is float noise, not overlap, px. */
const PACK_TOLERANCE_PX = 1e-6

/** Translate a whole island so its bounds start at `x`; inner offsets are unchanged. */
const moveTo = (layout: IslandLayout, x: number): IslandLayout => {
  const dx = x - layout.bounds.x
  return {
    sessionId: layout.sessionId,
    positions: new Map([...layout.positions].map(([k, p]) => [k, { x: p.x + dx, y: p.y }])),
    bounds: { ...layout.bounds, x },
  }
}

const rightOf = (layout: IslandLayout): number => layout.bounds.x + layout.bounds.width

export const layoutWeb = (
  islands: readonly Island[],
  options: LayoutOptions = {},
): IslandLayout[] => {
  const { previous, dense = false } = options
  const entries = islands.map((island) => {
    const { nodes, positions, survivors } = settle(island, dense, previous)
    const layout: IslandLayout = {
      sessionId: island.sessionId,
      positions,
      bounds: boundsOf(nodes),
    }
    // Where the island's survivors sit orders the walk below; growth can't reorder it.
    const anchor = survivors > 0 ? boundsOf(nodes.filter((n) => n.fx !== undefined)).x : null
    return { layout, anchor }
  })

  // Islands with survivors keep their place unless a left neighbour grew into
  // them: walking left to right, each is pushed right just enough to keep
  // ISLAND_GAP_PX. Nothing is pulled left, so a shrink or an unchanged refetch
  // moves nothing.
  const kept = entries
    .flatMap((entry) => (entry.anchor === null ? [] : [{ entry, anchor: entry.anchor }]))
    .sort((a, b) => a.anchor - b.anchor)
  let cursor = 0
  let minX = -Infinity
  for (const { entry } of kept) {
    if (entry.layout.bounds.x < minX - PACK_TOLERANCE_PX) entry.layout = moveTo(entry.layout, minX)
    minX = rightOf(entry.layout) + ISLAND_GAP_PX
    cursor = Math.max(cursor, minX)
  }

  // All-new islands go side by side to the right of everything placed.
  return entries.map((entry) => {
    if (entry.anchor !== null) return entry.layout
    const placed = moveTo(entry.layout, cursor)
    cursor += placed.bounds.width + ISLAND_GAP_PX
    return placed
  })
}
