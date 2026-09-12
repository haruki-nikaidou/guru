<script lang="ts">
import PlusIcon from '@lucide/svelte/icons/plus';
import Trash2Icon from '@lucide/svelte/icons/trash-2';
import { untrack } from 'svelte';
import { toast } from 'svelte-sonner';
import {
	addServerIpAddress,
	createPodNode,
	removeServerIpAddress,
	updateServerNode
} from '#lib/components/canvas/commands.js';
import { Button } from '#lib/components/ui/button/index.js';
import * as Field from '#lib/components/ui/field/index.js';
import { Input } from '#lib/components/ui/input/index.js';
import * as Select from '#lib/components/ui/select/index.js';
import { Separator } from '#lib/components/ui/separator/index.js';
import { Spinner } from '#lib/components/ui/spinner/index.js';
import { Textarea } from '#lib/components/ui/textarea/index.js';
import type { Ipv6ResolveName, ServerDto } from '#lib/dto/topology.js';
import { errorMessage } from '#lib/i18n/codes.js';
import { m } from '#lib/paraglide/messages.js';
import ServerPodRow from './ServerPodRow.svelte';

let { canvasId, server, editable }: { canvasId: string; server: ServerDto; editable: boolean } =
	$props();

const IPV6_OPTIONS: Ipv6ResolveName[] = ['required', 'preferred', 'tolerated', 'forbidden'];
const ipv6Label = (value: Ipv6ResolveName): string =>
	value === 'required'
		? m.editor_ipv6_required()
		: value === 'preferred'
			? m.editor_ipv6_preferred()
			: value === 'forbidden'
				? m.editor_ipv6_forbidden()
				: m.editor_ipv6_tolerated();

let name = $state('');
let icon = $state('');
let comment = $state('');
let ipv6Resolve = $state<Ipv6ResolveName>('tolerated');
let logLevel = $state('info');
let pending = $state(false);

// Draft rows for the two "add" forms.
let newIp = $state('');
let newCountry = $state('');
let newPodName = $state('');
/** Empty until the operator picks one; resolved through `podIp` below. */
let newPodIp = $state('');
// `Input` renders a dynamic `type`, so Svelte never coerces this to a number.
let newPodPort = $state('8080');

let seededFor = $state('');
$effect(() => {
	if (seededFor === server.id) return;
	seededFor = server.id;
	const snapshot = server;
	untrack(() => {
		name = snapshot.name;
		icon = snapshot.icon;
		comment = snapshot.comment;
		ipv6Resolve = snapshot.ipv6Resolve;
		logLevel = snapshot.logLevel;
		newIp = '';
		newCountry = '';
		newPodName = '';
		newPodIp = '';
		newPodPort = '8080';
	});
});

const addressOf = (id: string): string => server.ips.find(ip => ip.id === id)?.ip ?? id;

// The draft pod always points at a real ip record: the operator's choice while it
// still exists, otherwise the server's first address. Seeding this once would
// leave the draft empty for a server whose first ip is added in this same sheet.
const podIp = $derived(
	server.ips.some(ip => ip.id === newPodIp) ? newPodIp : (server.ips[0]?.id ?? '')
);

/** Every mutation here reports the control plane's own text on failure. */
async function run(action: () => Promise<unknown>, success: string) {
	pending = true;
	try {
		await action();
		toast.success(success);
	} catch (err) {
		const body = (err as { body?: App.Error }).body;
		toast.error(errorMessage(body?.code, body?.message ?? ''));
	} finally {
		pending = false;
	}
}

const save = () =>
	run(
		() =>
			updateServerNode({
				canvasId,
				serverId: server.id,
				name,
				icon,
				comment,
				ipv6Resolve,
				logLevel
			}),
		m.editor_saved()
	);

const addIp = () =>
	run(async () => {
		await addServerIpAddress({ canvasId, serverId: server.id, ip: newIp, country: newCountry });
		newIp = '';
		newCountry = '';
	}, m.editor_saved());

const removeIp = (ipRecordId: string) =>
	run(() => removeServerIpAddress({ canvasId, ipRecordId }), m.editor_deleted());

// A pod only exists against one of this server's ip records, so pods are created
// here. Their stored position is unused: they render inside the server node.
const addPod = () =>
	run(async () => {
		await createPodNode({
			canvasId,
			name: newPodName,
			ipRecordId: podIp,
			port: Number(newPodPort),
			x: server.x,
			y: server.y
		});
		newPodName = '';
	}, m.editor_saved());
</script>

