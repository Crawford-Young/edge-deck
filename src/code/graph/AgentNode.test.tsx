import type { NodeProps } from '@xyflow/react'
import { render, screen } from '@testing-library/react'
import { describe, expect, it, vi } from 'vitest'
import type { GraphAgent } from '../types'
import { AgentNode, type AgentFlowNode, type AgentNodeData } from './AgentNode'
import { HUB_CHILD_COUNT, nodeSizePx } from './rules'

vi.mock('@xyflow/react', async (importOriginal) => ({
  ...(await importOriginal<typeof import('@xyflow/react')>()),
  ...(await import('../../../tests/xyflow-mock')),
}))

const agent = (over: Partial<GraphAgent> = {}): GraphAgent => ({
  id: 1,
  parent: null,
  kind: 'orchestrator',
  role: 'implementer',
  chargeKey: 'demo#7',
  name: 'agent-one',
  state: 'working',
  costUsd: 1.5,
  contextPct: 42,
  depth: 0,
  focus: null,
  ...over,
})

const renderNode = (a: GraphAgent, dense = false, childCount = 0) => {
  const data: AgentNodeData = { agent: a, dense, childCount }
  const props = { id: 'k', data } as unknown as NodeProps<AgentFlowNode>
  return render(<AgentNode {...props} />)
}

const button = (): HTMLElement => screen.getByRole('button')

/** Text a sighted user sees: everything outside `.sr-only` subtrees. */
const visibleText = (root: HTMLElement): string => {
  const walk = (node: Node): string => {
    if (node.nodeType === Node.TEXT_NODE) return node.textContent ?? ''
    if (node instanceof HTMLElement && node.classList.contains('sr-only')) return ''
    return [...node.childNodes].map(walk).join(' ')
  }
  return walk(root)
}

describe('AgentNode, normal look', () => {
  it('is a button named by role, charge and state words', () => {
    renderNode(agent())
    const named = screen.getByRole('button', { name: /implementer/ })
    expect(named).toHaveAccessibleName(/demo#7/)
    expect(named).toHaveAccessibleName(/working/)
  })

  it('shows cost to two places and context percent', () => {
    renderNode(agent({ costUsd: 12 }))
    expect(screen.getByText('$12.00')).toBeInTheDocument()
    expect(screen.getByText('42%')).toBeInTheDocument()
  })

  it('rounds cost to two places', () => {
    renderNode(agent({ costUsd: 0.5 }))
    expect(screen.getByText('$0.50')).toBeInTheDocument()
  })

  it('leaves out charge, cost and context when unknown', () => {
    renderNode(agent({ chargeKey: null, costUsd: null, contextPct: null }))
    expect(button()).not.toHaveAccessibleName(/demo#7/)
    expect(screen.queryByText(/\$/)).toBeNull()
    expect(screen.queryByText(/%/)).toBeNull()
    expect(screen.queryByRole('meter', { hidden: true })).toBeNull()
  })

  it('names the node by role, else agent name, else kind', () => {
    const { unmount } = renderNode(agent({ role: null, name: 'agent-one' }))
    expect(screen.getByRole('button', { name: /agent-one/ })).toBeInTheDocument()
    unmount()
    renderNode(agent({ role: null, name: null, kind: 'teammate' }))
    expect(screen.getByRole('button', { name: /teammate/ })).toBeInTheDocument()
  })

  it.each([
    ['working', 'working', 'working'],
    ['needs_you', 'needs you', 'needs-you'],
    ['idle', 'idle', 'idle'],
    ['done', 'done', 'done'],
  ] as const)('state %s reads "%s" and maps to the ring state %s', (state, words, ring) => {
    renderNode(agent({ state }))
    expect(button()).toHaveAccessibleName(new RegExp(words))
    expect(button()).toHaveAttribute('data-state', ring)
  })

  it('draws the root at root size and others at default size', () => {
    const { unmount } = renderNode(agent({ depth: 0 }))
    expect(button()).toHaveAttribute('data-size', 'root')
    unmount()
    renderNode(agent({ depth: 2 }))
    expect(button()).toHaveAttribute('data-size', 'default')
  })

  it('carries state by library tokens only: no inline colour or hex', () => {
    const { container } = renderNode(agent({ state: 'needs_you' }))
    expect(container.innerHTML).not.toMatch(/#[0-9a-f]{3,8}\b|rgb\(|hsl\(/i)
    for (const el of container.querySelectorAll('[style]')) {
      expect(el.getAttribute('style')).not.toMatch(/color|background|fill|stroke/i)
    }
  })
})

describe('AgentNode, dense look', () => {
  it.each([0, 1, 3, 9])(
    'depth %i is dense-sized with width and height from nodeSizePx',
    (depth) => {
      const { container } = renderNode(agent({ depth }), true)
      expect(button()).toHaveAttribute('data-size', 'dense')
      const px = `${nodeSizePx(depth, true)}px`
      const sized = [...container.querySelectorAll<HTMLElement>('[style]')].filter(
        (el) => el.style.width === px && el.style.height === px,
      )
      expect(sized.length).toBeGreaterThan(0)
    },
  )

  it('an unlabelled node shows no label text but keeps its full accessible name', () => {
    const { container } = renderNode(agent({ depth: 2, state: 'working' }), true, 0)
    const text = visibleText(container)
    expect(text).not.toMatch(/implementer/)
    expect(text).not.toMatch(/working/)
    expect(text).not.toMatch(/needs you/i)
    expect(button()).toHaveAccessibleName(/implementer/)
    expect(button()).toHaveAccessibleName(/demo#7/)
    expect(button()).toHaveAccessibleName(/working/)
  })

  it('a root shows its role', () => {
    const { container } = renderNode(agent({ depth: 0 }), true, 0)
    expect(visibleText(container)).toMatch(/implementer/)
  })

  it('a hub shows its role', () => {
    const { container } = renderNode(agent({ depth: 2 }), true, HUB_CHILD_COUNT)
    expect(visibleText(container)).toMatch(/implementer/)
  })

  it('a node just under the hub threshold is unlabelled', () => {
    const { container } = renderNode(agent({ depth: 2 }), true, HUB_CHILD_COUNT - 1)
    expect(visibleText(container)).not.toMatch(/implementer/)
  })

  it('a needs-you node shows its role and says it needs you', () => {
    const { container } = renderNode(agent({ depth: 3, state: 'needs_you' }), true, 0)
    expect(visibleText(container)).toMatch(/implementer/)
    expect(visibleText(container)).toMatch(/needs you/i)
  })

  it('a labelled non-needs-you node does not claim it needs you', () => {
    const { container } = renderNode(agent({ depth: 0, state: 'working' }), true, 0)
    expect(visibleText(container)).not.toMatch(/needs you/i)
  })
})
