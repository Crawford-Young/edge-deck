import { act, render, screen } from '@testing-library/react'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { Header } from './Header'
import type { HeaderData } from './types'

const NOW = new Date(2030, 0, 15, 12, 0, 0).getTime()

const data = (over: Partial<HeaderData> = {}): HeaderData => ({
  startedAt: NOW - 45_000,
  sessionUsd: 1.2,
  todayUsd: 12,
  liveAgents: 3,
  needYou: 0,
  ...over,
})

beforeEach(() => {
  vi.useFakeTimers()
  vi.setSystemTime(NOW)
})

afterEach(() => {
  vi.useRealTimers()
})

describe('Header', () => {
  it('says "No live session" when there is no data', () => {
    render(<Header data={null} />)
    expect(screen.getByText('No live session')).toBeInTheDocument()
    expect(screen.queryByText(/agent/)).toBeNull()
  })

  it('shows running time as h:mm:ss', () => {
    render(<Header data={data({ startedAt: NOW - (3600 + 125) * 1000 })} />)
    expect(screen.getByText('1:02:05')).toBeInTheDocument()
    expect(screen.queryByText('No live session')).toBeNull()
  })

  it('ticks the running time each second', () => {
    render(<Header data={data({ startedAt: NOW - 5_000 })} />)
    expect(screen.getByText('0:00:05')).toBeInTheDocument()
    act(() => {
      vi.advanceTimersByTime(3_000)
    })
    expect(screen.getByText('0:00:08')).toBeInTheDocument()
  })

  it('shows dollars with two decimals for session and today', () => {
    render(<Header data={data({ sessionUsd: 1.2, todayUsd: 12 })} />)
    expect(screen.getByText('$1.20')).toBeInTheDocument()
    expect(screen.getByText('$12.00')).toBeInTheDocument()
  })

  it('pluralises agents', () => {
    const { rerender } = render(<Header data={data({ liveAgents: 3 })} />)
    expect(screen.getByText('3 agents')).toBeInTheDocument()
    rerender(<Header data={data({ liveAgents: 1 })} />)
    expect(screen.getByText('1 agent')).toBeInTheDocument()
    rerender(<Header data={data({ liveAgents: 0 })} />)
    expect(screen.getByText('0 agents')).toBeInTheDocument()
  })

  it('shows the "N need you" chip only when N > 0', () => {
    const { rerender } = render(<Header data={data({ needYou: 0 })} />)
    expect(screen.queryByText(/need you/)).toBeNull()
    rerender(<Header data={data({ needYou: 2 })} />)
    expect(screen.getByText('2 need you')).toBeInTheDocument()
  })

  it('clears its timer on unmount', () => {
    const { unmount } = render(<Header data={data()} />)
    unmount()
    expect(vi.getTimerCount()).toBe(0)
  })
})
