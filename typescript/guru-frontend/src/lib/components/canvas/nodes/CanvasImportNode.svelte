<script lang="ts">
import PackageOpenIcon from '@lucide/svelte/icons/package-open';
import type { NodeProps } from '@xyflow/svelte';
import { portLabel, type FlowNodeData } from '#lib/components/canvas/graph.js';
import { m } from '#lib/paraglide/messages.js';
import NodeShell from './NodeShell.svelte';
import PortHandle from './PortHandle.svelte';

// The embedded canvas, seen as one node. Its ports mirror that canvas's export
// nodes and are derived by the control plane, so nothing here is editable
// except the name; double-clicking the card descends into the subcanvas.
let { id, data }: NodeProps & { data: Extract<FlowNodeData, { kind: 'canvas_import' }> } = $props();
</script>

<NodeShell
	{id}
	title={data.node.name}
	kindLabel={m.editor_kind_subcanvas()}
	comment={data.node.comment}
	problem={data.problem}
	background="bg-canvas-import"
>
	{#snippet icon()}<PackageOpenIcon class="size-4 shrink-0" />{/snippet}
	<p class="truncate px-3 pt-1 text-xs" class:text-muted-foreground={!data.node.targetName}>
		{data.node.targetName || m.editor_subcanvas_missing()}
	</p>
	{#if data.node.ports.length === 0}
		<p class="px-3 pt-1 text-xs text-muted-foreground">{m.editor_subcanvas_no_ports()}</p>
	{:else}
		<div class="mt-1">
			{#each data.node.ports as port (port.id)}
				<PortHandle {port} label={portLabel(port)} />
			{/each}
		</div>
	{/if}
</NodeShell>
