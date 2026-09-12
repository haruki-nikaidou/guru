<script lang="ts">
import PlusIcon from '@lucide/svelte/icons/plus';
import {
	Background,
	Controls,
	MiniMap,
	Panel,
	SvelteFlow,
	useSvelteFlow,
	useUpdateNodeInternals,
	type Connection,
	type Edge
} from '@xyflow/svelte';
import '@xyflow/svelte/dist/style.css';
import { mode } from 'mode-watcher';
import { tick, untrack } from 'svelte';
import { toast } from 'svelte-sonner';
import {
	connectNodePorts,
	createServerNode,
	createStandaloneNode,
	deleteNode,
	deleteServerNode,
	disconnectEdge,
	getCanvasGraph,
	moveNode,
	moveServerNode
} from '#lib/components/canvas/commands.js';
import {
	buildBackendIndex,
	buildFlowEdges,
	buildFlowNodes,
	buildPortIndex,
	canConnect,
	keepEdges,
	keepNodes,
	mergeTombstones,
	parseFlowNodeId,
	reconcileFlowEdges,
	reconcileFlowNodes,
	type FlowNode,
	type ForceTarget,
	type PortIndexEntry,
	type PanelTarget,
	type Tombstones
} from '#lib/components/canvas/graph.js';
import EntryNode from '#lib/components/canvas/nodes/EntryNode.svelte';
import ExitNode from '#lib/components/canvas/nodes/ExitNode.svelte';
import LoadBalanceNode from '#lib/components/canvas/nodes/LoadBalanceNode.svelte';
import RelayNode from '#lib/components/canvas/nodes/RelayNode.svelte';
import ServerNode from '#lib/components/canvas/nodes/ServerNode.svelte';
import ForceDeleteDialog from '#lib/components/canvas/panels/ForceDeleteDialog.svelte';
import NodePanel from '#lib/components/canvas/panels/NodePanel.svelte';
import { Badge } from '#lib/components/ui/badge/index.js';
import { Button } from '#lib/components/ui/button/index.js';
import * as Card from '#lib/components/ui/card/index.js';
import * as Empty from '#lib/components/ui/empty/index.js';
import * as Resizable from '#lib/components/ui/resizable/index.js';
import { Skeleton } from '#lib/components/ui/skeleton/index.js';
import { errorMessage } from '#lib/i18n/codes.js';
import { m } from '#lib/paraglide/messages.js';

let { canvasId, editable, admin }: { canvasId: string; editable: boolean; admin: boolean } =
	$props();

const graph = $derived(getCanvasGraph({ canvasId }));

let nodes = $state.raw<FlowNode[]>([]);
let edges = $state.raw<Edge[]>([]);
let portIndex = $state.raw(new Map<string, PortIndexEntry>());
let panelTarget = $state<PanelTarget | null>(null);
let forceTargets = $state<ForceTarget[]>([]);
let problemsOpen = $state(false);
let flowEl = $state<HTMLDivElement | null>(null);
/**
 * The delete batches still in flight, each owning what it dropped from the
 * mirror. Every delete command refreshes the shared graph query, so without this
 * the first one would pop the items whose command has not run yet back onto the
 * canvas. A batch removes only its own entry when it settles, so two overlapping
 * gestures cannot lift each other's protection; the re-reconcile that follows is
 * what restores anything the control plane refused.
 */
let deleteBatches = $state.raw<Tombstones[]>([]);
const pendingDeletes = $derived(mergeTombstones(deleteBatches));

const { screenToFlowPosition, updateNode } = useSvelteFlow();
const updateNodeInternals = useUpdateNodeInternals();

const nodeTypes = {
	server: ServerNode,
	entry: EntryNode,
	relay: RelayNode,
	exit: ExitNode,
	loadBalance: LoadBalanceNode
};

