import { act, render, screen } from '@testing-library/react'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { Clock } from './Clock'

const at = (h: number, m: number, s = 0) => new Date(2030, 0, 15, h, m, s)

beforeEach(() => {
  vi.useFakeTimers()
})

afterEach(() => {
  vi.useRealTimers()
})

describe('Clock', () => {
  it('shows the time over AM, with an ISO dateTime and no seconds', () => {
    vi.setSystemTime(at(9, 5, 30))
    render(<Clock />)
    const time = screen.getByRole('time')
    expect(screen.getByText('9:05')).toBeInTheDocument()
    expect(screen.getByText('AM')).toBeInTheDocument()
    expect(time.textContent).not.toMatch(/30/)
    expect(time).toHaveAttribute('dateTime', at(9, 5, 30).toISOString())
  })

  it.each([
    [0, 0, '12:00', 'AM'],
    [11, 59, '11:59', 'AM'],
    [12, 0, '12:00', 'PM'],
    [22, 49, '10:49', 'PM'],
    [23, 59, '11:59', 'PM'],
  ])('shows %i:%i as %s %s', (h, m, clock, period) => {
    vi.setSystemTime(at(h, m))
    render(<Clock />)
    expect(screen.getByText(clock)).toBeInTheDocument()
    expect(screen.getByText(period)).toBeInTheDocument()
  })

  it('updates when the minute changes', () => {
    vi.setSystemTime(at(9, 5, 30))
    render(<Clock />)
    act(() => {
      vi.advanceTimersByTime(31_000)
    })
    expect(screen.getByText('9:06')).toBeInTheDocument()
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
    expect(screen.getByText('9:07')).toBeInTheDocument()
  })

  it('clears its timer on unmount', () => {
    vi.setSystemTime(at(9, 5, 30))
    const { unmount } = render(<Clock />)
    expect(vi.getTimerCount()).toBe(1)
    unmount()
    expect(vi.getTimerCount()).toBe(0)
    expect(screen.queryByText('9:05')).toBeNull()
  })
})
