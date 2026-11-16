// edge-deck#3 pass 2d, items 1 and 2: when the view fits again, and what a fit frames.
//
// How a user-made viewport change differs from a programmatic one, in node_modules:
// @xyflow/system/dist/esm/index.mjs:2843-2858 (createPanZoomStartHandler) and 2861-2870
// (createPanZoomHandler) hand `event.sourceEvent` to onMoveStart / onMove. A real pan, zoom or
// pinch carries the DOM event (mousedown, touchstart, wheel). fitView / setCenter / setViewport
// run through d3 with no source event (null), or an `internal` / `sync` marker that is swallowed
// before the callbacks (2845, 2864, 2867). @xyflow/react/dist/esm/index.mjs:1323-1329 forwards
// it as `onMoveStart(event, viewport)` / `onMove(event, viewport)`. So: event non-null = the
// user moved the view; event null = the library did. A node drag is `onNodeDragStart`.
import { act, fireEvent, render, screen } from '@testing-library/react'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { centerCalls, fitView, flowProps, resetFlow, setZoom } from '../../../tests/xyflow-mock'
import { focusTerminal } from '../../api'
import type { GraphAgent, Island } from '../types'
import { AgentWeb, CLUSTER_PAD_PX, REFIT_PANEL_PX } from './AgentWeb'
import { ISLAND_GAP_PX, LABEL_BELOW_PX, LABEL_SIDE_PX } from './layout'
import { nodeKey, nodeSizePx } from './rules'

vi.mock('@xyflow/react', async (importOriginal) => ({
  ...(await importOriginal<typeof import('@xyflow/react')>()),
  ...(await import('../../../tests/xyflow-mock')),
}))

vi.mock('../../api', () => ({ focusTerminal: vi.fn() }))

const island = (sessionId: string, count: number): Island => {
  const agents: GraphAgent[] = Array.from({ length: count }, (_, i) => ({
    id: i + 1,
    parent: i === 0 ? null : 1,
    kind: i === 0 ? 'orchestrator' : 'subagent',
    role: `role-${sessionId}-${i + 1}`,
    chargeKey: i === 0 ? `repo-${sessionId}#1` : null,
    name: null,
    state: 'working',
    costUsd: null,
    contextPct: null,
    depth: i === 0 ? 0 : 1,
    focus: null,
  }))
  return {
    sessionId,
    startedAtMs: null,
    sessionCostUsd: null,
    agents,
    edges: agents.slice(1).map((a) => ({ parent: 1, child: a.id, kind: 'spawn' })),
  }
}

interface FitOptions {
  nodes?: { id: string }[]
  padding?: unknown
}

const clusterIds = (): string[] =>
  (flowProps().nodes ?? []).filter((n) => n.type === 'cluster').map((n) => n.id)

const idsOf = (options: unknown): string[] =>
  ((options as FitOptions | undefined)?.nodes ?? []).map((n) => n.id)

const fitCalls = (): FitOptions[] => fitView.mock.calls.map((c) => c[0] as FitOptions)

/** Narrow a possibly-missing value, failing the test loudly when it is absent. */
const must = <T,>(value: T | null | undefined, what: string): T => {
  expect(value, what).toBeDefined()
  expect(value, what).not.toBeNull()
  if (value === undefined || value === null) throw new Error(`missing: ${what}`)
  return value
}

/** Pixels a padding value reserves on one side; numbers and "Npx" count, anything else is 0. */
const px = (value: unknown): number => {
  if (typeof value === 'number') return value
  const m = typeof value === 'string' ? /^(\d+(?:\.\d+)?)px$/.exec(value) : null
  return m ? Number(m[1]) : 0
}

/** Does this fit leave the Re-fit panel's corner (top-right, 96x96) free of framed content? */
const reservesCorner = (options: FitOptions): boolean => {
  const p = options.padding
  if (typeof p !== 'object' || p === null) return false
  const sides = p as Record<string, unknown>
  return px(sides.top) >= REFIT_PANEL_PX || px(sides.right) >= REFIT_PANEL_PX
}

interface Rect {
  left: number
  right: number
  top: number
  bottom: number
}

