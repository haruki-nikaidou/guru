<script lang="ts">
import LogOutIcon from '@lucide/svelte/icons/log-out';
import type { NodeProps } from '@xyflow/svelte';
import { portLabel, type FlowNodeData } from '#lib/components/canvas/graph.js';
import { m } from '#lib/paraglide/messages.js';
import NodeShell from './NodeShell.svelte';
import PortHandle from './PortHandle.svelte';

let { data }: NodeProps & { data: Extract<FlowNodeData, { kind: 'exit' }> } = $props();
</script>

<NodeShell
	title={data.node.name}
	kindLabel={m.editor_kind_exit()}
	comment={data.node.comment}
	problem={data.problem}
	background="bg-canvas-exit"
>
	{#snippet icon()}<LogOutIcon class="size-4 shrink-0" />{/snippet}
	{#snippet children()}
		<p class="truncate px-3 pt-1 font-mono text-xs" class:text-muted-foreground={!data.node.destination}>
			{data.node.destination || m.editor_destination_empty()}
		</p>
		<div class="mt-1">
			{#each data.node.ports as port (port.id)}
				<PortHandle {port} label={portLabel(port.key)} />
			{/each}
		</div>
	{/snippet}
</NodeShell>
