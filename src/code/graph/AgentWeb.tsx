import { Button } from '@crawfordyoung/ui'
import {
  applyNodeChanges,
  Panel,
  ReactFlow,
  ReactFlowProvider,
  useReactFlow,
  type Edge,
  type Node,
  type NodeChange,
  type NodeMouseHandler,
  type NodeProps,
  type NodeTypes,
} from '@xyflow/react'
import { useState } from 'react'
import { focusTerminal } from '../../api'
import type { Island } from '../types'
import { AgentNode, agentLabel, type AgentFlowNode } from './AgentNode'
import { layoutWeb, type IslandLayout, type Point } from './layout'
import {
  childCounts,
  DENSE_MIN_NODE_PX,
  isDense,
  isTappable,
  MIN_TAP_PX,
  nodeKey,
  nodeSizePx,
  webCounts,
} from './rules'

/** Space between an island's layout bounds and its cluster target's edge, px. */
export const CLUSTER_PAD_PX = 48
/** A too-small tap zooms this much past the size at which the node becomes tappable. */
export const TAP_ZOOM_MARGIN = 1.1
/** Fit, cluster and tap-zoom animation length, ms. */
export const VIEW_ANIMATION_MS = 300
/** Room fitView leaves around what it frames, as a share of the view. */
export const FIT_PADDING = 0.1
/** Far enough out to frame a large run. */
export const MIN_ZOOM = 0.1

type ClusterNodeData = { label: string; members: readonly string[] }
type ClusterFlowNode = Node<ClusterNodeData, 'cluster'>
type WebNode = AgentFlowNode | ClusterFlowNode

/** An island's tap target behind its nodes: tapping it frames that island. */
function ClusterNode({ data }: NodeProps<ClusterFlowNode>) {
  return (
    <button
      type="button"
      aria-label={`Zoom to cluster ${data.label}`}
      className="size-full rounded-3xl border border-border bg-surface-raised/30"
    />
  )
}

const NODE_TYPES: NodeTypes = { agent: AgentNode, cluster: ClusterNode }

const NODE_ORIGIN: [number, number] = [0.5, 0.5]

// The smallest dense node, zoomed to tappable, still fits under the ceiling.
const MAX_ZOOM = (MIN_TAP_PX / DENSE_MIN_NODE_PX) * TAP_ZOOM_MARGIN

const clusterId = (sessionId: string): string => `cluster:${sessionId.length}:${sessionId}`

/** Only what moves nodes: membership, sizes, edges and the look. */
const layoutKey = (islands: readonly Island[], dense: boolean): string =>
  JSON.stringify([
    dense,
    islands.map((i) => [
      i.sessionId,
      i.agents.map((a) => [a.id, a.depth]),
      i.edges.map((e) => [e.parent, e.child]),
    ]),
  ])

const clusterLabel = (island: Island): string | null => {
  const root = island.agents.find((a) => a.depth === 0) ?? island.agents[0]
  if (!root) return null
  return root.chargeKey === null ? agentLabel(root) : `${agentLabel(root)}, ${root.chargeKey}`
}

/**
 * Every node, cluster targets first so they sit behind. Positions come from
 * `layout` when given; otherwise each node keeps its current one.
 */
const buildNodes = (
  islands: readonly Island[],
  dense: boolean,
  current: readonly WebNode[],
  layout: readonly IslandLayout[] | null,
): WebNode[] => {
  const old = new Map(current.map((n) => [n.id, n]))
  const clusters: ClusterFlowNode[] = []
  const agents: AgentFlowNode[] = []
  islands.forEach((island, i) => {
    const counts = childCounts(island)
    const members = island.agents.map((a) => nodeKey(island.sessionId, a.id))
    const label = clusterLabel(island)
    const id = clusterId(island.sessionId)
    const placed = layout?.[i]
    if (label !== null) {
      const box = placed && {
        position: {
          x: placed.bounds.x + placed.bounds.width / 2,
          y: placed.bounds.y + placed.bounds.height / 2,
        },
        width: placed.bounds.width + CLUSTER_PAD_PX * 2,
        height: placed.bounds.height + CLUSTER_PAD_PX * 2,
      }
      clusters.push({
        ...old.get(id),
        ...box,
        id,
        type: 'cluster',
        data: { label, members },
        draggable: false,
        selectable: false,
        focusable: false,
        zIndex: -1,
      } as ClusterFlowNode)
    }
    island.agents.forEach((agent, j) => {
      const key = members[j] as string
      const kept = old.get(key)
      const position = placed?.positions.get(key) ?? (kept?.position as Point)
      agents.push({
        ...kept,
        id: key,
        type: 'agent',
        position,
        data: { agent, dense, childCount: counts.get(agent.id) as number },
        focusable: false,
      } as AgentFlowNode)
    })
  })
  return [...clusters, ...agents]
}

