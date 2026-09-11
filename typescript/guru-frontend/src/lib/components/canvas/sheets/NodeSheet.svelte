<script lang="ts">
import type { SheetTarget } from '#lib/components/canvas/graph.js';
import * as Sheet from '#lib/components/ui/sheet/index.js';
import type { CanvasGraph } from '#lib/dto/topology.js';
import { m } from '#lib/paraglide/messages.js';
import EntryForm from './EntryForm.svelte';
import ExitForm from './ExitForm.svelte';
import LoadBalanceForm from './LoadBalanceForm.svelte';
import RelayForm from './RelayForm.svelte';
import ServerForm from './ServerForm.svelte';

let {
	target = $bindable(null),
	canvasId,
	editable,
	graph
}: {
	target?: SheetTarget | null;
	canvasId: string;
	editable: boolean;
	graph: CanvasGraph | undefined;
} = $props();

// The target is resolved against the graph on every render, so a refresh reflects
// straight into the open sheet.
const server = $derived(
	target?.kind === 'server' ? graph?.servers.find(entry => entry.id === target?.id) : undefined
);
const node = $derived(
	target?.kind === 'node' ? graph?.nodes.find(entry => entry.id === target?.id) : undefined
);

// A deleted entity closes its own sheet.
$effect(() => {
	if (target && graph && !server && !node) target = null;
});

const title = $derived(server?.name ?? node?.name ?? '');
const kindLabel = $derived(
	server
		? m.editor_kind_server()
		: node?.kind === 'entry'
			? m.editor_kind_entry()
			: node?.kind === 'relay'
				? m.editor_kind_relay()
				: node?.kind === 'exit'
					? m.editor_kind_exit()
					: node?.kind === 'load_balance'
						? node.mode === 'distribute'
							? m.editor_kind_lb_distribute()
							: m.editor_kind_lb_aggregate()
						: ''
);
</script>

<Sheet.Root
	open={target !== null}
	onOpenChange={open => {
		if (!open) target = null;
	}}
>
	<Sheet.Content side="right" class="w-full overflow-y-auto sm:max-w-md">
		<Sheet.Header>
			<Sheet.Title>{title}</Sheet.Title>
			<Sheet.Description>{kindLabel}</Sheet.Description>
		</Sheet.Header>

		<div class="px-4 pb-8">
			{#if server}
				<ServerForm {canvasId} {server} {editable} />
			{:else if node?.kind === 'entry'}
				<EntryForm {canvasId} {node} {editable} />
			{:else if node?.kind === 'relay'}
				<RelayForm {canvasId} {node} {editable} />
			{:else if node?.kind === 'exit'}
				<ExitForm {canvasId} {node} {editable} />
			{:else if node?.kind === 'load_balance'}
				<LoadBalanceForm {canvasId} {node} {editable} />
			{/if}
		</div>
	</Sheet.Content>
</Sheet.Root>