<Field.FieldGroup>
	<Field.Field>
		<Field.FieldLabel for="server-name">{m.editor_node_name()}</Field.FieldLabel>
		<Input id="server-name" bind:value={name} disabled={!editable} />
	</Field.Field>

	<Field.Field>
		<Field.FieldLabel for="server-icon">{m.editor_server_icon()}</Field.FieldLabel>
		<Input id="server-icon" bind:value={icon} disabled={!editable} />
	</Field.Field>

	<Field.Field>
		<Field.FieldLabel for="server-comment">{m.editor_node_comment()}</Field.FieldLabel>
		<Textarea id="server-comment" bind:value={comment} disabled={!editable} />
	</Field.Field>

	<Field.Field>
		<Field.FieldLabel for="server-ipv6">{m.editor_server_ipv6()}</Field.FieldLabel>
		<Select.Root
			type="single"
			value={ipv6Resolve}
			disabled={!editable}
			onValueChange={next => (ipv6Resolve = next as Ipv6ResolveName)}
		>
			<Select.Trigger id="server-ipv6">{ipv6Label(ipv6Resolve)}</Select.Trigger>
			<Select.Content>
				<Select.Group>
					{#each IPV6_OPTIONS as option (option)}
						<Select.Item value={option} label={ipv6Label(option)}>{ipv6Label(option)}</Select.Item>
					{/each}
				</Select.Group>
			</Select.Content>
		</Select.Root>
	</Field.Field>

	<Field.Field>
		<Field.FieldLabel for="server-log-level">{m.editor_server_log_level()}</Field.FieldLabel>
		<Input id="server-log-level" bind:value={logLevel} disabled={!editable} />
	</Field.Field>
</Field.FieldGroup>

<Button class="mt-4 w-full" disabled={!editable || pending} onclick={save}>
	{#if pending}<Spinner data-icon="inline-start" />{/if}
	{m.common_save()}
</Button>

<Separator class="my-6" />

<h3 class="text-sm font-medium">{m.editor_server_ips()}</h3>
{#if server.ips.length === 0}
	<p class="mt-2 text-sm text-muted-foreground">{m.editor_server_no_ips()}</p>
{:else}
	<ul class="mt-2 grid gap-2">
		{#each server.ips as ip (ip.id)}
			<li class="flex items-center gap-2 text-sm">
				<span class="flex-1 font-mono">{ip.ip}</span>
				<span class="text-muted-foreground">{ip.country}</span>
				<Button
					size="sm"
					variant="ghost"
					disabled={!editable || pending}
					onclick={() => removeIp(ip.id)}
					aria-label={m.common_delete()}
				>
					<Trash2Icon />
				</Button>
			</li>
		{/each}
	</ul>
{/if}

<div class="mt-3 flex items-end gap-2">
	<Input placeholder={m.editor_server_ip()} bind:value={newIp} disabled={!editable} />
	<Input
		placeholder={m.editor_server_country()}
		class="w-24"
		bind:value={newCountry}
		disabled={!editable}
	/>
	<Button
		size="sm"
		variant="secondary"
		disabled={!editable || pending || newIp.trim() === ''}
		onclick={addIp}
	>
		<PlusIcon />
		{m.editor_server_add_ip()}
	</Button>
</div>

<Separator class="my-6" />

<h3 class="text-sm font-medium">{m.editor_pods()}</h3>
{#if server.pods.length === 0}
	<p class="mt-2 text-sm text-muted-foreground">{m.editor_pod_none()}</p>
{:else}
	<div class="mt-2 grid gap-4">
		{#each server.pods as pod (pod.id)}
			<ServerPodRow {canvasId} {pod} ips={server.ips} {editable} />
		{/each}
	</div>
{/if}

<div class="mt-4 grid gap-2">
	<Input placeholder={m.editor_node_name()} bind:value={newPodName} disabled={!editable} />
	<div class="flex gap-2">
		<Select.Root
			type="single"
			value={podIp}
			disabled={!editable || server.ips.length === 0}
			onValueChange={next => (newPodIp = next)}
		>
			<Select.Trigger class="flex-1">
				{podIp ? addressOf(podIp) : m.editor_server_ip()}
			</Select.Trigger>
			<Select.Content>
				<Select.Group>
					{#each server.ips as ip (ip.id)}
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
			bind:value={newPodPort}
			disabled={!editable}
			aria-label={m.editor_pod_port()}
		/>
		<Button
			size="sm"
			variant="secondary"
			disabled={!editable || pending || newPodName.trim() === '' || podIp === ''}
			onclick={addPod}
		>
			<PlusIcon />
			{m.editor_pod_add()}
		</Button>
	</div>
</div>
