<script lang="ts">
import AppSidebar from '#lib/components/nav/app-sidebar.svelte';
import * as Sidebar from '#lib/components/ui/sidebar/index.js';
import { m } from '#lib/paraglide/messages.js';
import type { LayoutProps } from './$types.js';

let { data, children }: LayoutProps = $props();
</script>

<svelte:head><title>{m.nav_brand()}</title></svelte:head>

<Sidebar.Provider>
	<AppSidebar identity={data.identity} />
	<Sidebar.Inset>
		<header class="flex h-14 items-center gap-2 border-b px-4">
			<Sidebar.Trigger />
		</header>
		<main class="flex flex-1 flex-col gap-6 p-4 md:p-8">{@render children()}</main>
	</Sidebar.Inset>
</Sidebar.Provider>
