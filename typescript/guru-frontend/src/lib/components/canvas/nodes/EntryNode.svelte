<script lang="ts">
import LogInIcon from '@lucide/svelte/icons/log-in';
import type { NodeProps } from '@xyflow/svelte';
import { portLabel, type FlowNodeData } from '#lib/components/canvas/graph.js';
import { m } from '#lib/paraglide/messages.js';
import NodeShell from './NodeShell.svelte';
import PortHandle from './PortHandle.svelte';

let { data }: NodeProps & { data: Extract<FlowNodeData, { kind: 'entry' }> } = $props();

const proxy = $derived(
	data.node.receiveProxyProtocol === 'v1'
		? m.editor_proxy_v1()
		: data.node.receiveProxyProtocol === 'v2'
			? m.editor_proxy_v2()
			: m.editor_proxy_none()
);
</script>

<NodeShell
	title={data.node.name}
	kindLabel={m.editor_kind_entry()}
	comment={data.node.comment}
	problem={data.problem}
	background="bg-canvas-entry"
>
	{#snippet icon()}<LogInIcon class="size-4 shrink-0" />{/snippet}
	{#snippet children()}
		<p class="px-3 pt-1 text-xs text-muted-foreground">
			{m.editor_receive_proxy()}: {proxy}
		</p>
		<div class="mt-1">
			{#each data.node.ports as port (port.id)}
				<PortHandle {port} label={portLabel(port.key)} />
			{/each}
		</div>
	{/snippet}
</NodeShell>
