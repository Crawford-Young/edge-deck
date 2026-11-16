import { act, render, screen } from '@testing-library/react'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { Header } from './Header'
import type { HeaderData } from './types'

const NOW = new Date(2030, 0, 15, 12, 0, 0).getTime()

const data = (over: Partial<HeaderData> = {}): HeaderData => ({
  startedAtMs: NOW - 45_000,
  sessionCostUsd: 1.2,
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

  it('shows running time from startedAtMs as h:mm:ss', () => {
    render(<Header data={data({ startedAtMs: NOW - (3600 + 125) * 1000 })} />)
    expect(screen.getByText('1:02:05')).toBeInTheDocument()
    expect(screen.queryByText('No live session')).toBeNull()
  })

  it('ticks the running time each second', () => {
    render(<Header data={data({ startedAtMs: NOW - 5_000 })} />)
    expect(screen.getByText('0:00:05')).toBeInTheDocument()
    act(() => {
      vi.advanceTimersByTime(3_000)
    })
    expect(screen.getByText('0:00:08')).toBeInTheDocument()
  })

  it('shows dollars with two decimals for session and today', () => {
    render(<Header data={data({ sessionCostUsd: 1.2, todayUsd: 12 })} />)
    expect(screen.getByText('$1.20')).toBeInTheDocument()
    expect(screen.getByText('$12.00')).toBeInTheDocument()
  })

  it('shows a placeholder, never NaN or $0.00, for an unknown start and session cost', () => {
    const { container } = render(
      <Header data={data({ startedAtMs: null, sessionCostUsd: null, todayUsd: 12 })} />,
    )
    expect(container.textContent).not.toMatch(/NaN|undefined|null/)
    expect(screen.queryByText('$0.00')).toBeNull()
    expect(screen.queryByText(/^\d+:\d\d:\d\d$/)).toBeNull()
    expect(screen.getAllByText('—')).toHaveLength(2)
    expect(screen.getByText('$12.00')).toBeInTheDocument()
  })

  it('still shows a real zero session cost as $0.00', () => {
    render(<Header data={data({ sessionCostUsd: 0, todayUsd: 5 })} />)
    expect(screen.getByText('$0.00')).toBeInTheDocument()
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
