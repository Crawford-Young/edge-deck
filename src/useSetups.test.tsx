import { act, renderHook } from '@testing-library/react'
import { StrictMode } from 'react'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import type { ObsState } from './api'
import * as api from './api'
import { useSetups } from './useSetups'

vi.mock('./api', () => ({ obsState: vi.fn(), onObsState: vi.fn() }))

const KEY = 'edge-deck.setup'
let cb: (s: ObsState) => void
const spies: { mockRestore: () => void }[] = []
let unlisten: ReturnType<typeof vi.fn>
const ev = (streaming: boolean): ObsState => ({ connection: 'connected', streaming })

async function mount(wrapper?: typeof StrictMode) {
  const r = renderHook(() => useSetups(), wrapper ? { wrapper } : undefined)
  await act(async () => {})
  return r
}

beforeEach(() => {
  localStorage.clear()
  unlisten = vi.fn()
  vi.mocked(api.onObsState).mockImplementation((fn) => {
    cb = fn
    return Promise.resolve(unlisten as never)
  })
  vi.mocked(api.obsState).mockResolvedValue({ connection: 'off', streaming: false })
})

afterEach(() => {
  spies.splice(0).forEach((spy) => spy.mockRestore())
  vi.restoreAllMocks()
})

describe('useSetups', () => {
  it('defaults to code when nothing is stored', async () => {
    const { result } = await mount()
    expect(result.current.selected).toBe('code')
  })

  it('restores a stored setup', async () => {
    localStorage.setItem(KEY, 'stream')
    const { result } = await mount()
    expect(result.current.selected).toBe('stream')
  })

  it('falls back to code for an unknown stored value', async () => {
    localStorage.setItem(KEY, 'bogus')
    const { result } = await mount()
    expect(result.current.selected).toBe('code')
  })

  it('falls back to code when storage read throws', async () => {
    const spy = vi.spyOn(localStorage, 'getItem').mockImplementation(() => {
      throw new Error('denied')
    })
    spies.push(spy)
    const { result } = await mount()
    expect(result.current.selected).toBe('code')
  })

  it('select persists the tapped setup', async () => {
    const { result } = await mount()
    act(() => result.current.select('stream'))
    expect(result.current.selected).toBe('stream')
    expect(localStorage.getItem(KEY)).toBe('stream')
  })

  it('swallows a throwing write', async () => {
    const spy = vi.spyOn(localStorage, 'setItem').mockImplementation(() => {
      throw new Error('full')
    })
    spies.push(spy)
    const { result } = await mount()
    act(() => result.current.select('stream'))
    expect(result.current.selected).toBe('stream')
  })

  it('auto-switches to stream while streaming without persisting, then returns', async () => {
    localStorage.setItem(KEY, 'code')
    const { result } = await mount()
    act(() => cb(ev(true)))
    expect(result.current.selected).toBe('stream')
    expect(localStorage.getItem(KEY)).toBe('code')
    act(() => cb(ev(false)))
    expect(result.current.selected).toBe('code')
  })

  it('applies the initial obsState fetch', async () => {
    vi.mocked(api.obsState).mockResolvedValue(ev(true))
    const { result } = await mount()
    expect(result.current.selected).toBe('stream')
  })

  it('ignores a rejected obsState', async () => {
    vi.mocked(api.obsState).mockRejectedValue(new Error('no backend'))
    const { result } = await mount()
    expect(result.current.selected).toBe('code')
  })

  it('unsubscribes on unmount', async () => {
    const { unmount } = await mount()
    unmount()
    await act(async () => {})
    expect(unlisten).toHaveBeenCalledTimes(1)
  })

  it('unsubscribes when unmounted before the subscription resolves', async () => {
    let resolve!: (f: () => void) => void
    vi.mocked(api.onObsState).mockImplementation(
      () => new Promise((r) => (resolve = r as typeof resolve)),
    )
    const { unmount } = renderHook(() => useSetups())
    unmount()
    resolve(unlisten as never)
    await act(async () => {})
    expect(unlisten).toHaveBeenCalledTimes(1)
  })

  it('does not leak listeners or double-apply under StrictMode', async () => {
    const { result } = await mount(StrictMode)
    expect(vi.mocked(api.onObsState)).toHaveBeenCalledTimes(2)
    expect(unlisten).toHaveBeenCalledTimes(1)
    act(() => cb(ev(true)))
    expect(result.current.selected).toBe('stream')
  })

  it('ignores a late obsState result after unmount', async () => {
    let resolve!: (s: ObsState) => void
    vi.mocked(api.obsState).mockImplementation(() => new Promise((r) => (resolve = r)))
    const { unmount } = renderHook(() => useSetups())
    unmount()
    resolve(ev(true))
    await act(async () => {})
    expect(unlisten).toHaveBeenCalledTimes(1)
  })
})
