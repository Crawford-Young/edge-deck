import { render, screen, within } from '@testing-library/react'
import { describe, expect, it } from 'vitest'
import { Panel } from './Panel'

describe('Panel', () => {
  it('is a region named by its title, holding the body', () => {
    render(
      <Panel title="Things">
        <p>body text</p>
      </Panel>,
    )
    const region = screen.getByRole('region', { name: 'Things' })
    expect(within(region).getByText('body text')).toBeInTheDocument()
  })

  it('renders the count in the title row when given', () => {
    render(
      <Panel title="Things" count={<span>7 here</span>}>
        <p>x</p>
      </Panel>,
    )
    expect(
      within(screen.getByRole('region', { name: 'Things' })).getByText('7 here'),
    ).toBeInTheDocument()
  })

  it('renders without a count', () => {
    render(
      <Panel title="Things">
        <p>x</p>
      </Panel>,
    )
    expect(screen.getByRole('region', { name: 'Things' })).toBeInTheDocument()
  })
})
