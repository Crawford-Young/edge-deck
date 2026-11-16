import { act, renderHook } from '@testing-library/react'
import { StrictMode } from 'react'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import type { CodeSnapshot } from '../api'
import * as api from '../api'
import { useCodeSnapshot } from './useCodeSnapshot'

vi.mock('../api', () => ({ codeSnapshot: vi.fn(), onCodeChanged: vi.fn() }))

let changed: () => void
let unlisten: ReturnType<typeof vi.fn>

const snap = (needYou: number): CodeSnapshot => ({
  graphs: [],
  needsYou: [],
  header: { startedAtMs: null, sessionCostUsd: null, todayUsd: 0, liveAgents: 0, needYou },
})

async function mount(wrapper?: typeof StrictMode) {
  const r = renderHook(() => useCodeSnapshot(), wrapper ? { wrapper } : undefined)
  await act(async () => {})
  return r
}

beforeEach(() => {
  vi.resetAllMocks()
  unlisten = vi.fn()
  vi.mocked(api.onCodeChanged).mockImplementation((fn) => {
    changed = fn
    return Promise.resolve(unlisten as never)
  })
  vi.mocked(api.codeSnapshot).mockResolvedValue(snap(0))
})

afterEach(() => {
  vi.useRealTimers()
})

describe('useCodeSnapshot', () => {
  it('has no snapshot and no error before the first read lands', () => {
    vi.mocked(api.codeSnapshot).mockReturnValue(new Promise(() => {}))
    const { result } = renderHook(() => useCodeSnapshot())
    expect(result.current.snapshot).toBeNull()
    expect(result.current.error).toBeNull()
  })

  it('reads on mount', async () => {
    vi.mocked(api.codeSnapshot).mockResolvedValue(snap(3))
    const { result } = await mount()
    expect(result.current.snapshot).toEqual(snap(3))
    expect(result.current.error).toBeNull()
  })

  it('re-reads on every code-changed event', async () => {
    const { result } = await mount()
    expect(api.codeSnapshot).toHaveBeenCalledTimes(1)
    vi.mocked(api.codeSnapshot).mockResolvedValue(snap(1))
    await act(async () => changed())
    expect(api.codeSnapshot).toHaveBeenCalledTimes(2)
    expect(result.current.snapshot).toEqual(snap(1))
    vi.mocked(api.codeSnapshot).mockResolvedValue(snap(2))
    await act(async () => changed())
    expect(api.codeSnapshot).toHaveBeenCalledTimes(3)
    expect(result.current.snapshot).toEqual(snap(2))
  })

  it('issues the first read only after the listener registration has resolved', async () => {
    let resolve!: (f: () => void) => void
    vi.mocked(api.onCodeChanged).mockImplementation(
      (fn) =>
        new Promise((r) => {
          changed = fn
          resolve = r
        }),
    )
    renderHook(() => useCodeSnapshot())
    await act(async () => {})
    expect(api.onCodeChanged).toHaveBeenCalledTimes(1)
    expect(api.codeSnapshot).not.toHaveBeenCalled()
    await act(async () => resolve(unlisten as never))
    expect(api.codeSnapshot).toHaveBeenCalledTimes(1)
  })

  it('reads again for a change fired right after registration resolves', async () => {
    let resolve!: (f: () => void) => void
    vi.mocked(api.onCodeChanged).mockImplementation(
      (fn) =>
        new Promise((r) => {
          changed = fn
          resolve = r
        }),
    )
    const { result } = renderHook(() => useCodeSnapshot())
    await act(async () => resolve(unlisten as never))
    vi.mocked(api.codeSnapshot).mockResolvedValue(snap(8))
    await act(async () => changed())
    expect(api.codeSnapshot).toHaveBeenCalledTimes(2)
    expect(result.current.snapshot).toEqual(snap(8))
  })

  it('uses no timer or interval', async () => {
    vi.useFakeTimers()
    await mount()
    expect(vi.getTimerCount()).toBe(0)
    await act(async () => {
      vi.advanceTimersByTime(60_000)
    })
    expect(api.codeSnapshot).toHaveBeenCalledTimes(1)
  })

  it('surfaces a failed read as an error, not a swallowed one', async () => {
    vi.mocked(api.codeSnapshot).mockRejectedValue('the Code store did not open')
    const { result } = await mount()
    expect(result.current.error).toContain('the Code store did not open')
    expect(result.current.snapshot).toBeNull()
  })

  it('surfaces an Error rejection by its message', async () => {
    vi.mocked(api.codeSnapshot).mockRejectedValue(new Error('store unreadable'))
    const { result } = await mount()
    expect(result.current.error).toContain('store unreadable')
  })

  it('clears the error when a later read succeeds', async () => {
    vi.mocked(api.codeSnapshot).mockRejectedValueOnce('boom')
    const { result } = await mount()
    expect(result.current.error).not.toBeNull()
    vi.mocked(api.codeSnapshot).mockResolvedValue(snap(4))
    await act(async () => changed())
    expect(result.current.error).toBeNull()
    expect(result.current.snapshot).toEqual(snap(4))
  })

  it('never lets a stale in-flight read overwrite a newer one', async () => {
    const resolvers: Array<(s: CodeSnapshot) => void> = []
    vi.mocked(api.codeSnapshot).mockImplementation(() => new Promise((r) => resolvers.push(r)))
    const { result } = await mount()
    await act(async () => changed())
    expect(resolvers).toHaveLength(2)
    await act(async () => resolvers[1]?.(snap(2)))
    await act(async () => resolvers[0]?.(snap(1)))
    expect(result.current.snapshot).toEqual(snap(2))
  })

  it('does not let a stale failed read replace a newer snapshot with an error', async () => {
    const settle: Array<{ ok: (s: CodeSnapshot) => void; no: (e: unknown) => void }> = []
    vi.mocked(api.codeSnapshot).mockImplementation(
      () => new Promise((ok, no) => settle.push({ ok, no })),
    )
    const { result } = await mount()
    await act(async () => changed())
    await act(async () => settle[1]?.ok(snap(5)))
    await act(async () => settle[0]?.no('late failure'))
    expect(result.current.snapshot).toEqual(snap(5))
    expect(result.current.error).toBeNull()
  })

  it('unsubscribes on unmount', async () => {
    const { unmount } = await mount()
    unmount()
    await act(async () => {})
    expect(unlisten).toHaveBeenCalledTimes(1)
  })

  it('unsubscribes when unmounted before the subscription resolves', async () => {
    let resolve!: (f: () => void) => void
    vi.mocked(api.onCodeChanged).mockImplementation(() => new Promise((r) => (resolve = r)))
    const { unmount } = renderHook(() => useCodeSnapshot())
    unmount()
    resolve(unlisten as never)
    await act(async () => {})
    expect(unlisten).toHaveBeenCalledTimes(1)
  })

  it('ignores a read that lands after unmount', async () => {
    let resolve!: (s: CodeSnapshot) => void
    vi.mocked(api.codeSnapshot).mockImplementation(() => new Promise((r) => (resolve = r)))
    const { result, unmount } = await mount()
    unmount()
    resolve(snap(9))
    await act(async () => {})
    expect(result.current.snapshot).toBeNull()
  })

  it('does not leak listeners under StrictMode', async () => {
    const { result } = await mount(StrictMode)
    expect(api.onCodeChanged).toHaveBeenCalledTimes(2)
    expect(unlisten).toHaveBeenCalledTimes(1)
    vi.mocked(api.codeSnapshot).mockResolvedValue(snap(6))
    await act(async () => changed())
    expect(result.current.snapshot).toEqual(snap(6))
  })
})
