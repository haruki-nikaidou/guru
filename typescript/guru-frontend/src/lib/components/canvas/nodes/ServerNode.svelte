<script lang="ts">
import ServerIcon from '@lucide/svelte/icons/server';
import type { NodeProps } from '@xyflow/svelte';
import { portLabel, type FlowNodeData } from '#lib/components/canvas/graph.js';
import { formatTimestamp } from '#lib/i18n/format.js';
import { m } from '#lib/paraglide/messages.js';
import NodeShell from './NodeShell.svelte';
import PortHandle from './PortHandle.svelte';

// A server renders as one node containing all of its pods; pods are never placed
// on the canvas themselves.
let { data }: NodeProps & { data: Extract<FlowNodeData, { kind: 'server' }> } = $props();

const ipv6 = $derived(
	data.server.ipv6Resolve === 'required'
		? m.editor_ipv6_required()
		: data.server.ipv6Resolve === 'preferred'
			? m.editor_ipv6_preferred()
			: data.server.ipv6Resolve === 'forbidden'
				? m.editor_ipv6_forbidden()
				: m.editor_ipv6_tolerated()
);

const addressOf = (ipRecordId: string): string =>
	data.server.ips.find(ip => ip.id === ipRecordId)?.ip ?? '?';
</script>

<NodeShell
	title={data.server.name}
	kindLabel={m.editor_kind_server()}
	comment={data.server.comment}
	problem={data.problem}
	background="bg-canvas-server"
	width="w-[300px]"
>
	<!-- `icon` is a free-form operator string, shown verbatim next to the glyph. -->
	{#snippet icon()}
		<ServerIcon class="size-4 shrink-0" />
		{#if data.server.icon}
			<span class="text-xs text-muted-foreground">{data.server.icon}</span>
		{/if}
	{/snippet}
	{#snippet children()}
		<p class="px-3 pt-1 text-xs text-muted-foreground">
			{data.server.logLevel} · {ipv6} · {data.server.ips.length}
			{m.editor_server_ips()}
		</p>
		<p class="px-3 text-xs text-muted-foreground">
			{m.editor_server_last_seen()}: {formatTimestamp(data.server.lastSeenAt)}
		</p>

		{#if data.server.pods.length === 0}
			<p class="px-3 py-2 text-xs text-muted-foreground">{m.editor_pod_none()}</p>
		{:else}
			{#each data.server.pods as pod (pod.id)}
				<div class="mt-2 border-t pt-1">
					<p class="truncate px-3 text-xs font-medium">
						{pod.name}
						<span class="font-mono text-muted-foreground">
							{addressOf(pod.ipRecordId)}:{pod.port}
						</span>
					</p>
					{#each pod.ports as port (port.id)}
						<PortHandle {port} label={portLabel(port.key)} />
					{/each}
				</div>
			{/each}
		{/if}
	{/snippet}
</NodeShell>
