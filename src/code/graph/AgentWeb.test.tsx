import type { NodeChange } from '@xyflow/react'
import { act, fireEvent, render, screen } from '@testing-library/react'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { centerCalls, fitView, flowProps, resetFlow, setZoom } from '../../../tests/xyflow-mock'
import { focusTerminal, type FocusTarget } from '../../api'
import type { GraphAgent, Island } from '../types'
import { AgentWeb } from './AgentWeb'
import { layoutWeb } from './layout'
import { DENSE_NODE_COUNT, MIN_TAP_PX, nodeKey, nodeSizePx } from './rules'

vi.mock('@xyflow/react', async (importOriginal) => ({
  ...(await importOriginal<typeof import('@xyflow/react')>()),
  ...(await import('../../../tests/xyflow-mock')),
}))

vi.mock('../../api', () => ({ focusTerminal: vi.fn() }))

vi.mock('./layout', async (importOriginal) => {
  const real = await importOriginal<typeof import('./layout')>()
  return { ...real, layoutWeb: vi.fn(real.layoutWeb) }
})

const focus = (pid: number): FocusTarget => ({
  pid,
  sessionId: `uuid-${pid}`,
  title: `t${pid}`,
  kind: 'interactive',
})

/** A root (id 1) plus `count - 1` children of the root; roles are unique per node. */
const island = (sessionId: string, count: number, over: Partial<GraphAgent> = {}): Island => {
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
    ...over,
  }))
  return {
    sessionId,
    startedAtMs: null,
    sessionCostUsd: null,
    agents,
    edges: agents.slice(1).map((a) => ({ parent: 1, child: a.id, kind: 'spawn' })),
  }
}

const withAgent = (isl: Island, id: number, over: Partial<GraphAgent>): Island => ({
  ...isl,
  agents: isl.agents.map((a) => (a.id === id ? { ...a, ...over } : a)),
})

const nodeButton = (sessionId: string, id: number): HTMLElement => {
  // The ring button carries data-size; a cluster target does not.
  const found = screen
    .getAllByRole('button', { name: new RegExp(`\\brole-${sessionId}-${id}\\b`) })
    .filter((el) => el.hasAttribute('data-size'))
  const [only] = found
  if (found.length !== 1 || !only)
    throw new Error(`expected one node button, found ${found.length}`)
  return only
}

const clusterButtons = (): HTMLElement[] =>
  screen.getAllByRole('button', { name: /^Zoom to cluster/ })

const fitIds = (call: unknown[] | undefined): string[] | undefined => {
  const options = call?.[0] as { nodes?: { id: string }[] } | undefined
  return options?.nodes?.map((n) => n.id)
}

const lastFit = (): unknown[] => {
  const call = fitView.mock.calls.at(-1)
  if (!call) throw new Error('fitView was not called')
  return call
}

const keys = (isl: Island): string[] => isl.agents.map((a) => nodeKey(isl.sessionId, a.id))

const nodeEl = (key: string): HTMLElement => screen.getByTestId(`node-${key}`)

beforeEach(() => {
  resetFlow()
  vi.mocked(layoutWeb).mockClear()
  vi.mocked(focusTerminal).mockReset()
  vi.mocked(focusTerminal).mockResolvedValue(undefined)
})

afterEach(() => {
  vi.useRealTimers()
})

describe('AgentWeb surface', () => {
  it('renders one node per agent keyed by nodeKey and spawn edges between them', () => {
    const a = island('a', 3)
    const b = island('b', 2)
    render(<AgentWeb islands={[a, b]} />)
    const nodes = (flowProps().nodes ?? []).map((n) => n.id)
    for (const key of [...keys(a), ...keys(b)]) expect(nodes).toContain(key)
    expect(nodeEl(nodeKey('a', 2))).toBeInTheDocument()
    const edges = flowProps().edges ?? []
    expect(edges).toHaveLength(3)
    expect(edges.map((e) => [e.source, e.target])).toEqual(
      expect.arrayContaining([
        [nodeKey('a', 1), nodeKey('a', 2)],
        [nodeKey('a', 1), nodeKey('a', 3)],
        [nodeKey('b', 1), nodeKey('b', 2)],
      ]),
    )
  })

  it('keeps edges non-focusable and non-selectable', () => {
    render(<AgentWeb islands={[island('a', 3)]} />)
    for (const e of flowProps().edges ?? []) {
      expect(e.focusable).toBe(false)
      expect(e.selectable).toBe(false)
    }
  })

  it('is not connectable, editable or deletable, but nodes drag and the view pans and zooms', () => {
    render(<AgentWeb islands={[island('a', 3)]} />)
    const p = flowProps()
    expect(p.nodesConnectable).toBe(false)
    expect(p.deleteKeyCode ?? null).toBeNull()
    expect(p.nodesDraggable).not.toBe(false)
    expect(p.panOnDrag).not.toBe(false)
    expect(p.zoomOnPinch).not.toBe(false)
    expect(p.colorMode).toBe('dark')
    expect(p.fitView).toBe(true)
  })

  it('sets no timers (no polling)', () => {
    vi.useFakeTimers()
    const first = [island('a', 3)]
    const { rerender } = render(<AgentWeb islands={first} />)
    rerender(<AgentWeb islands={[island('a', 4)]} />)
    expect(vi.getTimerCount()).toBe(0)
  })
})

