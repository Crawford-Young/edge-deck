import { beforeEach, describe, expect, it, vi } from 'vitest'
import { obsState, onObsState, type ObsState } from './api'

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
