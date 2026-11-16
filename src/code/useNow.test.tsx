import { act, renderHook } from '@testing-library/react'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { useNow } from './useNow'

beforeEach(() => {
  vi.useFakeTimers()
  vi.setSystemTime(1_000_000)
})

afterEach(() => {
  vi.useRealTimers()
})

describe('useNow', () => {
  it('returns the current time and advances each second', () => {
    const { result } = renderHook(() => useNow())
    expect(result.current).toBe(1_000_000)
    act(() => {
      vi.advanceTimersByTime(2_000)
    })
    expect(result.current).toBe(1_002_000)
  })

  it('stops ticking on unmount', () => {
    const { unmount } = renderHook(() => useNow())
    unmount()
    expect(vi.getTimerCount()).toBe(0)
  })
})
