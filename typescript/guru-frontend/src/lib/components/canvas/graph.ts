import type { Connection, Edge, Node } from '@xyflow/svelte';
import type {
	CanvasGraph,
	EntryNodeDto,
	ExitNodeDto,
	LoadBalanceNodeDto,
	PortDirectionName,
	PortKindName,
	RelayNodeDto,
	ServerDto
} from '#lib/dto/topology.js';
import { m } from '#lib/paraglide/messages.js';

/** Pure graph → Svelte Flow translation; the node components stay dumb. */

export type ProblemLevel = 'none' | 'warning' | 'error';

export type FlowNodeData =
	| { kind: 'server'; server: ServerDto; problem: ProblemLevel }
	| { kind: 'entry'; node: EntryNodeDto; problem: ProblemLevel }
	| { kind: 'relay'; node: RelayNodeDto; problem: ProblemLevel }
	| { kind: 'exit'; node: ExitNodeDto; problem: ProblemLevel }
	| { kind: 'load_balance'; node: LoadBalanceNodeDto; problem: ProblemLevel };

export type FlowNode = Node<FlowNodeData>;
export type PortIndexEntry = {
	flowNodeId: string;
	kind: PortKindName;
	direction: PortDirectionName;
};

/** Servers and nodes share one id space in Svelte Flow but not in the backend. */
export const flowNodeId = (kind: 'server' | 'node', id: string): string => `${kind}:${id}`;

export function parseFlowNodeId(flowId: string): { kind: 'server' | 'node'; id: string } {
	const separator = flowId.indexOf(':');
	const prefix = flowId.slice(0, separator);
	return { kind: prefix === 'server' ? 'server' : 'node', id: flowId.slice(separator + 1) };
}

/** What the node sheet is editing, in backend id space. */
export type SheetTarget = { kind: 'server' | 'node'; id: string };

/** A deletion the control plane refused, offered to admins as a force retry. */
export type ForceTarget = {
	kind: 'server' | 'node' | 'edge';
	id: string;
	label: string;
	message: string;
};

/** `error` beats `warning` beats `none`; a server inherits its pods' problems. */
function problemLevels(graph: CanvasGraph): Map<string, ProblemLevel> {
	const levels = new Map<string, ProblemLevel>();
	for (const problem of graph.problems) {
		if (problem.severity !== 'error' && problem.severity !== 'warning') continue;
		for (const nodeId of problem.nodeIds) {
			if (problem.severity === 'error' || levels.get(nodeId) === undefined) {
				levels.set(nodeId, problem.severity);
			}
		}
	}
	return levels;
}

function serverProblem(server: ServerDto, levels: Map<string, ProblemLevel>): ProblemLevel {
	let level: ProblemLevel = levels.get(server.id) ?? 'none';
	for (const pod of server.pods) {
		const podLevel = levels.get(pod.id);
		if (podLevel === 'error') return 'error';
		if (podLevel === 'warning') level = level === 'none' ? 'warning' : level;
	}
	return level;
}

export function buildFlowNodes(graph: CanvasGraph): FlowNode[] {
	const levels = problemLevels(graph);
	const nodes: FlowNode[] = graph.servers.map(server => ({
		id: flowNodeId('server', server.id),
		type: 'server',
		position: { x: server.x, y: server.y },
		data: { kind: 'server', server, problem: serverProblem(server, levels) }
	}));

	for (const node of graph.nodes) {
		const problem = levels.get(node.id) ?? 'none';
		nodes.push({
			id: flowNodeId('node', node.id),
			type:
				node.kind === 'entry'
					? 'entry'
					: node.kind === 'relay'
						? 'relay'
						: node.kind === 'exit'
							? 'exit'
							: 'loadBalance',
			position: { x: node.x, y: node.y },
			// The union is discriminated by the same `kind` the DTO carries.
			data: { kind: node.kind, node, problem } as FlowNodeData
		});
	}
	return nodes;
}

