import { render, screen, within } from '@testing-library/react'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { resetFlow } from '../../tests/xyflow-mock'
import { AgentsPanel } from './AgentsPanel'
import { DENSE_NODE_COUNT, webCounts } from './graph/rules'
import type { GraphAgent, Island } from './types'

vi.mock('@xyflow/react', async (importOriginal) => ({
  ...(await importOriginal<typeof import('@xyflow/react')>()),
  ...(await import('../../tests/xyflow-mock')),
}))

vi.mock('../api', () => ({ focusTerminal: vi.fn() }))

const island = (sessionId: string, states: GraphAgent['state'][], depthOf = 0): Island => {
  const agents: GraphAgent[] = states.map((state, i) => ({
    id: i + 1,
    parent: i === 0 ? null : 1,
    kind: i === 0 ? 'orchestrator' : 'subagent',
    role: `role-${sessionId}-${i + 1}`,
    chargeKey: null,
    name: null,
    state,
    costUsd: null,
    contextPct: null,
    depth: i === 0 ? 0 : i === 1 ? depthOf : 1,
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

beforeEach(() => {
  resetFlow()
})

describe('AgentsPanel', () => {
  it('is the "Agents" region with the live count and an empty state without islands', () => {
    render(<AgentsPanel islands={[]} liveAgents={0} />)
    const region = screen.getByRole('region', { name: 'Agents' })
    expect(within(region).getByText('0')).toBeInTheDocument()
    expect(within(region).getByText('No agents running')).toBeInTheDocument()
    expect(within(region).queryByTestId('flow')).toBeNull()
  })

  it('shows the empty state when every island is empty', () => {
    render(<AgentsPanel islands={[island('a', [])]} liveAgents={0} />)
    const region = screen.getByRole('region', { name: 'Agents' })
    expect(within(region).getByText('No agents running')).toBeInTheDocument()
    expect(within(region).queryByTestId('flow')).toBeNull()
  })

  it('renders the web and the live count in the normal look', () => {
    render(<AgentsPanel islands={[island('a', ['working', 'done'])]} liveAgents={7} />)
    const region = screen.getByRole('region', { name: 'Agents' })
    expect(within(region).getByText('7')).toBeInTheDocument()
    expect(within(region).getByTestId('flow')).toBeInTheDocument()
    expect(within(region).getByRole('button', { name: /^role-a-1/ })).toBeInTheDocument()
    expect(within(region).queryByText('No agents running')).toBeNull()
    expect(within(region).queryByText(/\btotal\b/)).toBeNull()
  })

  it('shows the total, working, finished and depth counts in the dense look', () => {
    const states: GraphAgent['state'][] = [
      'working',
      ...Array.from({ length: 3 }, () => 'working' as const),
      ...Array.from({ length: 2 }, () => 'done' as const),
      ...Array.from({ length: DENSE_NODE_COUNT - 5 }, () => 'idle' as const),
    ]
    const a = island('a', states, 4)
    const b = island('b', ['working', 'done'])
    const { total, working, finished, depth } = webCounts([a, b])
    expect(total).toBeGreaterThan(DENSE_NODE_COUNT)
    render(<AgentsPanel islands={[a, b]} liveAgents={3} />)
    const region = screen.getByRole('region', { name: 'Agents' })
    expect(within(region).getByText(new RegExp(`${total} total`))).toBeInTheDocument()
    expect(within(region).getByText(new RegExp(`${working} working`))).toBeInTheDocument()
    expect(within(region).getByText(new RegExp(`${finished} finished`))).toBeInTheDocument()
    expect(within(region).getByText(new RegExp(`${depth} deep`))).toBeInTheDocument()
  })
})