describe('AgentWeb look', () => {
  it('uses the normal look at the dense threshold', () => {
    render(<AgentWeb islands={[island('a', DENSE_NODE_COUNT)]} />)
    expect(nodeButton('a', 1)).toHaveAttribute('data-size', 'root')
    expect(nodeButton('a', 2)).toHaveAttribute('data-size', 'default')
    expect(vi.mocked(layoutWeb).mock.calls[0]?.[1]?.dense ?? false).toBe(false)
  })

  it('uses the dense look one past the threshold, counting across islands', () => {
    const half = Math.floor(DENSE_NODE_COUNT / 2)
    render(<AgentWeb islands={[island('a', half), island('b', DENSE_NODE_COUNT - half + 1)]} />)
    expect(nodeButton('a', 1)).toHaveAttribute('data-size', 'dense')
    expect(nodeButton('b', 2)).toHaveAttribute('data-size', 'dense')
    expect(vi.mocked(layoutWeb).mock.calls[0]?.[1]?.dense).toBe(true)
  })
})

describe('AgentWeb layout', () => {
  it('lays out once, and again only when islands changes, keeping dragged positions', () => {
    const a = island('a', 3)
    const { rerender } = render(<AgentWeb islands={[a]} />)
    expect(layoutWeb).toHaveBeenCalledTimes(1)
    expect(vi.mocked(layoutWeb).mock.calls[0]?.[0]).toEqual([a])

    const dragged: NodeChange[] = [
      { id: nodeKey('a', 2), type: 'position', position: { x: 999, y: -7 }, dragging: true },
    ]
    act(() => flowProps().onNodesChange?.(dragged))
    expect(nodeEl(nodeKey('a', 2))).toHaveAttribute('data-x', '999')

    rerender(<AgentWeb islands={[a]} />)
    expect(layoutWeb).toHaveBeenCalledTimes(1)

    const grown = island('a', 4)
    rerender(<AgentWeb islands={[grown]} />)
    expect(layoutWeb).toHaveBeenCalledTimes(2)
    const previous = vi.mocked(layoutWeb).mock.calls[1]?.[1]?.previous
    expect(previous?.get(nodeKey('a', 2))).toEqual({ x: 999, y: -7 })
    expect(nodeEl(nodeKey('a', 2))).toHaveAttribute('data-x', '999')
    expect(nodeEl(nodeKey('a', 4))).toBeInTheDocument()
  })
})