const buildEdges = (islands: readonly Island[]): Edge[] =>
  islands.flatMap((island) => {
    const ids = new Set(island.agents.map((a) => a.id))
    return island.edges
      .filter((e) => ids.has(e.parent) && ids.has(e.child))
      .map((e) => {
        const source = nodeKey(island.sessionId, e.parent)
        const target = nodeKey(island.sessionId, e.child)
        return {
          id: `${source}->${target}`,
          source,
          target,
          type: 'straight',
          focusable: false,
          selectable: false,
          deletable: false,
        }
      })
  })

interface WebState {
  islands: readonly Island[]
  key: string
  nodes: WebNode[]
  edges: Edge[]
}

const lay = (islands: readonly Island[], current: readonly WebNode[]): WebState => {
  const dense = isDense(webCounts(islands).total)
  const previous = new Map<string, Point>(
    current.filter((n) => n.type === 'agent').map((n) => [n.id, n.position]),
  )
  const layout = layoutWeb(islands, { previous, dense })
  return {
    islands,
    key: layoutKey(islands, dense),
    nodes: buildNodes(islands, dense, current, layout),
    edges: buildEdges(islands),
  }
}

function Web({ islands }: { islands: readonly Island[] }) {
  const { fitView, getZoom, setCenter } = useReactFlow()
  const [state, setState] = useState<WebState>(() => lay(islands, []))
  const [failed, setFailed] = useState(false)

  // A new snapshot: refresh node data; lay out again only when the shape changed.
  if (state.islands !== islands) {
    const dense = isDense(webCounts(islands).total)
    setState(
      layoutKey(islands, dense) === state.key
        ? { ...state, islands, nodes: buildNodes(islands, dense, state.nodes, null) }
        : lay(islands, state.nodes),
    )
  }

  const onNodesChange = (changes: NodeChange<WebNode>[]) =>
    setState((s) => ({ ...s, nodes: applyNodeChanges(changes, s.nodes) }))

  const agentIds = state.nodes.filter((n) => n.type === 'agent').map((n) => ({ id: n.id }))

  const onNodeClick: NodeMouseHandler<WebNode> = (_event, node) => {
    if (node.type === 'cluster') {
      void fitView({
        nodes: node.data.members.map((id) => ({ id })),
        padding: FIT_PADDING,
        duration: VIEW_ANIMATION_MS,
      })
      return
    }
    const { agent, dense } = (node as AgentFlowNode).data
    const size = nodeSizePx(agent.depth, dense)
    if (!isTappable(size, getZoom())) {
      void setCenter(node.position.x, node.position.y, {
        zoom: (MIN_TAP_PX / size) * TAP_ZOOM_MARGIN,
        duration: VIEW_ANIMATION_MS,
      })
      return
    }
    if (agent.focus === null) return
    focusTerminal(agent.focus).then(
      () => setFailed(false),
      () => setFailed(true),
    )
  }

  return (
    <ReactFlow<WebNode>
      nodes={state.nodes}
      edges={state.edges}
      nodeTypes={NODE_TYPES}
      nodeOrigin={NODE_ORIGIN}
      onNodesChange={onNodesChange}
      onNodeClick={onNodeClick}
      nodesConnectable={false}
      nodesFocusable={false}
      edgesFocusable={false}
      edgesReconnectable={false}
      elementsSelectable={false}
      deleteKeyCode={null}
      zoomOnDoubleClick={false}
      minZoom={MIN_ZOOM}
      maxZoom={MAX_ZOOM}
      colorMode="dark"
      fitView
      fitViewOptions={{ padding: FIT_PADDING }}
      proOptions={{ hideAttribution: true }}
      className="[--xy-background-color:transparent] [--xy-edge-stroke:theme(colors.muted.foreground/50%)]"
    >
      <Panel position="top-right">
        <Button
          type="button"
          variant="outline"
          size="touch"
          onClick={() =>
            void fitView({ nodes: agentIds, padding: FIT_PADDING, duration: VIEW_ANIMATION_MS })
          }
        >
          Re-fit
        </Button>
      </Panel>
      {failed && (
        <Panel position="bottom-left">
          <p
            role="alert"
            className="rounded-lg border border-border bg-surface px-4 py-2 text-xl text-destructive"
          >
            Could not bring that terminal forward
          </p>
        </Panel>
      )}
    </ReactFlow>
  )
}

/** The live agent web: one React Flow over every island. */
export function AgentWeb({ islands }: { islands: readonly Island[] }) {
  return (
    <ReactFlowProvider>
      <Web islands={islands} />
    </ReactFlowProvider>
  )
}
