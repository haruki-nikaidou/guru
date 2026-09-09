<script lang="ts">
import { locales, localizeHref } from '#lib/paraglide/runtime';
import { Toaster } from "$lib/components/ui/sonner/index.js";
import { resolve } from '$app/paths';
import { page } from '$app/state';
import type { Path } from '$app/types';
import './layout.css';
import favicon from '#lib/assets/favicon.svg';

let { children } = $props();
</script>

<svelte:head><link rel="icon" href={favicon} /></svelte:head>
<Toaster />
{@render children()}

<div style="display:none">
	{#each locales as locale (locale)}
		<a
			href={resolve(localizeHref(page.url.pathname, { locale }) as Path)}
		>{locale}</a>
	{/each}
</div>
