import { act, renderHook } from '@testing-library/react'
import { StrictMode } from 'react'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import type { GithubState } from '../api'
import * as api from '../api'
import { useGithub } from './useGithub'

vi.mock('../api', () => ({ githubState: vi.fn(), onGithubState: vi.fn() }))

let cb: (s: GithubState) => void
let unlisten: ReturnType<typeof vi.fn>

const loading: GithubState = { status: 'loading', prs: [], issues: [] }
const withPr = (number: number): GithubState => ({
  status: 'ok',
  prs: [
    {
      repo: 'octo/widgets',
      number,
      title: `PR ${number}`,
      url: `https://example.test/octo/widgets/pull/${number}`,
      draft: false,
      checks: 'passing',
    },
  ],
  issues: [],
})

async function mount(wrapper?: typeof StrictMode) {
  const r = renderHook(() => useGithub(), wrapper ? { wrapper } : undefined)
  await act(async () => {})
  return r
}

beforeEach(() => {
  vi.resetAllMocks()
  unlisten = vi.fn()
  vi.mocked(api.onGithubState).mockImplementation((fn) => {
    cb = fn
    return Promise.resolve(unlisten as never)
  })
  vi.mocked(api.githubState).mockResolvedValue(loading)
})

describe('useGithub', () => {
  it('starts as loading with empty lists before anything has arrived', () => {
    vi.mocked(api.githubState).mockReturnValue(new Promise(() => {}))
    const { result } = renderHook(() => useGithub())
    expect(result.current).toEqual(loading)
  })

  it('applies the initial githubState read', async () => {
    vi.mocked(api.githubState).mockResolvedValue(withPr(1))
    const { result } = await mount()
    expect(result.current).toEqual(withPr(1))
  })

  it('applies each github-state event', async () => {
    const { result } = await mount()
    act(() => cb(withPr(2)))
    expect(result.current).toEqual(withPr(2))
    act(() => cb({ status: 'gh-missing', prs: [], issues: [] }))
    expect(result.current.status).toBe('gh-missing')
  })

  it('keeps loading when the initial read rejects', async () => {
    vi.mocked(api.githubState).mockRejectedValue(new Error('no backend'))
    const { result } = await mount()
    expect(result.current).toEqual(loading)
  })

  it('subscribes first and reads only once the subscription is in place', async () => {
    let subscribed!: (fn: () => void) => void
    vi.mocked(api.onGithubState).mockImplementation(() => new Promise((r) => (subscribed = r)))
    renderHook(() => useGithub())
    await act(async () => {})
    expect(vi.mocked(api.onGithubState)).toHaveBeenCalledTimes(1)
    expect(vi.mocked(api.githubState)).not.toHaveBeenCalled()
    subscribed(unlisten as never)
    await act(async () => {})
    expect(vi.mocked(api.githubState)).toHaveBeenCalledTimes(1)
    expect(vi.mocked(api.onGithubState).mock.invocationCallOrder[0]).toBeLessThan(
      vi.mocked(api.githubState).mock.invocationCallOrder[0] ?? 0,
    )
  })

  it('drops a stale read that lands after a newer event', async () => {
    let resolve!: (s: GithubState) => void
    vi.mocked(api.githubState).mockImplementation(() => new Promise((r) => (resolve = r)))
    const { result } = await mount()
    act(() => cb(withPr(3)))
    resolve(withPr(4))
    await act(async () => {})
    expect(result.current).toEqual(withPr(3))
  })

  it('unsubscribes on unmount', async () => {
    const { unmount } = await mount()
    unmount()
    await act(async () => {})
    expect(unlisten).toHaveBeenCalledTimes(1)
  })

  it('unsubscribes when unmounted before the subscription resolves', async () => {
    let resolve!: (f: () => void) => void
    vi.mocked(api.onGithubState).mockImplementation(() => new Promise((r) => (resolve = r)))
    const { unmount } = renderHook(() => useGithub())
    unmount()
    resolve(unlisten as never)
    await act(async () => {})
    expect(unlisten).toHaveBeenCalledTimes(1)
  })

  it('ignores a late read after unmount', async () => {
    let resolve!: (s: GithubState) => void
    vi.mocked(api.githubState).mockImplementation(() => new Promise((r) => (resolve = r)))
    const { result, unmount } = await mount()
    unmount()
    resolve(withPr(5))
    await act(async () => {})
    expect(unlisten).toHaveBeenCalledTimes(1)
    expect(result.current).toEqual(loading)
  })

  it('does not leak listeners under StrictMode', async () => {
    const { result } = await mount(StrictMode)
    expect(vi.mocked(api.onGithubState)).toHaveBeenCalledTimes(2)
    expect(unlisten).toHaveBeenCalledTimes(1)
    act(() => cb(withPr(6)))
    expect(result.current).toEqual(withPr(6))
  })
})
