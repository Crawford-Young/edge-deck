import { render, screen, within } from '@testing-library/react'
import { describe, expect, it } from 'vitest'
import { AgentsPanel } from './AgentsPanel'

describe('AgentsPanel', () => {
  it('is the "Agents" region with the count and an empty state without children', () => {
    render(<AgentsPanel agentCount={0} />)
    const region = screen.getByRole('region', { name: 'Agents' })
    expect(within(region).getByText('0')).toBeInTheDocument()
    expect(within(region).getByText('No agents running')).toBeInTheDocument()
  })

  it('renders children instead of the empty state', () => {
    render(
      <AgentsPanel agentCount={4}>
        <p>the graph</p>
      </AgentsPanel>,
    )
    const region = screen.getByRole('region', { name: 'Agents' })
    expect(within(region).getByText('4')).toBeInTheDocument()
    expect(within(region).getByText('the graph')).toBeInTheDocument()
    expect(within(region).queryByText('No agents running')).toBeNull()
  })
})
