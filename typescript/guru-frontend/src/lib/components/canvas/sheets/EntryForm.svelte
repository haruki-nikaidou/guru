<script lang="ts">
import { untrack } from 'svelte';
import { toast } from 'svelte-sonner';
import { replaceEntrySpec, updateNodeText } from '#lib/components/canvas/commands.js';
import { Button } from '#lib/components/ui/button/index.js';
import * as Field from '#lib/components/ui/field/index.js';
import { Input } from '#lib/components/ui/input/index.js';
import * as Select from '#lib/components/ui/select/index.js';
import { Spinner } from '#lib/components/ui/spinner/index.js';
import { Textarea } from '#lib/components/ui/textarea/index.js';
import type { EntryNodeDto, ProxyProtocolName } from '#lib/dto/topology.js';
import { errorMessage } from '#lib/i18n/codes.js';
import { m } from '#lib/paraglide/messages.js';

let { canvasId, node, editable }: { canvasId: string; node: EntryNodeDto; editable: boolean } =
	$props();

const PROXY_OPTIONS: ProxyProtocolName[] = ['none', 'v1', 'v2'];
const proxyLabel = (value: ProxyProtocolName): string =>
	value === 'v1'
		? m.editor_proxy_v1()
		: value === 'v2'
			? m.editor_proxy_v2()
			: m.editor_proxy_none();

let name = $state('');
let comment = $state('');
let proxy = $state<ProxyProtocolName>('none');
let pending = $state(false);

// Seed once per node: the effect writes the same state it would otherwise read
// back, so an unguarded version would clobber every keystroke.
let seededFor = $state('');
$effect(() => {
	if (seededFor === node.id) return;
	seededFor = node.id;
	const snapshot = node;
	untrack(() => {
		name = snapshot.name;
		comment = snapshot.comment;
		proxy = snapshot.receiveProxyProtocol;
	});
});

async function save() {
	pending = true;
	try {
		await updateNodeText({ canvasId, nodeId: node.id, name, comment });
		// TLS is round-tripped verbatim: `ReplaceNodeSpec` replaces the whole spec.
		await replaceEntrySpec({
			canvasId,
			nodeId: node.id,
			receiveProxyProtocol: proxy,
			tls: node.tls
		});
		toast.success(m.editor_saved());
	} catch (err) {
		const body = (err as { body?: App.Error }).body;
		toast.error(errorMessage(body?.code, body?.message ?? ''));
	} finally {
		pending = false;
	}
}
</script>

<Field.FieldGroup>
	<Field.Field>
		<Field.FieldLabel for="entry-name">{m.editor_node_name()}</Field.FieldLabel>
		<Input id="entry-name" bind:value={name} disabled={!editable} />
	</Field.Field>

	<Field.Field>
		<Field.FieldLabel for="entry-comment">{m.editor_node_comment()}</Field.FieldLabel>
		<Textarea id="entry-comment" bind:value={comment} disabled={!editable} />
	</Field.Field>

	<Field.Field>
		<Field.FieldLabel for="entry-proxy">{m.editor_receive_proxy()}</Field.FieldLabel>
		<Select.Root
			type="single"
			value={proxy}
			disabled={!editable}
			onValueChange={next => (proxy = next as ProxyProtocolName)}
		>
			<Select.Trigger id="entry-proxy">{proxyLabel(proxy)}</Select.Trigger>
			<Select.Content>
				<Select.Group>
					{#each PROXY_OPTIONS as option (option)}
						<Select.Item value={option} label={proxyLabel(option)}>{proxyLabel(option)}</Select.Item>
					{/each}
				</Select.Group>
			</Select.Content>
		</Select.Root>
	</Field.Field>
</Field.FieldGroup>

<Button class="mt-6 w-full" disabled={!editable || pending} onclick={save}>
	{#if pending}<Spinner data-icon="inline-start" />{/if}
	{m.common_save()}
</Button>
