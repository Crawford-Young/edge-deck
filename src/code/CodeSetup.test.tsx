import { fireEvent, render, screen, within } from '@testing-library/react'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { focusTerminal, type CodeSnapshot, type GithubState } from '../api'
import { CodeSetup } from './CodeSetup'
import { useGithub } from './useGithub'

vi.mock('./useGithub', () => ({ useGithub: vi.fn() }))
vi.mock('../api', () => ({ focusTerminal: vi.fn() }))
vi.mock('@xyflow/react', async (importOriginal) => ({
  ...(await importOriginal<typeof import('@xyflow/react')>()),
  ...(await import('../../tests/xyflow-mock')),
}))

const NOW = new Date(2030, 0, 15, 12, 0, 0).getTime()

const snapshot = (over: Partial<CodeSnapshot> = {}): CodeSnapshot => ({
  graphs: [],
  needsYou: [],
  header: { startedAtMs: NOW - 5_000, sessionCostUsd: 1.2, todayUsd: 3, liveAgents: 2, needYou: 1 },
  ...over,
})

beforeEach(() => {
  vi.useFakeTimers()
  vi.setSystemTime(NOW)
  vi.mocked(useGithub).mockReturnValue({ status: 'loading', prs: [], issues: [] })
  vi.mocked(focusTerminal).mockReset()
  vi.mocked(focusTerminal).mockResolvedValue(undefined)
})

afterEach(() => {
  vi.useRealTimers()
})

describe('CodeSetup', () => {
  it('renders header and the three panels in order, with empty states, before a snapshot', () => {
    render(<CodeSetup snapshot={null} />)
    expect(screen.getByText('No live session')).toBeInTheDocument()
    const needs = screen.getByRole('region', { name: 'Needs you' })
    const agents = screen.getByRole('region', { name: 'Agents' })
    const github = screen.getByRole('region', { name: 'GitHub' })
    expect(needs.compareDocumentPosition(agents) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy()
    expect(agents.compareDocumentPosition(github) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy()
    expect(within(needs).getByText('Nothing needs you')).toBeInTheDocument()
    expect(within(github).getByText('Loading GitHub…')).toBeInTheDocument()
  })

  it('has a level-1 heading named Code', () => {
    render(<CodeSetup snapshot={null} />)
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
    render(<CodeSetup snapshot={null} />)
    const github = screen.getByRole('region', { name: 'GitHub' })
    expect(within(github).getByText('Add a gizmo')).toBeInTheDocument()
    expect(within(github).getByText('Gizmo is slow')).toBeInTheDocument()
    expect(within(github).getByText('1 PR · 1 issue')).toBeInTheDocument()
    expect(screen.queryByText('GitHub panel coming soon')).toBeNull()
  })

  it('passes the snapshot to the header and needs-you panel, and its graphs and live count to the agents panel', () => {
    render(
      <CodeSetup
        snapshot={snapshot({
          needsYou: [
            {
              label: 'Planner',
              waitingFor: 'approve the plan',
              sinceMs: NOW - 45_000,
              focus: null,
            },
          ],
          graphs: [
            {
              sessionId: 'sess-g',
              startedAtMs: null,
              sessionCostUsd: null,
              agents: [
                {
                  id: 1,
                  parent: null,
                  kind: 'orchestrator',
                  role: 'conductor',
                  chargeKey: null,
                  name: null,
                  state: 'working',
                  costUsd: null,
                  contextPct: null,
                  depth: 0,
                  focus: null,
                },
              ],
              edges: [],
            },
          ],
        })}
      />,
    )
    expect(screen.getByText('$1.20')).toBeInTheDocument()
    expect(screen.getByText('1 need you')).toBeInTheDocument()
    const needs = screen.getByRole('region', { name: 'Needs you' })
    expect(within(needs).getByText('Planner')).toBeInTheDocument()
    expect(within(needs).getByText('approve the plan')).toBeInTheDocument()
    const agents = screen.getByRole('region', { name: 'Agents' })
    expect(within(agents).getByText('2')).toBeInTheDocument()
    expect(within(agents).getByRole('button', { name: /^conductor/ })).toBeInTheDocument()
    expect(within(agents).queryByText('No agents running')).toBeNull()
  })

  it('a needs-you row with focus taps through to focusTerminal', () => {
    const f = { pid: 77, sessionId: 'sess-x', kind: 'interactive' as const, title: 'demo' }
    render(
      <CodeSetup
        snapshot={snapshot({
          needsYou: [{ label: 'Builder', waitingFor: 'pick one', sinceMs: NOW - 9_000, focus: f }],
        })}
      />,
    )
    fireEvent.click(screen.getByRole('button', { name: /Builder/ }))
    expect(focusTerminal).toHaveBeenCalledWith(f)
  })

  it('shows a failed snapshot read as an alert, with the message', () => {
    render(<CodeSetup snapshot={null} error="the Code store did not open" />)
    expect(screen.getByRole('alert')).toHaveTextContent('the Code store did not open')
  })

  it('shows no alert without an error', () => {
    render(<CodeSetup snapshot={snapshot()} error={null} />)
    expect(screen.queryByRole('alert')).toBeNull()
  })
})