/** What each island's target must cover by the spec: discs, label footprint and the pad. */
const requiredRects = (sessionIds: string[]): Rect[] =>
  sessionIds.map((sessionId) => {
    const prefix = nodeKey(sessionId, 0).slice(0, -1)
    const members = (flowProps().nodes ?? []).filter(
      (n) => n.type === 'agent' && n.id.startsWith(prefix),
    )
    const rects = members.map((m) => {
      const r = nodeSizePx((m.data as { agent: GraphAgent }).agent.depth, false) / 2
      return {
        left: m.position.x - r - LABEL_SIDE_PX - CLUSTER_PAD_PX,
        right: m.position.x + r + LABEL_SIDE_PX + CLUSTER_PAD_PX,
        top: m.position.y - r - CLUSTER_PAD_PX,
        bottom: m.position.y + r + LABEL_BELOW_PX + CLUSTER_PAD_PX,
      }
    })
    return {
      left: Math.min(...rects.map((q) => q.left)),
      right: Math.max(...rects.map((q) => q.right)),
      top: Math.min(...rects.map((q) => q.top)),
      bottom: Math.max(...rects.map((q) => q.bottom)),
    }
  })

/** The whole-web fits React Flow was asked for: the mount fit prop and every fitView call. */
const wholeWebFitsAtMount = (): FitOptions[] => {
  const props = flowProps()
  const fromProp = props.fitView ? [props.fitViewOptions as FitOptions] : []
  return [...fromProp, ...fitCalls()]
}

const userSignals: Record<string, () => void> = {
  'a pan (mousedown)': () => {
    const vp = { x: 5, y: 5, zoom: 1 }
    flowProps().onMoveStart?.(new MouseEvent('mousedown'), vp)
    flowProps().onMove?.(new MouseEvent('mousemove'), vp)
  },
  'a wheel zoom': () => {
    const vp = { x: 0, y: 0, zoom: 1.5 }
    flowProps().onMoveStart?.(new WheelEvent('wheel'), vp)
    flowProps().onMove?.(new WheelEvent('wheel'), vp)
  },
  'a pinch (touchstart)': () => {
    const vp = { x: 0, y: 0, zoom: 0.7 }
    flowProps().onMoveStart?.(new Event('touchstart') as unknown as TouchEvent, vp)
    flowProps().onMove?.(new Event('touchmove') as unknown as TouchEvent, vp)
  },
  'a node drag': () => {
    const node = must(
      (flowProps().nodes ?? []).find((n) => n.type === 'agent'),
      'an agent node',
    )
    flowProps().onNodeDragStart?.(new MouseEvent('mousedown'), node, [node])
    flowProps().onNodesChange?.([
      { id: node.id, type: 'position', position: { x: 9, y: 9 }, dragging: true },
    ])
  },
  'a cluster zoom': () => {
    fireEvent.click(screen.getByRole('button', { name: /^Zoom to cluster.*role-a-1/ }))
  },
  'a tap-zoom': () => {
    setZoom(0.4)
    const ring = must(
      screen
        .getAllByRole('button', { name: /role-a-2/ })
        .find((el) => el.hasAttribute('data-size')),
      'a sized ring button',
    )
    fireEvent.click(ring)
    expect(centerCalls).toHaveLength(1)
  },
}

beforeEach(() => {
  resetFlow()
  vi.mocked(focusTerminal).mockReset()
  vi.mocked(focusTerminal).mockResolvedValue(undefined)
})

afterEach(() => {
  vi.useRealTimers()
})

