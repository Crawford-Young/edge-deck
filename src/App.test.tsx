import { act, fireEvent, render, screen, within } from '@testing-library/react'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { App } from './App'
import type { CodeSnapshot, ObsState } from './api'
import * as api from './api'

vi.mock('./api', () => ({
  obsState: vi.fn(),
  onObsState: vi.fn(),
  githubState: vi.fn(),
  onGithubState: vi.fn(),
  codeSnapshot: vi.fn(),
  onCodeChanged: vi.fn(),
  focusTerminal: vi.fn(),
}))

let cb: (s: ObsState) => void
let codeChanged: () => void

const live = (): CodeSnapshot => ({
  graphs: [],
  needsYou: [
    {
      label: 'Planner',
      waitingFor: 'approve the plan',
      sinceMs: Date.now() - 30_000,
      focus: { pid: 11, sessionId: 'sess-a', kind: 'interactive', title: 'demo' },
    },
  ],
  header: {
    startedAtMs: Date.now() - 60_000,
    sessionCostUsd: 2.5,
    todayUsd: 7,
    liveAgents: 4,
    needYou: 1,
  },
})
const ev = (streaming: boolean): ObsState => ({ connection: 'connected', streaming })

async function mount() {
  render(<App />)
  await act(async () => {})
}

beforeEach(() => {
  localStorage.clear()
  vi.mocked(api.onObsState).mockImplementation((fn) => {
    cb = fn
    return Promise.resolve((() => {}) as never)
  })
  vi.mocked(api.obsState).mockResolvedValue({ connection: 'off', streaming: false })
  vi.mocked(api.onGithubState).mockResolvedValue((() => {}) as never)
  vi.mocked(api.githubState).mockResolvedValue({ status: 'loading', prs: [], issues: [] })
  vi.mocked(api.onCodeChanged).mockImplementation((fn) => {
    codeChanged = fn
    return Promise.resolve((() => {}) as never)
  })
  vi.mocked(api.codeSnapshot).mockReturnValue(new Promise(() => {}))
  vi.mocked(api.focusTerminal).mockReset()
  vi.mocked(api.focusTerminal).mockResolvedValue(undefined)
})

describe('App', () => {
  it('shows the Code setup by default, beside the rail', async () => {
    await mount()
    expect(screen.getByRole('navigation', { name: 'Setups' })).toBeInTheDocument()
    expect(screen.getByRole('region', { name: 'Needs you' })).toBeInTheDocument()
    expect(screen.queryByRole('heading', { name: 'edge-deck' })).toBeNull()
  })

  it('Code setup shows the three panels in order and an h1 named Code', async () => {
    await mount()
    const needs = screen.getByRole('region', { name: 'Needs you' })
    const agents = screen.getByRole('region', { name: 'Agents' })
    const github = screen.getByRole('region', { name: 'GitHub' })
    expect(needs.compareDocumentPosition(agents) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy()
    expect(agents.compareDocumentPosition(github) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy()
    expect(screen.getByRole('heading', { level: 1, name: 'Code' })).toBeInTheDocument()
    expect(screen.getByText('No live session')).toBeInTheDocument()
    expect(screen.getByText('Nothing needs you')).toBeInTheDocument()
    expect(screen.getByText('No agents running')).toBeInTheDocument()
    expect(screen.getByText('Loading GitHub…')).toBeInTheDocument()
  })

  it('tapping Stream shows the Stream setup', async () => {
    await mount()
    fireEvent.click(screen.getByRole('radio', { name: 'Stream' }))
    expect(screen.getByRole('heading', { level: 1, name: 'Stream' })).toBeInTheDocument()
    expect(screen.getByRole('radio', { name: 'Stream' })).toBeChecked()
  })

  it('re-tapping the selected setup keeps it selected', async () => {
    await mount()
    fireEvent.click(screen.getByRole('radio', { name: 'Code' }))
    expect(screen.getByRole('radio', { name: 'Code' })).toBeChecked()
    expect(screen.getByRole('region', { name: 'Agents' })).toBeInTheDocument()
  })

  it('switches to Stream while OBS streams and back when it stops', async () => {
    await mount()
    act(() => cb(ev(true)))
    expect(screen.getByRole('heading', { level: 1, name: 'Stream' })).toBeInTheDocument()
    expect(screen.getByRole('radio', { name: 'Stream' })).toBeChecked()
    expect(screen.queryByRole('region', { name: 'Needs you' })).toBeNull()
    act(() => cb(ev(false)))
    expect(screen.getByRole('region', { name: 'Needs you' })).toBeInTheDocument()
  })

  it('fills the header, needs-you panel and agents count from the code snapshot', async () => {
    vi.mocked(api.codeSnapshot).mockResolvedValue(live())
    await mount()
    expect(screen.queryByText('No live session')).toBeNull()
    expect(screen.getByText('$2.50')).toBeInTheDocument()
    expect(screen.getByText('1 need you')).toBeInTheDocument()
    const needs = screen.getByRole('region', { name: 'Needs you' })
    expect(within(needs).getByText('approve the plan')).toBeInTheDocument()
    expect(
      within(screen.getByRole('region', { name: 'Agents' })).getByText('4'),
    ).toBeInTheDocument()
  })

  it('tapping a needs-you row focuses that session', async () => {
    vi.mocked(api.codeSnapshot).mockResolvedValue(live())
    await mount()
    fireEvent.click(screen.getByRole('button', { name: /Planner/ }))
    expect(api.focusTerminal).toHaveBeenCalledWith({
      pid: 11,
      sessionId: 'sess-a',
      kind: 'interactive',
      title: 'demo',
    })
  })

  it('refetches on code-changed', async () => {
    vi.mocked(api.codeSnapshot).mockResolvedValue(live())
    await mount()
    vi.mocked(api.codeSnapshot).mockResolvedValue({
      graphs: [],
      needsYou: [],
      header: { startedAtMs: null, sessionCostUsd: null, todayUsd: 0, liveAgents: 0, needYou: 0 },
    })
    await act(async () => codeChanged())
    expect(screen.getByText('Nothing needs you')).toBeInTheDocument()
    expect(screen.queryByText('1 need you')).toBeNull()
  })

  it('shows a failed code read as an alert', async () => {
    vi.mocked(api.codeSnapshot).mockRejectedValue('the Code store did not open')
    await mount()
    expect(screen.getByRole('alert')).toHaveTextContent('the Code store did not open')
  })
})
