<script lang="ts">
import Trash2Icon from '@lucide/svelte/icons/trash-2';
import { untrack } from 'svelte';
import { toast } from 'svelte-sonner';
import { deleteNode, replacePodSpec, updateNodeText } from '#lib/components/canvas/commands.js';
import { Button } from '#lib/components/ui/button/index.js';
import { Input } from '#lib/components/ui/input/index.js';
import * as Select from '#lib/components/ui/select/index.js';
import { Spinner } from '#lib/components/ui/spinner/index.js';
import type { PodDto, ServerIpDto } from '#lib/dto/topology.js';
import { errorMessage } from '#lib/i18n/codes.js';
import { m } from '#lib/paraglide/messages.js';

let {
	canvasId,
	pod,
	ips,
	editable
}: { canvasId: string; pod: PodDto; ips: ServerIpDto[]; editable: boolean } = $props();

let name = $state('');
let ipRecordId = $state('');
// `Input` renders a dynamic `type`, so Svelte never coerces this to a number.
let port = $state('1');
let pending = $state(false);

let seededFor = $state('');
$effect(() => {
	if (seededFor === pod.id) return;
	seededFor = pod.id;
	const snapshot = pod;
	untrack(() => {
		name = snapshot.name;
		ipRecordId = snapshot.ipRecordId;
		port = String(snapshot.port);
	});
});

const addressOf = (id: string): string => ips.find(ip => ip.id === id)?.ip ?? id;

async function save() {
	pending = true;
	try {
		await updateNodeText({ canvasId, nodeId: pod.id, name, comment: pod.comment });
		await replacePodSpec({ canvasId, nodeId: pod.id, ipRecordId, port: Number(port) });
		toast.success(m.editor_saved());
	} catch (err) {
		const body = (err as { body?: App.Error }).body;
		toast.error(errorMessage(body?.code, body?.message ?? ''));
	} finally {
		pending = false;
	}
}

async function remove() {
	pending = true;
	try {
		await deleteNode({ canvasId, nodeId: pod.id, force: false });
		toast.success(m.editor_deleted());
	} catch (err) {
		const body = (err as { body?: App.Error }).body;
		toast.error(errorMessage(body?.code, body?.message ?? ''));
	} finally {
		pending = false;
	}
}
</script>

<div class="flex items-end gap-2">
	<div class="grid flex-1 gap-2">
		<Input bind:value={name} disabled={!editable} aria-label={m.editor_node_name()} />
		<div class="flex gap-2">
			<Select.Root
				type="single"
				value={ipRecordId}
				disabled={!editable}
				onValueChange={next => (ipRecordId = next)}
			>
				<Select.Trigger class="flex-1">{addressOf(ipRecordId)}</Select.Trigger>
				<Select.Content>
					<Select.Group>
						{#each ips as ip (ip.id)}
							<Select.Item value={ip.id} label={ip.ip}>{ip.ip}</Select.Item>
						{/each}
					</Select.Group>
				</Select.Content>
			</Select.Root>
			<Input
				type="number"
				min={1}
				max={65535}
				class="w-24"
				bind:value={port}
				disabled={!editable}
				aria-label={m.editor_pod_port()}
			/>
		</div>
	</div>
	<div class="grid gap-2">
		<Button size="sm" variant="secondary" disabled={!editable || pending} onclick={save}>
			{#if pending}<Spinner data-icon="inline-start" />{/if}
			{m.common_save()}
		</Button>
		<Button
			size="sm"
			variant="ghost"
			disabled={!editable || pending}
			onclick={remove}
			aria-label={m.common_delete()}
		>
			<Trash2Icon />
		</Button>
	</div>
</div>