// The server owns the graph, but a refresh is merged into the local mirror
// instead of replacing it: untouched nodes keep their object identity, so they
// neither re-render nor lose their measured geometry. Only the nodes that
// actually changed are re-measured — handle counts move with pods and
// load-balance members, and stale handle geometry would misplace their edges.
$effect(() => {
	const current = graph.current;
	if (!current) return;
	const gone = pendingDeletes;
	const nextNodes = gone ? keepNodes(buildFlowNodes(current), gone) : buildFlowNodes(current);
	const nextEdges = gone ? keepEdges(buildFlowEdges(current), gone) : buildFlowEdges(current);
	const merged = untrack(() => reconcileFlowNodes(nodes, nextNodes));
	nodes = merged.nodes;
	edges = untrack(() => reconcileFlowEdges(edges, nextEdges));
	portIndex = buildPortIndex(current);
	if (merged.remeasure.length > 0) {
		const ids = merged.remeasure;
		tick().then(() => updateNodeInternals(ids));
	}
});

const refresh = () => getCanvasGraph({ canvasId }).refresh();

function reportError(err: unknown) {
	const body = (err as { body?: App.Error }).body;
	toast.error(errorMessage(body?.code, body?.message ?? ''));
}

const failureMessage = (err: unknown): string => {
	const body = (err as { body?: App.Error }).body;
	return errorMessage(body?.code, body?.message ?? '');
};

/** Centre of the visible pane, in flow coordinates. */
function palettePosition(): { x: number; y: number } {
	const rect = flowEl?.getBoundingClientRect();
	if (!rect) return { x: 0, y: 0 };
	const point = screenToFlowPosition({
		x: rect.left + rect.width / 2,
		y: rect.top + rect.height / 2
	});
	return { x: Math.round(point.x), y: Math.round(point.y) };
}

async function addServer() {
	const { x, y } = palettePosition();
	try {
		await createServerNode({ canvasId, name: m.editor_add_server(), x, y });
	} catch (err) {
		reportError(err);
	}
}

async function addNode(
	kind: 'entry' | 'relay' | 'exit' | 'load_balance_distribute' | 'load_balance_aggregate',
	name: string
) {
	const { x, y } = palettePosition();
	try {
		await createStandaloneNode({ canvasId, kind, name, x, y, memberCount: 2 });
	} catch (err) {
		reportError(err);
	}
}

/**
 * Positions are persisted on drop. These two commands deliberately skip the
 * query refresh: the local position already matches, so re-rendering the whole
 * graph after every drag would only cost a flicker.
 */
async function persistMove(dragged: FlowNode[]) {
	try {
		for (const node of dragged) {
			const { kind, id } = parseFlowNodeId(node.id);
			const x = Math.round(node.position.x);
			const y = Math.round(node.position.y);
			if (kind === 'server') {
				await moveServerNode({ canvasId, serverId: id, x, y });
			} else if (node.data.kind !== 'server') {
				// `UpdateNodeMeta` replaces name and comment, so they are resent as-is.
				await moveNode({
					canvasId,
					nodeId: id,
					name: node.data.node.name,
					comment: node.data.node.comment,
					x,
					y
				});
			}
		}
	} catch (err) {
		reportError(err);
		await refresh();
	}
}

const ARROW_KEYS: Record<string, true> = {
	ArrowUp: true,
	ArrowDown: true,
	ArrowLeft: true,
	ArrowRight: true
};

/**
 * Svelte Flow nudges the selection on arrow keys but never fires a drag stop, so
 * the release is what persists the new positions. Only key events aimed at the
 * flow count: the node panel's inputs handle their own arrows.
 */
function nudgeStop(event: KeyboardEvent) {
	if (!editable || !ARROW_KEYS[event.key]) return;
	if (!(event.target instanceof Node) || !flowEl?.contains(event.target)) return;
	const selected = nodes.filter(node => node.selected);
	if (selected.length > 0) persistMove(selected);
}

const isValidConnection = (connection: Edge | Connection): boolean => {
	const current = graph.current;
	return current ? canConnect(connection, portIndex, current, edges) : false;
};

async function connect(connection: Connection) {
	try {
		await connectNodePorts({
			canvasId,
			outputPortId: connection.sourceHandle ?? '',
			inputPortId: connection.targetHandle ?? ''
		});
		toast.success(m.editor_connected());
	} catch (err) {
		reportError(err);
		// Drops the optimistic edge Svelte Flow inserted.
		await refresh();
	}
}

