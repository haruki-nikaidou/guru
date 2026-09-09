<script lang="ts">
import { page } from '$app/state';
import * as Alert from '#lib/components/ui/alert/index.js';
import { Button } from '#lib/components/ui/button/index.js';
import * as Card from '#lib/components/ui/card/index.js';
import * as Field from '#lib/components/ui/field/index.js';
import { Input } from '#lib/components/ui/input/index.js';
import { Spinner } from '#lib/components/ui/spinner/index.js';
import { issueMessage, resultMessage } from '#lib/i18n/codes.js';
import { m } from '#lib/paraglide/messages.js';
import { login } from './auth.remote.js';

const next = $derived(page.url.searchParams.get('next') ?? '/');

const emailIssues = $derived(login.fields.email.issues());
const passwordIssues = $derived(login.fields.password.issues());
</script>

<Card.Root class="w-full max-w-sm">
	<Card.Header>
		<Card.Title>{m.auth_title()}</Card.Title>
		<Card.Description>{m.auth_description()}</Card.Description>
	</Card.Header>
	<Card.Content>
		<form {...login}>
			<Field.FieldGroup>
				<Field.Field data-invalid={emailIssues !== undefined}>
					<Field.FieldLabel for="email">{m.auth_email()}</Field.FieldLabel>
					<Input
						id="email"
						autocomplete="username"
						{...login.fields.email.as('email')}
						aria-invalid={emailIssues !== undefined}
					/>
					{#each emailIssues ?? [] as issue (issue.message)}
						<Field.FieldError>{issueMessage(issue.message)}</Field.FieldError>
					{/each}
				</Field.Field>

				<Field.Field data-invalid={passwordIssues !== undefined}>
					<Field.FieldLabel for="password">{m.auth_password()}</Field.FieldLabel>
					<Input
						id="password"
						autocomplete="current-password"
						{...login.fields.password.as('password')}
						aria-invalid={passwordIssues !== undefined}
					/>
					{#each passwordIssues ?? [] as issue (issue.message)}
						<Field.FieldError>{issueMessage(issue.message)}</Field.FieldError>
					{/each}
				</Field.Field>

				<input {...login.fields.next.as('hidden', next)} />

				<Button type="submit" disabled={login.pending > 0}>
					{#if login.pending > 0}<Spinner data-icon="inline-start" />{/if}
					{m.auth_sign_in()}
				</Button>
			</Field.FieldGroup>
		</form>

		{#if login.result?.error}
			<Alert.Root variant="destructive" class="mt-4">
				<Alert.Description>{resultMessage(login.result.error)}</Alert.Description>
			</Alert.Root>
		{/if}
	</Card.Content>
</Card.Root>
