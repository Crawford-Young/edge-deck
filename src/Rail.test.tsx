import { fireEvent, render, screen } from '@testing-library/react'
import { describe, expect, it, vi } from 'vitest'
import { Rail } from './Rail'

describe('Rail', () => {
  it('is a navigation landmark named Setups with a Stream and a Code control', () => {
    render(<Rail selected="code" onSelect={() => {}} />)
    const nav = screen.getByRole('navigation', { name: 'Setups' })
    expect(nav).toContainElement(screen.getByRole('radio', { name: 'Stream' }))
    expect(nav).toContainElement(screen.getByRole('radio', { name: 'Code' }))
  })

  it('marks only the selected setup as checked', () => {
    render(<Rail selected="stream" onSelect={() => {}} />)
    expect(screen.getByRole('radio', { name: 'Stream' })).toBeChecked()
    expect(screen.getByRole('radio', { name: 'Code' })).not.toBeChecked()
  })

  it('reports a tap on the other setup', () => {
    const onSelect = vi.fn()
    render(<Rail selected="code" onSelect={onSelect} />)
    fireEvent.click(screen.getByRole('radio', { name: 'Stream' }))
    expect(onSelect).toHaveBeenCalledExactlyOnceWith('stream')
  })

  it('ignores a re-tap of the selected setup', () => {
    const onSelect = vi.fn()
    render(<Rail selected="code" onSelect={onSelect} />)
    fireEvent.click(screen.getByRole('radio', { name: 'Code' }))
    expect(onSelect).not.toHaveBeenCalled()
  })

  it('shows the clock', () => {
    render(<Rail selected="code" onSelect={() => {}} />)
    expect(document.querySelector('nav time')).not.toBeNull()
  })
})
