<script lang="ts">
import CopyIcon from '@lucide/svelte/icons/copy';
import { toast } from 'svelte-sonner';
import * as Alert from '#lib/components/ui/alert/index.js';
import { Button } from '#lib/components/ui/button/index.js';
import * as Dialog from '#lib/components/ui/dialog/index.js';
import * as Field from '#lib/components/ui/field/index.js';
import * as InputGroup from '#lib/components/ui/input-group/index.js';
import { Input } from '#lib/components/ui/input/index.js';
import { Spinner } from '#lib/components/ui/spinner/index.js';
import { issueMessage } from '#lib/i18n/codes.js';
import { m } from '#lib/paraglide/messages.js';
import { createApiKey } from './apiKeys.remote.js';

let { open = $bindable(false) }: { open?: boolean } = $props();

// The secret is returned exactly once; it lives only in this component's state.
let secret = $state('');

$effect(() => {
	const result = createApiKey.result;
	if (result?.ok) secret = result.secret;
});

$effect(() => {
	if (!open) {
		secret = '';
		createApiKey.fields.name.set('');
	}
});

const nameIssues = $derived(createApiKey.fields.name.issues());

async function copySecret() {
	try {
		await navigator.clipboard.writeText(secret);
		toast.success(m.api_keys_copied());
	} catch {
		// Clipboard access can be denied outright; the secret stays selectable.
		toast.error(m.api_keys_copy_failed());
	}
}
</script>

<Dialog.Root bind:open>
	<Dialog.Content>
		<Dialog.Header>
			<Dialog.Title>{m.api_keys_create_title()}</Dialog.Title>
			<Dialog.Description>{m.api_keys_create_description()}</Dialog.Description>
		</Dialog.Header>

		{#if secret}
			<Alert.Root variant="destructive">
				<Alert.Description>{m.api_keys_secret_once()}</Alert.Description>
			</Alert.Root>
			<InputGroup.Root>
				<InputGroup.Input readonly value={secret} />
				<InputGroup.Addon align="inline-end">
					<Button variant="ghost" size="icon" aria-label={m.api_keys_copy()} onclick={copySecret}>
						<CopyIcon />
					</Button>
				</InputGroup.Addon>
			</InputGroup.Root>
			<Dialog.Footer>
				<Button onclick={() => (open = false)}>{m.common_close()}</Button>
			</Dialog.Footer>
		{:else}
			<form {...createApiKey}>
				<Field.FieldGroup>
					<Field.Field data-invalid={nameIssues !== undefined}>
						<Field.FieldLabel for="api-key-name">{m.api_keys_name()}</Field.FieldLabel>
						<Input
							id="api-key-name"
							{...createApiKey.fields.name.as('text')}
							aria-invalid={nameIssues !== undefined}
						/>
						{#each nameIssues ?? [] as issue (issue.message)}
							<Field.FieldError>{issueMessage(issue.message)}</Field.FieldError>
						{/each}
					</Field.Field>
				</Field.FieldGroup>

				<Dialog.Footer class="mt-6">
					<Button type="button" variant="outline" onclick={() => (open = false)}>
						{m.common_cancel()}
					</Button>
					<Button type="submit" disabled={createApiKey.pending > 0}>
						{#if createApiKey.pending > 0}<Spinner data-icon="inline-start" />{/if}
						{m.common_create()}
					</Button>
				</Dialog.Footer>
			</form>
		{/if}
	</Dialog.Content>
</Dialog.Root>
