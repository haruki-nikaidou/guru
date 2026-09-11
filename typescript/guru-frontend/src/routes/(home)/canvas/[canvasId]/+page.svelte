<script lang="ts">
import { SvelteFlowProvider } from '@xyflow/svelte';
import CanvasFlow from '#lib/components/canvas/CanvasFlow.svelte';
import { canEditWorkspace } from '#lib/permissions.js';
import { page } from '$app/state';
import type { PageProps } from './$types.js';

let { data }: PageProps = $props();

const canvasId = $derived(page.params.canvasId ?? '');
const editable = $derived(canEditWorkspace(data.identity.role));
const admin = $derived(data.identity.role === 'admin');
</script>

<!--
	The `(home)` layout pads its `<main>`; the editor is full-bleed, so the padding
	is cancelled here and the height is the viewport minus the `h-14` header.
	`SvelteFlowProvider` is required: `CanvasFlow` calls `useSvelteFlow()`.
-->
<div class="-m-4 h-[calc(100svh-3.5rem)] md:-m-8">
	<SvelteFlowProvider>
		<CanvasFlow {canvasId} {editable} {admin} />
	</SvelteFlowProvider>
</div>
