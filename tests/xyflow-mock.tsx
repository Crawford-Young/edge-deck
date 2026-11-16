// A stand-in for React Flow's renderer. happy-dom has no layout, so the real
// canvas never measures or draws nodes. This one renders each node through its
// custom node component with its position as data attributes, lists the edges,
// and records fitView / setCenter calls and the flow's props. The zoom it
// reports is settable, so a test can drive the tap-size rule.
// Use it as `vi.mock('@xyflow/react', ...)` spread over the real module, which
// keeps the enums (Position) real.
import type { HandleProps, ReactFlowProps } from '@xyflow/react'
import { useLayoutEffect, useRef, type ComponentType, type ReactNode, type Ref } from 'react'
import { vi } from 'vitest'

const renders: ReactFlowProps[] = []

let zoom = 1

/** Set the zoom the flow reports until the next `resetFlow`. */
export const setZoom = (value: number) => {
  zoom = value
}

/** The props the flow last rendered with. */
export const flowProps = (): ReactFlowProps => renders.at(-1) ?? {}

/** Every `setCenter` call: where the view was centred, at what zoom. */
export const centerCalls: { x: number; y: number; zoom?: number }[] = []

/** Every `fitView` call, as the flow's own re-fit would make it. */
export const fitView = vi.fn<(options?: unknown) => Promise<boolean>>(() => Promise.resolve(true))

export function resetFlow() {
  fitView.mockClear()
  centerCalls.length = 0
  renders.length = 0
  zoom = 1
}

// One instance, as the real hook returns a stable one. getZoom reads the live value.
const flow = {
  getZoom: () => zoom,
  setCenter: (x: number, y: number, options?: { zoom?: number }) =>
    Promise.resolve(centerCalls.push({ x, y, zoom: options?.zoom }) > 0),
  setViewport: () => Promise.resolve(true),
  fitView,
}

export const useReactFlow = () => flow

export const useViewport = () => ({ x: 0, y: 0, zoom })

export const useStore = <T,>(selector: (state: { transform: [number, number, number] }) => T): T =>
  selector({ transform: [0, 0, zoom] })

export function ReactFlowProvider({ children }: { children?: ReactNode }) {
  return <>{children}</>
}

/** React Flow's overlay slot: renders its children in place. */
export function Panel({ children, className }: { children?: ReactNode; className?: string }) {
  return <div className={className}>{children}</div>
}

/** React Flow's in-viewport slot: renders its children in place. */
export function ViewportPortal({ children }: { children?: ReactNode }) {
  return <>{children}</>
}

type NodeComponent = ComponentType<{
  id: string
  data: unknown
  positionAbsoluteX: number
  positionAbsoluteY: number
}>

/** A node's measured size, as the real flow reports it once per mounted node. */
export const MEASURED = { width: 192, height: 74 }

/**
 * Contracts of the real flow kept here:
 * - Its `ref` is the root element.
 * - It measures each node once and reports the size through `onNodesChange`.
 * - A click on a node reaches `onNodeClick`.
 */
export function ReactFlow({ ref, ...rest }: ReactFlowProps & { ref?: Ref<HTMLDivElement> }) {
  const props: ReactFlowProps = rest
  const reported = useRef(new Set<string>())
  const { nodes = [], nodeTypes = {}, children, onNodesChange, onNodeClick } = props
  useLayoutEffect(() => {
    renders.push(props)
    const fresh = nodes.filter((n) => !reported.current.has(n.id))
    fresh.forEach((n) => reported.current.add(n.id))
    if (fresh.length > 0) {
      onNodesChange?.(
        fresh.map((n) => ({ id: n.id, type: 'dimensions', dimensions: { ...MEASURED } })),
      )
    }
  })
  return (
    <div ref={ref} data-testid="flow" className="react-flow">
      {nodes.map((n) => {
        const Node = nodeTypes[n.type as string] as unknown as NodeComponent
        return (
          <div
            key={n.id}
            data-testid={`node-${n.id}`}
            data-x={n.position.x}
            data-y={n.position.y}
            onClick={(e) => onNodeClick?.(e, n)}
          >
            <Node
              id={n.id}
              data={n.data}
              positionAbsoluteX={n.position.x}
              positionAbsoluteY={n.position.y}
            />
          </div>
        )
      })}
      {children}
    </div>
  )
}

export function Handle({ type, position, isConnectable }: HandleProps) {
  return (
    <span
      data-testid={`handle-${type}`}
      data-position={position}
      data-connectable={String(isConnectable)}
    />
  )
}
