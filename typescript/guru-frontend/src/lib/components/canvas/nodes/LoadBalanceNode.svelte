<script lang="ts">
import MergeIcon from '@lucide/svelte/icons/merge';
import SplitIcon from '@lucide/svelte/icons/split';
import type { NodeProps } from '@xyflow/svelte';
import { portLabel, type FlowNodeData } from '#lib/components/canvas/graph.js';
import { m } from '#lib/paraglide/messages.js';
import NodeShell from './NodeShell.svelte';
import PortHandle from './PortHandle.svelte';

// Both load-balance variants render here: the backend refuses to change a node's
// spec kind, so the variant is fixed at creation and only ever displayed.
let { data }: NodeProps & { data: Extract<FlowNodeData, { kind: 'load_balance' }> } = $props();

const distribute = $derived(data.node.mode === 'distribute');
const balanceMode = $derived(
	data.node.balanceMode === 'random'
		? m.editor_balance_random()
		: data.node.balanceMode === 'ip_hash'
			? m.editor_balance_ip_hash()
			: data.node.balanceMode === 'fallback'
				? m.editor_balance_fallback()
				: m.editor_balance_round_robin()
);
</script>

<NodeShell
	title={data.node.name}
	kindLabel={distribute ? m.editor_kind_lb_distribute() : m.editor_kind_lb_aggregate()}
	comment={data.node.comment}
	problem={data.problem}
	background="bg-canvas-load-balance"
>
	{#snippet icon()}
		{#if distribute}
			<SplitIcon class="size-4 shrink-0" />
		{:else}
			<MergeIcon class="size-4 shrink-0" />
		{/if}
	{/snippet}
	{#snippet children()}
		<p class="px-3 pt-1 text-xs text-muted-foreground">
			{distribute ? `${balanceMode} · ` : ''}{m.editor_member_count()}: {data.node.memberCount}
		</p>
		<div class="mt-1">
			{#each data.node.ports as port (port.id)}
				<PortHandle {port} label={portLabel(port.key)} />
			{/each}
		</div>
	{/snippet}
</NodeShell>
