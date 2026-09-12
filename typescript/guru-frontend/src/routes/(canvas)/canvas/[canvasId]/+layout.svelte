<script lang="ts">
import { goto } from '$app/navigation';
import { page } from '$app/state';
import CanvasSidebar from '#lib/components/nav/canvas-sidebar.svelte';
import * as Select from '#lib/components/ui/select/index.js';
import * as Sidebar from '#lib/components/ui/sidebar/index.js';
import { Skeleton } from '#lib/components/ui/skeleton/index.js';
import { m } from '#lib/paraglide/messages.js';
import { listCanvasOptions } from '../../../(home)/canvases.remote.js';
import type { LayoutProps } from './$types.js';

let { data, children }: LayoutProps = $props();

const canvasId = $derived(page.params.canvasId ?? '');
const options = listCanvasOptions();
const current = $derived(options.current?.find(option => option.id === canvasId));

// Switching keeps the sub-page: `/canvas/a/help` becomes `/canvas/b/help`.
function switchCanvas(id: string) {
	if (!id || id === canvasId) return;
	const suffix = page.url.pathname.slice(`/canvas/${canvasId}`.length);
	goto(`/canvas/${id}${suffix}`);
}
</script>

<svelte:head><title>{current?.name ?? m.nav_brand()}</title></svelte:head>

<!-- Bounded to the viewport so the editor fills it; other pages scroll inside `main`. -->
<Sidebar.Provider class="h-svh">
	<CanvasSidebar identity={data.identity} {canvasId} />
	<Sidebar.Inset>
		<header class="flex h-14 shrink-0 items-center gap-2 border-b px-4">
			<Sidebar.Trigger />
			{#if options.current === undefined}
				<Skeleton class="h-8 w-56" />
			{:else}
				<Select.Root type="single" value={canvasId} onValueChange={switchCanvas}>
					<Select.Trigger size="sm" class="w-56" aria-label={m.canvas_switch()}>
						<span class="truncate">{current?.name ?? m.canvas_switch()}</span>
					</Select.Trigger>
					<Select.Content>
						<Select.Group>
							{#each options.current as option (option.id)}
								<Select.Item value={option.id} label={option.name}>{option.name}</Select.Item>
							{/each}
						</Select.Group>
					</Select.Content>
				</Select.Root>
			{/if}
		</header>
		<main class="flex min-h-0 flex-1 flex-col overflow-auto">{@render children()}</main>
	</Sidebar.Inset>
</Sidebar.Provider>