describe('AgentWeb auto-fit', () => {
  it('fits again to every cluster when a layout change adds an island and the view is untouched', () => {
    const { rerender } = render(<AgentWeb islands={[island('a', 3)]} />)
    fitView.mockClear()
    rerender(<AgentWeb islands={[island('a', 3), island('b', 2)]} />)
    expect(fitView).toHaveBeenCalled()
    const last = fitCalls().at(-1)
    expect(clusterIds()).toHaveLength(2)
    expect([...idsOf(last)].sort()).toEqual([...clusterIds()].sort())
  })

  it('fits again when a layout change only grows an existing island', () => {
    const { rerender } = render(<AgentWeb islands={[island('a', 3)]} />)
    fitView.mockClear()
    rerender(<AgentWeb islands={[island('a', 4)]} />)
    expect(fitView).toHaveBeenCalled()
  })

  it('never moves the view for a refetch that changes no layout', () => {
    const { rerender } = render(<AgentWeb islands={[island('a', 3)]} />)
    fitView.mockClear()
    rerender(<AgentWeb islands={[island('a', 3)]} />)
    rerender(<AgentWeb islands={[island('a', 3)]} />)
    expect(fitView).not.toHaveBeenCalled()
    expect(centerCalls).toHaveLength(0)
  })

  it('a move with no source event (the library moving the view itself) does not stop auto-fit', () => {
    const { rerender } = render(<AgentWeb islands={[island('a', 3)]} />)
    act(() => {
      flowProps().onMoveStart?.(null, { x: 0, y: 0, zoom: 1 })
      flowProps().onMove?.(null, { x: 0, y: 0, zoom: 1 })
    })
    fitView.mockClear()
    rerender(<AgentWeb islands={[island('a', 3), island('b', 2)]} />)
    expect(fitView).toHaveBeenCalled()
  })

  for (const [name, move] of Object.entries(userSignals)) {
    it(`${name} counts as moving the view: a later layout change does not fit`, () => {
      const { rerender } = render(<AgentWeb islands={[island('a', 3)]} />)
      act(move)
      fitView.mockClear()
      rerender(<AgentWeb islands={[island('a', 3), island('b', 2)]} />)
      expect(fitView).not.toHaveBeenCalled()
    })
  }

  it('Re-fit turns auto-fit back on after a user move', () => {
    const { rerender } = render(<AgentWeb islands={[island('a', 3)]} />)
    act(must(userSignals['a pan (mousedown)'], 'the pan signal'))
    rerender(<AgentWeb islands={[island('a', 3), island('b', 2)]} />)
    expect(fitView).not.toHaveBeenCalled()
    fireEvent.click(screen.getByRole('button', { name: 'Re-fit' }))
    fitView.mockClear()
    rerender(<AgentWeb islands={[island('a', 3), island('b', 2), island('c', 2)]} />)
    expect(fitView).toHaveBeenCalled()
    expect([...idsOf(fitCalls().at(-1))].sort()).toEqual([...clusterIds()].sort())
  })
})