export function buildPortIndex(graph: CanvasGraph): Map<string, PortIndexEntry> {
	const index = new Map<string, PortIndexEntry>();
	for (const server of graph.servers) {
		const owner = flowNodeId('server', server.id);
		for (const pod of server.pods) {
			for (const port of pod.ports) {
				index.set(port.id, { flowNodeId: owner, kind: port.kind, direction: port.direction });
			}
		}
	}
	for (const node of graph.nodes) {
		const owner = flowNodeId('node', node.id);
		for (const port of node.ports) {
			index.set(port.id, { flowNodeId: owner, kind: port.kind, direction: port.direction });
		}
	}
	return index;
}

export function buildFlowEdges(graph: CanvasGraph): Edge[] {
	const index = buildPortIndex(graph);
	const edges: Edge[] = [];
	for (const edge of graph.edges) {
		const source = index.get(edge.sourcePortId);
		const target = index.get(edge.targetPortId);
		// A port outside this canvas cannot be drawn; the backend reports it too.
		if (!source || !target) continue;
		edges.push({
			id: edge.id,
			source: source.flowNodeId,
			sourceHandle: edge.sourcePortId,
			target: target.flowNodeId,
			targetHandle: edge.targetPortId,
			style:
				source.kind === 'derive_listen'
					? 'stroke: var(--canvas-port-listen); stroke-width: 2'
					: 'stroke: var(--canvas-port-destination); stroke-width: 2'
		});
	}
	return edges;
}

/**
 * Port ids already carrying an edge; a port is allowed exactly one.
 *
 * Both sources are needed. `buildFlowEdges` drops any edge whose opposite port
 * is off-canvas, so its surviving endpoint would otherwise look free; the graph
 * edges keep it occupied. The flow edges in turn carry the optimistic edge Svelte
 * Flow inserts before `onconnect`, which the graph has not been refreshed with
 * yet, so a second drag right after the first is still refused.
 */
export function connectedPortIds(graph: CanvasGraph, flowEdges: Edge[]): Set<string> {
	const used = new Set<string>();
	for (const edge of graph.edges) {
		used.add(edge.sourcePortId);
		used.add(edge.targetPortId);
	}
	for (const edge of flowEdges) {
		if (edge.sourceHandle) used.add(edge.sourceHandle);
		if (edge.targetHandle) used.add(edge.targetHandle);
	}
	return used;
}

/**
 * Mirrors `check_edges` in the control plane so a doomed drag never round-trips:
 * both ports must be known, sit on different nodes, share a kind, run
 * output → input, and still be free — a second edge on either endpoint is what
 * the backend reports as `PortOversubscribed`.
 */
export function canConnect(
	connection: Edge | Connection,
	portIndex: Map<string, PortIndexEntry>,
	graph: CanvasGraph,
	flowEdges: Edge[]
): boolean {
	const sourceHandle = connection.sourceHandle ?? '';
	const targetHandle = connection.targetHandle ?? '';
	const source = portIndex.get(sourceHandle);
	const target = portIndex.get(targetHandle);
	if (!source || !target) return false;
	if (connection.source === connection.target) return false;
	if (source.kind !== target.kind) return false;
	if (source.direction !== 'output' || target.direction !== 'input') return false;
	const used = connectedPortIds(graph, flowEdges);
	return !used.has(sourceHandle) && !used.has(targetHandle);
}

/**
 * The label of a port row. Only the two shared keys are translated; the
 * load-balance keys (`member_0`, `copy_0`, `source`) are shown verbatim because
 * they are the identifiers the control plane derives them as.
 */
export function portLabel(key: string): string {
	if (key === 'listen') return m.editor_port_listen();
	if (key === 'destination') return m.editor_port_destination();
	return key;
}

/**
 * Backend id (server, pod or standalone node) → the flow node that draws it and
 * the sheet target that edits it. A pod resolves to its server.
 */
export function buildBackendIndex(
	graph: CanvasGraph
): Map<string, { flowId: string; target: SheetTarget }> {
	const index = new Map<string, { flowId: string; target: SheetTarget }>();
	for (const server of graph.servers) {
		const entry = {
			flowId: flowNodeId('server', server.id),
			target: { kind: 'server', id: server.id } as SheetTarget
		};
		index.set(server.id, entry);
		for (const pod of server.pods) index.set(pod.id, entry);
	}
	for (const node of graph.nodes) {
		index.set(node.id, {
			flowId: flowNodeId('node', node.id),
			target: { kind: 'node', id: node.id }
		});
	}
	return index;
}
