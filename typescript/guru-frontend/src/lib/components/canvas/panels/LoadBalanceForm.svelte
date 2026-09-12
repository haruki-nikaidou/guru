<script lang="ts">
import { untrack } from 'svelte';
import { toast } from 'svelte-sonner';
import { replaceLoadBalanceSpec, updateNodeText } from '#lib/components/canvas/commands.js';
import { Button } from '#lib/components/ui/button/index.js';
import * as Field from '#lib/components/ui/field/index.js';
import { Input } from '#lib/components/ui/input/index.js';
import * as Select from '#lib/components/ui/select/index.js';
import { Spinner } from '#lib/components/ui/spinner/index.js';
import { Textarea } from '#lib/components/ui/textarea/index.js';
import type { LoadBalanceModeName, LoadBalanceNodeDto } from '#lib/dto/topology.js';
import { errorMessage } from '#lib/i18n/codes.js';
import { m } from '#lib/paraglide/messages.js';

let {
	canvasId,
	node,
	editable
}: { canvasId: string; node: LoadBalanceNodeDto; editable: boolean } = $props();

const MODES: LoadBalanceModeName[] = ['round_robin', 'random', 'ip_hash', 'fallback'];
const modeLabel = (value: LoadBalanceModeName): string =>
	value === 'random'
		? m.editor_balance_random()
		: value === 'ip_hash'
			? m.editor_balance_ip_hash()
			: value === 'fallback'
				? m.editor_balance_fallback()
				: m.editor_balance_round_robin();

let name = $state('');
let comment = $state('');
let balanceMode = $state<LoadBalanceModeName>('round_robin');
// `Input` renders a dynamic `type`, so Svelte never coerces: number fields are
// strings here and are converted exactly once, at the call.
let memberCount = $state('2');
let pending = $state(false);

let seededFor = $state('');
$effect(() => {
	if (seededFor === node.id) return;
	seededFor = node.id;
	const snapshot = node;
	untrack(() => {
		name = snapshot.name;
		comment = snapshot.comment;
		balanceMode = snapshot.balanceMode;
		memberCount = String(snapshot.memberCount);
	});
});

async function save() {
	pending = true;
	try {
		await updateNodeText({ canvasId, nodeId: node.id, name, comment });
		// Shrinking is refused by the control plane while a removed port still
		// carries an edge; that `Conflict` text surfaces in the toast below.
		await replaceLoadBalanceSpec({
			canvasId,
			nodeId: node.id,
			mode: node.mode,
			balanceMode,
			memberCount: Number(memberCount)
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
		<Field.FieldLabel for="lb-name">{m.editor_node_name()}</Field.FieldLabel>
		<Input id="lb-name" bind:value={name} disabled={!editable} />
	</Field.Field>

	<Field.Field>
		<Field.FieldLabel for="lb-comment">{m.editor_node_comment()}</Field.FieldLabel>
		<Textarea id="lb-comment" bind:value={comment} disabled={!editable} />
	</Field.Field>

	{#if node.mode === 'distribute'}
		<Field.Field>
			<Field.FieldLabel for="lb-mode">{m.editor_balance_mode()}</Field.FieldLabel>
			<Select.Root
				type="single"
				value={balanceMode}
				disabled={!editable}
				onValueChange={next => (balanceMode = next as LoadBalanceModeName)}
			>
				<Select.Trigger id="lb-mode">{modeLabel(balanceMode)}</Select.Trigger>
				<Select.Content>
					<Select.Group>
						{#each MODES as option (option)}
							<Select.Item value={option} label={modeLabel(option)}>{modeLabel(option)}</Select.Item>
						{/each}
					</Select.Group>
				</Select.Content>
			</Select.Root>
		</Field.Field>
	{/if}

	<Field.Field>
		<Field.FieldLabel for="lb-members">{m.editor_member_count()}</Field.FieldLabel>
		<Input
			id="lb-members"
			type="number"
			min={2}
			max={256}
			bind:value={memberCount}
			disabled={!editable}
		/>
		<Field.FieldDescription>{m.editor_member_count_hint()}</Field.FieldDescription>
	</Field.Field>
</Field.FieldGroup>

<Button class="mt-6 w-full" disabled={!editable || pending} onclick={save}>
	{#if pending}<Spinner data-icon="inline-start" />{/if}
	{m.common_save()}
</Button>
