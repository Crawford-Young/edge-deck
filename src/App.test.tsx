import { act, fireEvent, render, screen } from '@testing-library/react'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { App } from './App'
import type { ObsState } from './api'
import * as api from './api'

vi.mock('./api', () => ({
  obsState: vi.fn(),
  onObsState: vi.fn(),
  githubState: vi.fn(),
  onGithubState: vi.fn(),
}))

let cb: (s: ObsState) => void
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
})
