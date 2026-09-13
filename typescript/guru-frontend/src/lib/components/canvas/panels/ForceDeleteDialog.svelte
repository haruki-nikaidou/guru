<script lang="ts">
import { toast } from 'svelte-sonner';
import { deleteNode, deleteServerNode, disconnectEdge } from '#lib/components/canvas/commands.js';
import type { ForceTarget } from '#lib/components/canvas/graph.js';
import * as AlertDialog from '#lib/components/ui/alert-dialog/index.js';
import { buttonVariants } from '#lib/components/ui/button/index.js';
import { Spinner } from '#lib/components/ui/spinner/index.js';
import { errorMessage } from '#lib/i18n/codes.js';
import { m } from '#lib/paraglide/messages.js';

// Admin-only: the force RPCs skip validation and the control plane rejects them
// for every other role anyway.
let { targets = $bindable([]), canvasId }: { targets?: ForceTarget[]; canvasId: string } = $props();

let pending = $state(false);

async function force() {
	pending = true;
	try {
		for (const target of targets) {
			if (target.kind === 'edge') {
				await disconnectEdge({ canvasId, edgeId: target.id, force: true });
			} else if (target.kind === 'server') {
				await deleteServerNode({ canvasId, serverId: target.id, force: true });
			} else {
				await deleteNode({
					canvasId,
					nodeId: target.id,
					force: true,
					subcanvasTarget: target.subcanvasTarget ?? '',
					boundary: target.boundary ?? false
				});
			}
		}
		targets = [];
		toast.success(m.editor_deleted());
	} catch (err) {
		const body = (err as { body?: App.Error }).body;
		toast.error(errorMessage(body?.code, body?.message ?? ''));
	} finally {
		pending = false;
	}
}
</script>

<AlertDialog.Root
	open={targets.length > 0}
	onOpenChange={open => {
		if (!open) targets = [];
	}}
>
	<AlertDialog.Content>
		<AlertDialog.Header>
			<AlertDialog.Title>{m.editor_force_title()}</AlertDialog.Title>
			<AlertDialog.Description>{m.editor_force_description()}</AlertDialog.Description>
		</AlertDialog.Header>

		<ul class="grid gap-2 text-sm">
			{#each targets as target (target.id)}
				<li>
					<span class="font-medium">{target.label}</span>
					<span class="text-muted-foreground"> — {target.message}</span>
				</li>
			{/each}
		</ul>

		<AlertDialog.Footer>
			<AlertDialog.Cancel>{m.common_cancel()}</AlertDialog.Cancel>
			<AlertDialog.Action
				class={buttonVariants({ variant: 'destructive' })}
				disabled={pending}
				onclick={force}
			>
				{#if pending}<Spinner data-icon="inline-start" />{/if}
				{m.editor_force_confirm()}
			</AlertDialog.Action>
		</AlertDialog.Footer>
	</AlertDialog.Content>
</AlertDialog.Root>
