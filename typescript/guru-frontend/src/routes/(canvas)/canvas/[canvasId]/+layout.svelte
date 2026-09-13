<script lang="ts">
import ChevronRightIcon from '@lucide/svelte/icons/chevron-right';
import { goto } from '$app/navigation';
import { page } from '$app/state';
import CanvasSidebar from '#lib/components/nav/canvas-sidebar.svelte';
import * as Select from '#lib/components/ui/select/index.js';
import * as Sidebar from '#lib/components/ui/sidebar/index.js';
import { Skeleton } from '#lib/components/ui/skeleton/index.js';
import { m } from '#lib/paraglide/messages.js';
import { getCanvasTrail, listCanvasOptions } from '../../../(home)/canvases.remote.js';
import type { LayoutProps } from './$types.js';

let { data, children }: LayoutProps = $props();

const canvasId = $derived(page.params.canvasId ?? '');
const options = listCanvasOptions();
const current = $derived(options.current?.find(option => option.id === canvasId));
// Every canvas has a trail; a root's is just itself.
const trail = $derived(getCanvasTrail({ canvasId }));
const roots = $derived(options.current?.filter(option => option.isRoot) ?? []);
/** The switcher lists roots, so inside a subcanvas it shows the tree it is in. */
const rootId = $derived(trail.current?.path[0]?.id ?? canvasId);

// Switching keeps the sub-page: `/canvas/a/help` becomes `/canvas/b/help`.
const suffix = $derived(page.url.pathname.slice(`/canvas/${canvasId}`.length));

function switchCanvas(id: string) {
	if (!id || id === canvasId) return;
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
				<!-- The switcher changes tree; the breadcrumb next to it moves inside
				     one. Its label is fixed: picking the root already on screen would
				     emit no change, so the way back up is the breadcrumb, not this. -->
				<Select.Root type="single" value={rootId} onValueChange={switchCanvas}>
					<Select.Trigger size="sm" class="w-40" aria-label={m.canvas_switch()}>
						<span class="truncate">{m.canvas_switch()}</span>
					</Select.Trigger>
					<Select.Content>
						<Select.Group>
							{#each roots as option (option.id)}
								<Select.Item value={option.id} label={option.name}>{option.name}</Select.Item>
							{/each}
						</Select.Group>
					</Select.Content>
				</Select.Root>

				{#each trail.current?.path ?? [] as step, index (step.id)}
					{#if index > 0}
						<ChevronRightIcon class="size-4 shrink-0 text-muted-foreground" />
					{/if}
					{#if step.id === canvasId}
						<span class="truncate text-sm font-medium">{step.name}</span>
					{:else}
						<a href="/canvas/{step.id}{suffix}" class="truncate text-sm hover:underline">
							{step.name}
						</a>
					{/if}
				{/each}
			{/if}
		</header>
		<main class="flex min-h-0 flex-1 flex-col overflow-auto">{@render children()}</main>
	</Sidebar.Inset>
</Sidebar.Provider>
