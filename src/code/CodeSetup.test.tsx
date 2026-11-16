import { render, screen, within } from '@testing-library/react'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import type { GithubState } from '../api'
import { CodeSetup } from './CodeSetup'
import { useGithub } from './useGithub'

vi.mock('./useGithub', () => ({ useGithub: vi.fn() }))

const NOW = new Date(2030, 0, 15, 12, 0, 0).getTime()

beforeEach(() => {
  vi.useFakeTimers()
  vi.setSystemTime(NOW)
  vi.mocked(useGithub).mockReturnValue({ status: 'loading', prs: [], issues: [] })
})

afterEach(() => {
  vi.useRealTimers()
})

describe('CodeSetup', () => {
  it('renders header and the three panels in order, with empty states', () => {
    render(<CodeSetup header={null} waits={[]} agentCount={0} />)
    expect(screen.getByText('No live session')).toBeInTheDocument()
    const needs = screen.getByRole('region', { name: 'Needs you' })
    const agents = screen.getByRole('region', { name: 'Agents' })
    const github = screen.getByRole('region', { name: 'GitHub' })
    expect(needs.compareDocumentPosition(agents) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy()
    expect(agents.compareDocumentPosition(github) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy()
    expect(within(github).getByText('Loading GitHub…')).toBeInTheDocument()
    expect(screen.queryByText('GitHub panel coming soon')).toBeNull()
  })

  it('has a level-1 heading named Code', () => {
    render(<CodeSetup header={null} waits={[]} agentCount={0} />)
    expect(screen.getByRole('heading', { level: 1, name: 'Code' })).toBeInTheDocument()
  })

  it('renders the GitHub panel from the GitHub state', () => {
    const state: GithubState = {
      status: 'ok',
      prs: [
        {
          repo: 'octo/widgets',
          number: 7,
          title: 'Add a gizmo',
          url: 'https://example.test/octo/widgets/pull/7',
          draft: false,
          checks: 'failing',
        },
      ],
      issues: [
        {
          repo: 'octo/gadgets',
          number: 9,
          title: 'Gizmo is slow',
          url: 'https://example.test/octo/gadgets/issues/9',
        },
      ],
    }
    vi.mocked(useGithub).mockReturnValue(state)
    render(<CodeSetup header={null} waits={[]} agentCount={0} />)
    const github = screen.getByRole('region', { name: 'GitHub' })
    expect(within(github).getByText('Add a gizmo')).toBeInTheDocument()
    expect(within(github).getByText('Gizmo is slow')).toBeInTheDocument()
    expect(within(github).getByText('1 PR · 1 issue')).toBeInTheDocument()
  })

  it('passes data through to header, needs-you and agents panels', () => {
    render(
      <CodeSetup
        header={{ startedAt: NOW - 5_000, sessionUsd: 1.2, todayUsd: 3, liveAgents: 2, needYou: 1 }}
        waits={[{ id: 'a', label: 'Planner', waitsFor: 'approve', since: NOW - 45_000 }]}
        agentCount={2}
        agents={<p>graph here</p>}
      />,
    )
    expect(screen.getByText('$1.20')).toBeInTheDocument()
    expect(screen.getByText('1 need you')).toBeInTheDocument()
    expect(
      within(screen.getByRole('region', { name: 'Needs you' })).getByText('Planner'),
    ).toBeInTheDocument()
    const agents = screen.getByRole('region', { name: 'Agents' })
    expect(within(agents).getByText('graph here')).toBeInTheDocument()
  })
})
