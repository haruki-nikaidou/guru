<script lang="ts">
import WaypointsIcon from '@lucide/svelte/icons/waypoints';
import type { NodeProps } from '@xyflow/svelte';
import { portLabel, type FlowNodeData } from '#lib/components/canvas/graph.js';
import { m } from '#lib/paraglide/messages.js';
import NodeShell from './NodeShell.svelte';
import PortHandle from './PortHandle.svelte';

let { id, data }: NodeProps & { data: Extract<FlowNodeData, { kind: 'relay' }> } = $props();

const protocol = $derived(
	data.node.protocol === 'tcp_tls'
		? m.editor_relay_tcp_tls()
		: data.node.protocol === 'quic'
			? m.editor_relay_quic()
			: m.editor_relay_tcp_raw()
);
// Both overrides are optional; `0` and `''` mean "use the derived value".
const override = $derived(
	data.node.overrideIpAddress || data.node.overridePort > 0
		? `${data.node.overrideIpAddress || '*'}:${data.node.overridePort || '*'}`
		: ''
);
</script>

<NodeShell
	{id}
	title={data.node.name}
	kindLabel={m.editor_kind_relay()}
	comment={data.node.comment}
	problem={data.problem}
	background="bg-canvas-relay"
>
	{#snippet icon()}<WaypointsIcon class="size-4 shrink-0" />{/snippet}
	<p class="px-3 pt-1 text-xs text-muted-foreground">
		{protocol}{override ? ` · ${override}` : ''}
	</p>
	<div class="mt-1">
		{#each data.node.ports as port (port.id)}
			<PortHandle {port} label={portLabel(port.key)} />
		{/each}
	</div>
</NodeShell>
