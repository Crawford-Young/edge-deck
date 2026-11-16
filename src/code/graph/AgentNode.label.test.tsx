import type { NodeProps } from '@xyflow/react'
import { render } from '@testing-library/react'
import { afterEach, describe, expect, it, vi } from 'vitest'
import { resetFlow, setZoom } from '../../../tests/xyflow-mock'
import type { GraphAgent } from '../types'
import { AgentNode, type AgentFlowNode, type AgentNodeData } from './AgentNode'

vi.mock('@xyflow/react', async (importOriginal) => ({
  ...(await importOriginal<typeof import('@xyflow/react')>()),
  ...(await import('../../../tests/xyflow-mock')),
}))

afterEach(resetFlow)

const root: GraphAgent = {
  id: 1,
  parent: null,
  kind: 'orchestrator',
  role: 'labelrole',
  chargeKey: null,
  name: null,
  state: 'working',
  costUsd: null,
  contextPct: null,
  depth: 0,
  focus: null,
}

const renderDense = () => {
  const data: AgentNodeData = { agent: root, dense: true, childCount: 0 }
  return render(<AgentNode {...({ id: 'k', data } as unknown as NodeProps<AgentFlowNode>)} />)
}

/** The visible label wrapper: the aria-hidden element carrying the role text. */
const label = (container: HTMLElement): HTMLElement => {
  const found = [...container.querySelectorAll<HTMLElement>('[aria-hidden="true"]')].find((el) =>
    el.textContent.includes('labelrole'),
  )
  if (!found) throw new Error('no label')
  return found
}

/** The scale factor on the label or any element inside it; 1 when none is set. */
const appliedScale = (el: HTMLElement): number => {
  for (const node of [el, ...el.querySelectorAll<HTMLElement>('[style]')]) {
    const m = /scale\(\s*([\d.]+)/.exec(node.style.transform)
    if (m) return Number(m[1])
  }
  return 1
}

describe('AgentNode dense label minimum size', () => {
  it('scales the label up at a low zoom so it is 16px on screen', () => {
    setZoom(0.2)
    const { container } = renderDense()
    expect(appliedScale(label(container))).toBeCloseTo(16 / (18 * 0.2), 4)
  })

  it('scales from the top centre so the label stays under its disc', () => {
    setZoom(0.2)
    const { container } = renderDense()
    const el = label(container)
    const scaled = [el, ...el.querySelectorAll<HTMLElement>('[style]')].find((n) =>
      /scale/.test(n.style.transform),
    )
    expect(scaled?.style.transformOrigin).toMatch(/^(top center|center top|50% 0(%|px)?|50% top)$/)
  })

  it('draws the label font at 18px', () => {
    setZoom(0.2)
    const { container } = renderDense()
    const el = label(container)
    const sized = [el, ...el.querySelectorAll<HTMLElement>('[style]')].some(
      (n) => n.style.fontSize === '18px',
    )
    expect(sized).toBe(true)
  })

  it.each([0.9, 1, 2])('applies no enlargement at zoom %f', (zoom) => {
    setZoom(zoom)
    const { container } = renderDense()
    expect(appliedScale(label(container))).toBe(1)
  })
})
