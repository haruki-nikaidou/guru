<script lang="ts">
import * as Card from '#lib/components/ui/card/index.js';
import * as Kbd from '#lib/components/ui/kbd/index.js';
import * as Table from '#lib/components/ui/table/index.js';
import { m } from '#lib/paraglide/messages.js';

// Svelte Flow binds the multi-select and zoom modifiers to ⌘ on macOS and Ctrl
// elsewhere; the page shows the one the reader actually has.
const mod =
	typeof navigator !== 'undefined' && /Mac|iPhone|iPad/.test(navigator.platform) ? '⌘' : 'Ctrl';

const steps = [
	{ title: m.help_step_add_title, body: m.help_step_add_body },
	{ title: m.help_step_edit_title, body: m.help_step_edit_body },
	{ title: m.help_step_connect_title, body: m.help_step_connect_body },
	{ title: m.help_step_arrange_title, body: m.help_step_arrange_body },
	{ title: m.help_step_problems_title, body: m.help_step_problems_body },
	{ title: m.help_step_delete_title, body: m.help_step_delete_body }
];

const shortcuts = [
	{ keys: ['Delete'], action: m.help_key_delete },
	{ keys: ['Shift', m.help_key_drag()], action: m.help_key_box_select },
	{ keys: [mod, m.help_key_click()], action: m.help_key_multi_select },
	{ keys: ['Tab'], action: m.help_key_focus },
	{ keys: ['Enter'], action: m.help_key_select },
	{ keys: ['Esc'], action: m.help_key_deselect },
	{ keys: ['↑ ↓ ← →'], action: m.help_key_nudge },
	{ keys: ['Shift', '↑ ↓ ← →'], action: m.help_key_nudge_fast },
	{ keys: ['Space', m.help_key_drag()], action: m.help_key_pan },
	{ keys: [m.help_key_scroll()], action: m.help_key_zoom }
];
</script>

<div class="flex flex-1 flex-col gap-6 p-4 md:p-8">
	<div>
		<h1 class="text-2xl font-semibold">{m.help_title()}</h1>
		<p class="text-sm text-muted-foreground">{m.help_description()}</p>
	</div>

	<div class="grid max-w-5xl gap-6 lg:grid-cols-2">
		<Card.Root>
			<Card.Header>
				<Card.Title>{m.help_usage_title()}</Card.Title>
				<Card.Description>{m.help_usage_description()}</Card.Description>
			</Card.Header>
			<Card.Content>
				<ol class="flex flex-col gap-4">
					{#each steps as step, index (step.title)}
						<li class="flex gap-3">
							<span
								class="flex size-6 shrink-0 items-center justify-center rounded-full bg-muted text-xs font-medium text-muted-foreground"
							>
								{index + 1}
							</span>
							<div class="flex flex-col gap-1">
								<p class="text-sm font-medium">{step.title()}</p>
								<p class="text-sm text-muted-foreground">{step.body()}</p>
							</div>
						</li>
					{/each}
				</ol>
			</Card.Content>
		</Card.Root>

		<Card.Root>
			<Card.Header>
				<Card.Title>{m.help_keys_title()}</Card.Title>
				<Card.Description>{m.help_keys_description()}</Card.Description>
			</Card.Header>
			<Card.Content>
				<Table.Root>
					<Table.Header>
						<Table.Row>
							<Table.Head>{m.help_keys_column_keys()}</Table.Head>
							<Table.Head>{m.help_keys_column_action()}</Table.Head>
						</Table.Row>
					</Table.Header>
					<Table.Body>
						{#each shortcuts as shortcut (shortcut.action)}
							<Table.Row>
								<Table.Cell>
									<Kbd.Group>
										{#each shortcut.keys as key, index (key)}
											{#if index > 0}<span class="text-xs text-muted-foreground">+</span>{/if}
											<Kbd.Root>{key}</Kbd.Root>
										{/each}
									</Kbd.Group>
								</Table.Cell>
								<Table.Cell class="whitespace-normal">{shortcut.action()}</Table.Cell>
							</Table.Row>
						{/each}
					</Table.Body>
				</Table.Root>
			</Card.Content>
		</Card.Root>
	</div>
</div>