/**
 * Always returns `false`: deletions are the server's to make. The whole doomed
 * selection leaves the local mirror before any command is awaited, so the canvas
 * reacts to the keypress immediately instead of trailing the slowest call, and
 * stays gone for the rest of the batch through `pendingDeletes`. Clearing that
 * re-reconciles against the graph the control plane actually kept, which is what
 * puts a refused item back.
 */
async function beforeDelete({
	nodes: doomedNodes,
	edges: doomedEdges
}: {
	nodes: FlowNode[];
	edges: Edge[];
}): Promise<boolean> {
	const failures: ForceTarget[] = [];
	let deleted = false;

	const gone: Tombstones = {
		nodes: new Set(doomedNodes.map(node => node.id)),
		edges: new Set(doomedEdges.map(edge => edge.id))
	};
	deleteBatches = [...deleteBatches, gone];
	nodes = keepNodes(nodes, gone);
	edges = keepEdges(edges, gone);

	try {
		for (const edge of doomedEdges) {
			try {
				await disconnectEdge({ canvasId, edgeId: edge.id, force: false });
				deleted = true;
			} catch (err) {
				failures.push({
					kind: 'edge',
					id: edge.id,
					label: m.editor_disconnected(),
					message: failureMessage(err)
				});
			}
		}

		for (const node of doomedNodes) {
			const { kind, id } = parseFlowNodeId(node.id);
			const label = node.data.kind === 'server' ? node.data.server.name : node.data.node.name;
			try {
				if (kind === 'server') await deleteServerNode({ canvasId, serverId: id, force: false });
				else await deleteNode({ canvasId, nodeId: id, force: false });
				deleted = true;
			} catch (err) {
				failures.push({ kind, id, label, message: failureMessage(err) });
			}
		}
	} finally {
		// This batch has settled: it stops hiding its own items, so the reconcile
		// that follows brings back whatever the control plane refused. Any batch
		// still running keeps hiding its own.
		deleteBatches = deleteBatches.filter(batch => batch !== gone);
	}

	if (failures.length > 0) {
		// Forcing needs admin; everyone else only gets the reason.
		if (admin) forceTargets = failures;
		else toast.error(failures[0]?.message ?? '');
		// Every refusal above left the server untouched, but a call can also fail
		// after the control plane acted, so this one case is re-read.
		await refresh();
	} else if (deleted) {
		toast.success(m.editor_deleted());
	}

	return false;
}

function openProblem(nodeIds: string[]) {
	const current = graph.current;
	const first = nodeIds[0];
	if (!current || first === undefined) return;
	const located = buildBackendIndex(current).get(first);
	if (!located) return;
	updateNode(located.flowId, { selected: true });
	panelTarget = located.target;
}
</script>

<svelte:window onkeyup={nudgeStop} />

