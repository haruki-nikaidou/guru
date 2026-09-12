<script lang="ts">
import CheckIcon from '@lucide/svelte/icons/check';
import CopyIcon from '@lucide/svelte/icons/copy';
import { toast } from 'svelte-sonner';
import { Button, type ButtonSize } from '#lib/components/ui/button/index.js';
import { m } from '#lib/paraglide/messages.js';

// Clipboard button for a read-only identifier. The glyph carries no meaning of
// its own, so `label` is always the accessible name; success is reported by the
// check glyph rather than a toast, which would be far too loud next to an id.
let {
	value,
	label = m.common_copy(),
	size = 'icon',
	class: className
}: { value: string; label?: string; size?: ButtonSize; class?: string } = $props();

let copied = $state(false);
let timer: ReturnType<typeof setTimeout> | undefined;

// The component can be unmounted while the check glyph is still showing (the
// node panel closes on any click outside it), so the pending timer is dropped.
$effect(() => () => clearTimeout(timer));

async function copy() {
	try {
		await navigator.clipboard.writeText(value);
		copied = true;
		clearTimeout(timer);
		timer = setTimeout(() => (copied = false), 1500);
	} catch {
		// Clipboard access can be denied outright; the value stays selectable.
		toast.error(m.common_copy_failed());
	}
}
</script>

<Button variant="ghost" {size} class={className} aria-label={label} title={label} onclick={copy}>
	{#if copied}
		<CheckIcon />
	{:else}
		<CopyIcon />
	{/if}
</Button>