describe('AgentWeb re-fit and clusters', () => {
  it('Re-fit fits the view to every node of every island', () => {
    const a = island('a', 3)
    const b = island('b', 2)
    render(<AgentWeb islands={[a, b]} />)
    fitView.mockClear()
    fireEvent.click(screen.getByRole('button', { name: 'Re-fit' }))
    expect(fitView).toHaveBeenCalledTimes(1)
    const ids = fitIds(lastFit())
    if (ids) expect(ids).toEqual(expect.arrayContaining([...keys(a), ...keys(b)]))
  })

  it('each island has a "Zoom to cluster" target named for its root and charge', () => {
    render(<AgentWeb islands={[island('a', 3), island('b', 2)]} />)
    const buttons = clusterButtons()
    expect(buttons).toHaveLength(2)
    expect(screen.getByRole('button', { name: /^Zoom to cluster.*role-a-1/ })).toBeInTheDocument()
    expect(screen.getByRole('button', { name: /^Zoom to cluster.*repo-a#1/ })).toBeInTheDocument()
    expect(screen.getByRole('button', { name: /^Zoom to cluster.*role-b-1/ })).toBeInTheDocument()
  })

  it('a cluster name falls back to the root name, and omits an unknown charge', () => {
    const a = withAgent(island('a', 2), 1, { role: null, name: 'lead-agent', chargeKey: null })
    render(<AgentWeb islands={[a]} />)
    const cluster = screen.getByRole('button', { name: /^Zoom to cluster/ })
    expect(cluster).toHaveAccessibleName(/lead-agent/)
    expect(cluster).not.toHaveAccessibleName(/null|undefined/)
  })

  it('tapping a cluster fits the view to that island only', () => {
    const a = island('a', 3)
    const b = island('b', 2)
    render(<AgentWeb islands={[a, b]} />)
    fitView.mockClear()
    fireEvent.click(screen.getByRole('button', { name: /^Zoom to cluster.*role-b-1/ }))
    expect(fitView).toHaveBeenCalledTimes(1)
    const ids = fitIds(lastFit())
    expect(ids).toBeDefined()
    expect(ids).toEqual(expect.arrayContaining(keys(b)))
    for (const key of keys(a)) expect(ids).not.toContain(key)
  })
})

describe('AgentWeb node tap', () => {
  it('focuses the agent when it is big enough to tap', () => {
    const f = focus(11)
    render(<AgentWeb islands={[withAgent(island('a', 3), 2, { focus: f })]} />)
    expect(nodeSizePx(1, false)).toBeGreaterThanOrEqual(MIN_TAP_PX)
    fireEvent.click(nodeButton('a', 2))
    expect(focusTerminal).toHaveBeenCalledTimes(1)
    expect(focusTerminal).toHaveBeenCalledWith(f)
    expect(centerCalls).toHaveLength(0)
  })

  it('does nothing when the agent has no focus', () => {
    render(<AgentWeb islands={[island('a', 3)]} />)
    fireEvent.click(nodeButton('a', 2))
    expect(focusTerminal).not.toHaveBeenCalled()
    expect(centerCalls).toHaveLength(0)
  })

  it('shows an alert when the focus is rejected, and clears it on the next success', async () => {
    const f = focus(12)
    render(<AgentWeb islands={[withAgent(island('a', 3), 2, { focus: f })]} />)
    vi.mocked(focusTerminal).mockRejectedValueOnce(new Error('no terminal'))
    fireEvent.click(nodeButton('a', 2))
    expect(await screen.findByRole('alert')).toBeInTheDocument()
    fireEvent.click(nodeButton('a', 2))
    await act(() => Promise.resolve())
    expect(screen.queryByRole('alert')).toBeNull()
  })

  it('a dense node below the tap size zooms toward it instead of focusing', () => {
    const f = focus(13)
    const dense = island('a', DENSE_NODE_COUNT + 1)
    render(<AgentWeb islands={[withAgent(dense, 2, { focus: f, depth: 3 })]} />)
    const size = nodeSizePx(3, true)
    expect(size).toBeLessThan(MIN_TAP_PX)
    fireEvent.click(nodeButton('a', 2))
    expect(focusTerminal).not.toHaveBeenCalled()
    expect(centerCalls).toHaveLength(1)
    const call = centerCalls[0]
    expect(call?.zoom).toBeDefined()
    expect((call?.zoom ?? 0) * size).toBeGreaterThanOrEqual(MIN_TAP_PX)
    const el = nodeEl(nodeKey('a', 2))
    expect(Math.abs((call?.x ?? NaN) - Number(el.dataset.x))).toBeLessThanOrEqual(size)
    expect(Math.abs((call?.y ?? NaN) - Number(el.dataset.y))).toBeLessThanOrEqual(size)
  })

  it('a dense node is tappable once the zoom brings it to the tap size', () => {
    const f = focus(14)
    const dense = island('a', DENSE_NODE_COUNT + 1)
    setZoom(MIN_TAP_PX / nodeSizePx(3, true) + 0.01)
    render(<AgentWeb islands={[withAgent(dense, 2, { focus: f, depth: 3 })]} />)
    fireEvent.click(nodeButton('a', 2))
    expect(focusTerminal).toHaveBeenCalledWith(f)
    expect(centerCalls).toHaveLength(0)
  })

  it('a normal node zoomed far out is not tappable and zooms toward it', () => {
    const f = focus(15)
    setZoom(0.4)
    render(<AgentWeb islands={[withAgent(island('a', 3), 2, { focus: f })]} />)
    fireEvent.click(nodeButton('a', 2))
    expect(focusTerminal).not.toHaveBeenCalled()
    expect(centerCalls).toHaveLength(1)
  })
})
