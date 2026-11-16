import { act, render, screen } from '@testing-library/react'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { Clock } from './Clock'

const at = (h: number, m: number, s = 0) => new Date(2030, 0, 15, h, m, s)
const text = (d: Date) => d.toLocaleTimeString([], { hour: 'numeric', minute: '2-digit' })

beforeEach(() => {
  vi.useFakeTimers()
})

afterEach(() => {
  vi.useRealTimers()
})

describe('Clock', () => {
  it('shows the local time as hours and minutes with an ISO dateTime', () => {
    vi.setSystemTime(at(9, 5, 30))
    render(<Clock />)
    const time = screen.getByText(text(at(9, 5)))
    expect(time).toHaveTextContent(text(at(9, 5)))
    expect(time.textContent).not.toMatch(/:30/)
    expect(time).toHaveAttribute('dateTime', at(9, 5, 30).toISOString())
  })

  it('updates when the minute changes', () => {
    vi.setSystemTime(at(9, 5, 30))
    render(<Clock />)
    act(() => {
      vi.advanceTimersByTime(31_000)
    })
    expect(document.querySelector('time')).toHaveTextContent(text(at(9, 6)))
  })

  it('keeps ticking minute after minute', () => {
    vi.setSystemTime(at(9, 5, 30))
    render(<Clock />)
    act(() => {
      vi.advanceTimersByTime(31_000)
    })
    act(() => {
      vi.advanceTimersByTime(60_000)
    })
    expect(document.querySelector('time')).toHaveTextContent(text(at(9, 7)))
  })

  it('clears its timer on unmount', () => {
    vi.setSystemTime(at(9, 5, 30))
    const { unmount } = render(<Clock />)
    expect(vi.getTimerCount()).toBe(1)
    unmount()
    expect(vi.getTimerCount()).toBe(0)
    expect(screen.queryByText(text(at(9, 5)))).toBeNull()
  })
})
