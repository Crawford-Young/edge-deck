import { beforeEach, describe, expect, it, vi } from 'vitest'
import {
  focusTerminal,
  githubState,
  obsState,
  onGithubState,
  onObsState,
  type GithubState,
  type ObsState,
  type SessionKind,
} from './api'

const { invoke, listen } = vi.hoisted(() => ({ invoke: vi.fn(), listen: vi.fn() }))

vi.mock('@tauri-apps/api/core', () => ({ invoke }))
vi.mock('@tauri-apps/api/event', () => ({ listen }))

beforeEach(() => {
  invoke.mockReset()
  listen.mockReset()
})

describe('obsState', () => {
  it('asks the backend for the current OBS state', async () => {
    const state: ObsState = { connection: 'connected', streaming: true }
    invoke.mockResolvedValueOnce(state)
    await expect(obsState()).resolves.toEqual(state)
    expect(invoke).toHaveBeenCalledWith('obs_state')
  })
})

describe('focusTerminal', () => {
  it('invokes focus_terminal with the session fields as camelCase args', async () => {
    invoke.mockResolvedValueOnce(undefined)
    await expect(
      focusTerminal({
        pid: 4242,
        sessionId: 'sess-abc_1',
        title: 'fix the parser',
        kind: 'background',
      }),
    ).resolves.toBeUndefined()
    expect(invoke).toHaveBeenCalledWith('focus_terminal', {
      pid: 4242,
      sessionId: 'sess-abc_1',
      title: 'fix the parser',
      kind: 'background',
    })
  })

  it('passes interactive sessions through unchanged', async () => {
    invoke.mockResolvedValueOnce(undefined)
    const kind: SessionKind = 'interactive'
    await focusTerminal({ pid: 7, sessionId: 's', title: '', kind })
    expect(invoke).toHaveBeenCalledWith('focus_terminal', {
      pid: 7,
      sessionId: 's',
      title: '',
      kind: 'interactive',
    })
  })

  it('rejects with the backend error message', async () => {
    invoke.mockRejectedValueOnce('terminal not found')
    await expect(
      focusTerminal({ pid: 1, sessionId: 's', title: 't', kind: 'interactive' }),
    ).rejects.toBe('terminal not found')
  })
})

describe('onObsState', () => {
  it('subscribes to obs-state and hands each payload to the callback', async () => {
    const unlisten = vi.fn()
    listen.mockResolvedValueOnce(unlisten)
    const seen: ObsState[] = []

    const stop = await onObsState((state) => seen.push(state))

    expect(listen).toHaveBeenCalledWith('obs-state', expect.any(Function))
    const [, handler] = listen.mock.lastCall as [string, (event: { payload: ObsState }) => void]
    handler({ payload: { connection: 'auth-failed', streaming: false } })
    handler({ payload: { connection: 'off', streaming: false } })
    expect(seen).toEqual([
      { connection: 'auth-failed', streaming: false },
      { connection: 'off', streaming: false },
    ])
    expect(stop).toBe(unlisten)
  })
})

const sample: GithubState = {
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
      repo: 'octo/widgets',
      number: 9,
      title: 'Gizmo is slow',
      url: 'https://example.test/octo/widgets/issues/9',
    },
  ],
}

describe('githubState', () => {
  it('asks the backend for the current GitHub state', async () => {
    invoke.mockResolvedValueOnce(sample)
    await expect(githubState()).resolves.toEqual(sample)
    expect(invoke).toHaveBeenCalledWith('github_state')
  })
})

describe('onGithubState', () => {
  it('subscribes to github-state and hands each payload to the callback', async () => {
    const unlisten = vi.fn()
    listen.mockResolvedValueOnce(unlisten)
    const seen: GithubState[] = []

    const stop = await onGithubState((state) => seen.push(state))

    expect(listen).toHaveBeenCalledWith('github-state', expect.any(Function))
    const [, handler] = listen.mock.lastCall as [string, (event: { payload: GithubState }) => void]
    handler({ payload: sample })
    handler({ payload: { status: 'signed-out', prs: [], issues: [] } })
    expect(seen).toEqual([sample, { status: 'signed-out', prs: [], issues: [] }])
    expect(stop).toBe(unlisten)
  })
})
