<script lang="ts">
import LanguagesIcon from '@lucide/svelte/icons/languages';
import { Button } from '#lib/components/ui/button/index.js';
import * as DropdownMenu from '#lib/components/ui/dropdown-menu/index.js';
import { m } from '#lib/paraglide/messages.js';
import { getLocale, setLocale } from '#lib/paraglide/runtime.js';
</script>

<DropdownMenu.Root>
	<DropdownMenu.Trigger>
		{#snippet child({ props })}
			<Button {...props} variant="ghost" size="icon" aria-label={m.nav_language()}>
				<LanguagesIcon />
			</Button>
		{/snippet}
	</DropdownMenu.Trigger>
	<DropdownMenu.Content align="end">
		<DropdownMenu.Group>
			<DropdownMenu.GroupHeading>{m.nav_language()}</DropdownMenu.GroupHeading>
			<DropdownMenu.Separator />
			<DropdownMenu.Item disabled={getLocale() === 'en'} onSelect={() => setLocale('en')}>
				{m.nav_language_en()}
			</DropdownMenu.Item>
			<DropdownMenu.Item disabled={getLocale() === 'ja'} onSelect={() => setLocale('ja')}>
				{m.nav_language_ja()}
			</DropdownMenu.Item>
		</DropdownMenu.Group>
	</DropdownMenu.Content>
</DropdownMenu.Root>