describe('AgentWeb framing', () => {
  it('exports the label footprint at least as large as the measured one', () => {
    expect(LABEL_BELOW_PX).toBeGreaterThanOrEqual(103)
    expect(LABEL_SIDE_PX).toBeGreaterThanOrEqual(55)
    expect(REFIT_PANEL_PX).toBe(96)
  })

  it('pads the top by the Re-fit button, the panel margin and a 16px gap', () => {
    // 96 button + 15 React Flow panel margin + 16 breathing room, written out.
    render(<AgentWeb islands={[island('a', 3)]} />)
    expect((flowProps().fitViewOptions as FitOptions).padding).toMatchObject({ top: '127px' })
    fireEvent.click(screen.getByRole('button', { name: 'Re-fit' }))
    expect((fitCalls().at(-1) as FitOptions).padding).toMatchObject({ top: '127px' })
  })

  it('a cluster target contains every node disc, its label footprint and the pad', () => {
    render(<AgentWeb islands={[island('a', 4), island('b', 2)]} />)
    const nodes = flowProps().nodes ?? []
    for (const sessionId of ['a', 'b']) {
      const members = nodes.filter(
        (n) => n.type === 'agent' && n.id.startsWith(nodeKey(sessionId, 0).slice(0, -1)),
      )
      const cluster = must(
        nodes.find((n) => n.type === 'cluster' && n.id.endsWith(`:${sessionId}`)),
        `cluster for ${sessionId}`,
      )
      const w = cluster.width as number
      const h = cluster.height as number
      const box = {
        left: cluster.position.x - w / 2,
        right: cluster.position.x + w / 2,
        top: cluster.position.y - h / 2,
        bottom: cluster.position.y + h / 2,
      }
      expect(members.length).toBeGreaterThan(0)
      for (const m of members) {
        const depth = (m.data as { agent: GraphAgent }).agent.depth
        const r = nodeSizePx(depth, false) / 2
        expect(m.position.x - r - LABEL_SIDE_PX - CLUSTER_PAD_PX).toBeGreaterThanOrEqual(
          box.left - 1e-6,
        )
        expect(m.position.x + r + LABEL_SIDE_PX + CLUSTER_PAD_PX).toBeLessThanOrEqual(
          box.right + 1e-6,
        )
        expect(m.position.y - r - CLUSTER_PAD_PX).toBeGreaterThanOrEqual(box.top - 1e-6)
        expect(m.position.y + r + LABEL_BELOW_PX + CLUSTER_PAD_PX).toBeLessThanOrEqual(
          box.bottom + 1e-6,
        )
      }
    }
  })

  it('the mount fit frames the cluster targets, not the bare nodes, and clears the Re-fit corner', () => {
    render(<AgentWeb islands={[island('a', 3), island('b', 2)]} />)
    const fits = wholeWebFitsAtMount()
    expect(fits.length).toBeGreaterThan(0)
    for (const options of fits) {
      expect([...idsOf(options)].sort()).toEqual([...clusterIds()].sort())
      expect(reservesCorner(options)).toBe(true)
    }
  })

  it('an auto-fit frames the cluster targets and clears the Re-fit corner', () => {
    const { rerender } = render(<AgentWeb islands={[island('a', 3)]} />)
    fitView.mockClear()
    rerender(<AgentWeb islands={[island('a', 3), island('b', 2)]} />)
    const options = must(fitCalls().at(-1), 'a fitView call')
    expect([...idsOf(options)].sort()).toEqual([...clusterIds()].sort())
    expect(reservesCorner(options)).toBe(true)
  })

  it('Re-fit frames the cluster targets and clears the Re-fit corner', () => {
    render(<AgentWeb islands={[island('a', 3), island('b', 2)]} />)
    fitView.mockClear()
    fireEvent.click(screen.getByRole('button', { name: 'Re-fit' }))
    expect(fitView).toHaveBeenCalledTimes(1)
    const options = must(fitCalls()[0], 'a fitView call')
    expect([...idsOf(options)].sort()).toEqual([...clusterIds()].sort())
    expect(reservesCorner(options)).toBe(true)
  })

  it('a cluster zoom frames that cluster target only, clear of the Re-fit corner', () => {
    render(<AgentWeb islands={[island('a', 3), island('b', 2)]} />)
    fitView.mockClear()
    fireEvent.click(screen.getByRole('button', { name: /^Zoom to cluster.*role-b-1/ }))
    expect(fitView).toHaveBeenCalledTimes(1)
    const options = must(fitCalls()[0], 'a fitView call')
    const bId = must(
      clusterIds().find((id) => id.endsWith(':b')),
      'cluster b id',
    )
    expect(idsOf(options)).toEqual([bId])
    expect(reservesCorner(options)).toBe(true)
  })

  it('keeps cluster targets apart by their larger size when islands are packed', () => {
    render(<AgentWeb islands={[island('a', 4), island('b', 3), island('c', 2)]} />)
    const rects = requiredRects(['a', 'b', 'c']).sort((p, q) => p.left - q.left)
    expect(rects).toHaveLength(3)
    for (let i = 1; i < rects.length; i += 1) {
      const cur = must(rects[i], `rect ${i}`)
      const prev = must(rects[i - 1], `rect ${i - 1}`)
      expect(cur.left).toBeGreaterThanOrEqual(prev.right)
    }
    // The old rule left only ISLAND_GAP_PX between island bounds, less than the targets' growth.
    expect(2 * (LABEL_SIDE_PX + CLUSTER_PAD_PX)).toBeGreaterThan(ISLAND_GAP_PX)
  })

  it('keeps targets apart when a left island grows into its neighbour', () => {
    const { rerender } = render(<AgentWeb islands={[island('a', 2), island('b', 2)]} />)
    rerender(<AgentWeb islands={[island('a', 6), island('b', 2)]} />)
    const rects = requiredRects(['a', 'b']).sort((p, q) => p.left - q.left)
    expect(rects).toHaveLength(2)
    expect(must(rects[1], 'rect 1').left).toBeGreaterThanOrEqual(must(rects[0], 'rect 0').right)
  })
})
