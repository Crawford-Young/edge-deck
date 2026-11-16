import { act, fireEvent, render, screen, within } from '@testing-library/react'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { focusTerminal } from '../api'
import { NeedsYouPanel } from './NeedsYouPanel'
import type { Wait } from './types'

vi.mock('../api', () => ({ focusTerminal: vi.fn() }))

const NOW = new Date(2030, 0, 15, 12, 0, 0).getTime()

const focus = (sessionId: string) => ({
  pid: 4321,
  sessionId,
  kind: 'interactive' as const,
  title: `title ${sessionId}`,
})

const wait = (
  label: string,
  ageSec: number | null,
  waitingFor: string | null = `answer ${label}`,
  withFocus: ReturnType<typeof focus> | null = focus(label),
): Wait => ({
  label,
  waitingFor,
  sinceMs: ageSec === null ? null : NOW - ageSec * 1000,
  focus: withFocus,
})

beforeEach(() => {
  vi.useFakeTimers()
  vi.setSystemTime(NOW)
  vi.mocked(focusTerminal).mockReset()
  vi.mocked(focusTerminal).mockResolvedValue(undefined)
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
    render(<NeedsYouPanel waits={[wait('Planner', 10), wait('Builder', 20)]} />)
    const region = screen.getByRole('region', { name: 'Needs you' })
    expect(within(region).getByText('2')).toBeInTheDocument()
    expect(within(region).queryByText('Nothing needs you')).toBeNull()
  })

  it('shows label and what it waits for', () => {
    render(<NeedsYouPanel waits={[wait('Planner', 10, 'approve the plan')]} />)
    expect(screen.getByText('Planner')).toBeInTheDocument()
    expect(screen.getByText('approve the plan')).toBeInTheDocument()
  })

  it('shows a placeholder, not null text, for a missing waitingFor or sinceMs', () => {
    const { container } = render(<NeedsYouPanel waits={[wait('Planner', null, null)]} />)
    expect(container.textContent).not.toMatch(/NaN|undefined|null/)
    expect(screen.getAllByText('—').length).toBeGreaterThanOrEqual(1)
  })

  it('lists the longest wait first regardless of input order', () => {
    render(<NeedsYouPanel waits={[wait('Middle', 100), wait('Newest', 5), wait('Oldest', 900)]} />)
    const [oldest, middle, newest] = ['Oldest', 'Middle', 'Newest'].map((n) => screen.getByText(n))
    const follows = (a: Node | undefined, b: Node | undefined) =>
      a && b ? (a.compareDocumentPosition(b) & Node.DOCUMENT_POSITION_FOLLOWING) !== 0 : false
    expect(follows(oldest, middle)).toBe(true)
    expect(follows(middle, newest)).toBe(true)
  })

  it('puts a wait with no start time last', () => {
    render(<NeedsYouPanel waits={[wait('Unknown', null), wait('Known', 5)]} />)
    const known = screen.getByText('Known')
    const unknown = screen.getByText('Unknown')
    expect(known.compareDocumentPosition(unknown) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy()
  })

  it('does not mutate the input array order', () => {
    const waits = [wait('Newest', 5), wait('Oldest', 900)]
    render(<NeedsYouPanel waits={waits} />)
    expect(waits.map((w) => w.label)).toEqual(['Newest', 'Oldest'])
  })

  it.each([
    [45, '45s'],
    [185, '3m 05s'],
    [3720, '1h 02m'],
  ])('formats an age of %is as %s', (sec, text) => {
    render(<NeedsYouPanel waits={[wait('Planner', sec)]} />)
    expect(screen.getByText(text)).toBeInTheDocument()
  })

  it('ticks the age each second', () => {
    render(<NeedsYouPanel waits={[wait('Planner', 58)]} />)
    expect(screen.getByText('58s')).toBeInTheDocument()
    act(() => {
      vi.advanceTimersByTime(3_000)
    })
    expect(screen.getByText('1m 01s')).toBeInTheDocument()
  })

  describe('row tap', () => {
    it('makes a row with focus a button named for the agent, at least 96px tall', () => {
      render(<NeedsYouPanel waits={[wait('Planner', 10)]} />)
      const button = screen.getByRole('button', { name: /Planner/ })
      // 96px = Tailwind min-h-24 (happy-dom has no layout; real size is live QA, unverified).
      expect(button.className).toMatch(/\bmin-h-24\b|\bh-24\b|\bmin-h-\[(9[6-9]|[1-9]\d{2,})px\]/)
    })

    it('calls focusTerminal with the row focus when tapped', () => {
      const f = focus('s-1')
      render(<NeedsYouPanel waits={[wait('Planner', 10, 'approve', f)]} />)
      fireEvent.click(screen.getByRole('button', { name: /Planner/ }))
      expect(focusTerminal).toHaveBeenCalledTimes(1)
      expect(focusTerminal).toHaveBeenCalledWith(f)
    })

    it('focuses the tapped row, not another', () => {
      render(<NeedsYouPanel waits={[wait('Planner', 10), wait('Builder', 20)]} />)
      fireEvent.click(screen.getByRole('button', { name: /Builder/ }))
      expect(focusTerminal).toHaveBeenCalledWith(focus('Builder'))
    })

    it('renders a row with null focus as plain text, not a button, and tapping does nothing', () => {
      render(<NeedsYouPanel waits={[wait('Ghost', 10, 'approve', null)]} />)
      expect(screen.queryByRole('button')).toBeNull()
      fireEvent.click(screen.getByText('Ghost'))
      expect(focusTerminal).not.toHaveBeenCalled()
    })

    it('shows a short inline alert, and does not throw, when focus fails', async () => {
      vi.mocked(focusTerminal).mockRejectedValue('terminal not found')
      render(<NeedsYouPanel waits={[wait('Planner', 10)]} />)
      expect(screen.queryByRole('alert')).toBeNull()
      fireEvent.click(screen.getByRole('button', { name: /Planner/ }))
      await act(async () => {})
      expect(screen.getByRole('alert')).toBeInTheDocument()
      expect(screen.getByRole('button', { name: /Planner/ })).toBeInTheDocument()
    })

    it('shows the failure alert for an Error rejection too', async () => {
      vi.mocked(focusTerminal).mockRejectedValue(new Error('no tab'))
      render(<NeedsYouPanel waits={[wait('Planner', 10)]} />)
      fireEvent.click(screen.getByRole('button', { name: /Planner/ }))
      await act(async () => {})
      expect(screen.getByRole('alert')).toBeInTheDocument()
    })

    it('clears the alert after a later tap succeeds', async () => {
      vi.mocked(focusTerminal).mockRejectedValueOnce('nope')
      render(<NeedsYouPanel waits={[wait('Planner', 10)]} />)
      fireEvent.click(screen.getByRole('button', { name: /Planner/ }))
      await act(async () => {})
      expect(screen.getByRole('alert')).toBeInTheDocument()
      fireEvent.click(screen.getByRole('button', { name: /Planner/ }))
      await act(async () => {})
      expect(screen.queryByRole('alert')).toBeNull()
    })
  })
})
