import { act, render, screen, within } from '@testing-library/react'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { NeedsYouPanel } from './NeedsYouPanel'
import type { Wait } from './types'

const NOW = new Date(2030, 0, 15, 12, 0, 0).getTime()

const wait = (id: string, label: string, ageSec: number, waitsFor = `answer ${id}`): Wait => ({
  id,
  label,
  waitsFor,
  since: NOW - ageSec * 1000,
})

beforeEach(() => {
  vi.useFakeTimers()
  vi.setSystemTime(NOW)
})

afterEach(() => {
  vi.useRealTimers()
})

describe('NeedsYouPanel', () => {
  it('is the "Needs you" region with an empty state', () => {
    render(<NeedsYouPanel waits={[]} />)
    const region = screen.getByRole('region', { name: 'Needs you' })
    expect(within(region).getByText('Nothing needs you')).toBeInTheDocument()
  })

  it('shows the number of waits as the count and hides the empty state', () => {
    render(<NeedsYouPanel waits={[wait('a', 'Planner', 10), wait('b', 'Builder', 20)]} />)
    const region = screen.getByRole('region', { name: 'Needs you' })
    expect(within(region).getByText('2')).toBeInTheDocument()
    expect(within(region).queryByText('Nothing needs you')).toBeNull()
  })

  it('shows label and what it waits for', () => {
    render(<NeedsYouPanel waits={[wait('a', 'Planner', 10, 'approve the plan')]} />)
    expect(screen.getByText('Planner')).toBeInTheDocument()
    expect(screen.getByText('approve the plan')).toBeInTheDocument()
  })

  it('lists the longest wait first regardless of input order', () => {
    render(
      <NeedsYouPanel
        waits={[wait('m', 'Middle', 100), wait('n', 'Newest', 5), wait('o', 'Oldest', 900)]}
      />,
    )
    const [oldest, middle, newest] = ['Oldest', 'Middle', 'Newest'].map((n) => screen.getByText(n))
    const follows = (a: Node | undefined, b: Node | undefined) =>
      a && b ? (a.compareDocumentPosition(b) & Node.DOCUMENT_POSITION_FOLLOWING) !== 0 : false
    expect(follows(oldest, middle)).toBe(true)
    expect(follows(middle, newest)).toBe(true)
  })

  it('does not mutate the input array order', () => {
    const waits = [wait('n', 'Newest', 5), wait('o', 'Oldest', 900)]
    render(<NeedsYouPanel waits={waits} />)
    expect(waits.map((w) => w.id)).toEqual(['n', 'o'])
  })

  it.each([
    [45, '45s'],
    [185, '3m 05s'],
    [3720, '1h 02m'],
  ])('formats an age of %is as %s', (sec, text) => {
    render(<NeedsYouPanel waits={[wait('a', 'Planner', sec)]} />)
    expect(screen.getByText(text)).toBeInTheDocument()
  })

  it('ticks the age each second', () => {
    render(<NeedsYouPanel waits={[wait('a', 'Planner', 58)]} />)
    expect(screen.getByText('58s')).toBeInTheDocument()
    act(() => {
      vi.advanceTimersByTime(3_000)
    })
    expect(screen.getByText('1m 01s')).toBeInTheDocument()
  })
})