<svelte:boundary>
	<!-- Only the first load has nothing to show: a refresh keeps the flow mounted,
	     otherwise remounting it would re-run `fitView` and reset the viewport. -->
	{#if graph.current === undefined}
		<Skeleton class="h-full w-full" />
	{:else}
		{@const current = graph.current}
		<Resizable.PaneGroup direction="horizontal">
			<Resizable.Pane defaultSize={70} minSize={40} order={1}>
				<div class="relative h-full w-full" bind:this={flowEl}>
					<SvelteFlow
						bind:nodes
						bind:edges
						{nodeTypes}
						fitView
						minZoom={0.2}
						colorMode={mode.current ?? 'system'}
						nodesDraggable={editable}
						nodesConnectable={editable}
						{isValidConnection}
						onconnect={connect}
						onbeforedelete={beforeDelete}
						onnodeclick={({ node }) => (panelTarget = parseFlowNodeId(node.id))}
						onnodedragstop={({ nodes: dragged }) => persistMove(dragged)}
						deleteKey={editable ? 'Delete' : null}
					>
						<Background />
						<!-- Bottom-left belongs to the problems panel. -->
						<Controls position="top-right" />
						<MiniMap />

						{#if editable}
							<Panel position="top-left">
								<div class="flex flex-wrap gap-2">
									<Button size="sm" variant="secondary" onclick={addServer}>
										<PlusIcon />
										{m.editor_add_server()}
									</Button>
									<Button
										size="sm"
										variant="secondary"
										onclick={() => addNode('entry', m.editor_add_entry())}
									>
										<PlusIcon />
										{m.editor_add_entry()}
									</Button>
									<Button
										size="sm"
										variant="secondary"
										onclick={() => addNode('relay', m.editor_add_relay())}
									>
										<PlusIcon />
										{m.editor_add_relay()}
									</Button>
									<Button
										size="sm"
										variant="secondary"
										onclick={() => addNode('exit', m.editor_add_exit())}
									>
										<PlusIcon />
										{m.editor_add_exit()}
									</Button>
									<Button
										size="sm"
										variant="secondary"
										onclick={() => addNode('load_balance_distribute', m.editor_add_lb_distribute())}
									>
										<PlusIcon />
										{m.editor_add_lb_distribute()}
									</Button>
									<Button
										size="sm"
										variant="secondary"
										onclick={() => addNode('load_balance_aggregate', m.editor_add_lb_aggregate())}
									>
										<PlusIcon />
										{m.editor_add_lb_aggregate()}
									</Button>
								</div>
							</Panel>
						{/if}

						<Panel position="bottom-left">
							{#if current}
								{@const problems = current.problems}
								{#if problems.length === 0}
									<Badge variant="secondary">{m.canvas_health_ok()}</Badge>
								{:else if problems.length > 3 && !problemsOpen}
									<Button size="sm" variant="outline" onclick={() => (problemsOpen = true)}>
										{m.editor_problems({ count: problems.length })}
									</Button>
								{:else}
									<Card.Root class="max-w-md gap-2 py-3">
										<Card.Content class="grid gap-2 px-3">
											{#if current.orphanPods.length > 0}
												<p class="text-xs text-muted-foreground">
													{m.editor_pod_orphan({ count: current.orphanPods.length })}
												</p>
											{/if}
											{#each problems as problem (problem.message)}
												<button
													type="button"
													class="flex items-start gap-2 text-start text-xs hover:underline"
													onclick={() => openProblem(problem.nodeIds)}
												>
													<Badge
														variant={problem.severity === 'error' ? 'destructive' : 'outline'}
													>
														{problem.severity === 'error'
															? m.editor_severity_error()
															: m.editor_severity_warning()}
													</Badge>
													<span>{problem.message}</span>
												</button>
											{/each}
										</Card.Content>
									</Card.Root>
								{/if}
							{/if}
						</Panel>
					</SvelteFlow>

					{#if current && current.servers.length === 0 && current.nodes.length === 0}
						<div class="pointer-events-none absolute inset-0 flex items-center justify-center">
							<Empty.Root>
								<Empty.Header>
									<Empty.Title>{m.editor_empty_title()}</Empty.Title>
									<Empty.Description>{m.editor_empty_description()}</Empty.Description>
								</Empty.Header>
							</Empty.Root>
						</div>
					{/if}
				</div>
			</Resizable.Pane>

			<!-- Conditional panes need `order` so the flow stays first when this mounts. -->
			{#if panelTarget}
				<Resizable.Handle withHandle />
				<Resizable.Pane defaultSize={30} minSize={20} maxSize={50} order={2}>
					<NodePanel bind:target={panelTarget} {canvasId} {editable} graph={current} />
				</Resizable.Pane>
			{/if}
		</Resizable.PaneGroup>

		{#if admin}
			<ForceDeleteDialog bind:targets={forceTargets} {canvasId} />
		{/if}
	{/if}

	{#snippet failed(error)}
		{@const body = (error as { body?: App.Error }).body}
		<Empty.Root>
			<Empty.Header>
				<Empty.Title>{m.error_title()}</Empty.Title>
				<Empty.Description>{errorMessage(body?.code, body?.message ?? '')}</Empty.Description>
			</Empty.Header>
		</Empty.Root>
	{/snippet}
</svelte:boundary>
