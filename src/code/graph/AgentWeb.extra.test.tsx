import { render, screen } from '@testing-library/react'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { flowProps, resetFlow } from '../../../tests/xyflow-mock'
import type { GraphAgent, Island } from '../types'
import { AgentWeb } from './AgentWeb'
import { layoutWeb } from './layout'
import { nodeKey } from './rules'

vi.mock('@xyflow/react', async (importOriginal) => ({
  ...(await importOriginal<typeof import('@xyflow/react')>()),
  ...(await import('../../../tests/xyflow-mock')),
}))

vi.mock('../../api', () => ({ focusTerminal: vi.fn() }))

vi.mock('./layout', async (importOriginal) => {
  const real = await importOriginal<typeof import('./layout')>()
  return { ...real, layoutWeb: vi.fn(real.layoutWeb) }
})

const agent = (id: number, over: Partial<GraphAgent> = {}): GraphAgent => ({
  id,
  parent: id === 1 ? null : 1,
  kind: 'subagent',
  role: `worker-${id}`,
  chargeKey: null,
  name: null,
  state: 'working',
  costUsd: null,
  contextPct: null,
  depth: id === 1 ? 0 : 1,
  focus: null,
  ...over,
})

const island = (sessionId: string, agents: GraphAgent[]): Island => ({
  sessionId,
  startedAtMs: null,
  sessionCostUsd: null,
  agents,
  edges: agents
    .filter((a) => a.parent !== null)
    .map((a) => ({ parent: a.parent as number, child: a.id, kind: 'spawn' })),
})

beforeEach(() => {
  resetFlow()
  vi.mocked(layoutWeb).mockClear()
})

describe('AgentWeb clusters, edge cases', () => {
  it('gives an empty island no cluster target and no nodes', () => {
    render(<AgentWeb islands={[island('empty', []), island('full', [agent(1), agent(2)])]} />)
    const clusters = screen.getAllByRole('button', { name: /^Zoom to cluster/ })
    expect(clusters).toHaveLength(1)
    expect(clusters[0]).toHaveAccessibleName(/worker-1/)
  })

  it('names a cluster after its first agent when no agent is at depth 0', () => {
    const orphans = [agent(5, { parent: null, depth: 2 }), agent(6, { parent: 5, depth: 3 })]
    render(<AgentWeb islands={[island('a', orphans)]} />)
    expect(screen.getByRole('button', { name: /^Zoom to cluster/ })).toHaveAccessibleName(
      /worker-5/,
    )
  })
})

describe('AgentWeb refresh', () => {
  it('a state change refreshes the node without laying out again', () => {
    const before = island('a', [agent(1), agent(2)])
    const { rerender } = render(<AgentWeb islands={[before]} />)
    const x = flowProps().nodes?.find((n) => n.id === nodeKey('a', 2))?.position.x
    rerender(<AgentWeb islands={[island('a', [agent(1), agent(2, { state: 'done' })])]} />)
    expect(layoutWeb).toHaveBeenCalledTimes(1)
    expect(screen.getByRole('button', { name: /worker-2.*done/ })).toBeInTheDocument()
    const after = flowProps().nodes?.find((n) => n.id === nodeKey('a', 2))?.position.x
    expect(after).toBe(x)
  })
})
