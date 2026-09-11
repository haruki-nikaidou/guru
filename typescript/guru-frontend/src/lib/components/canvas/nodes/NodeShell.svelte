<script lang="ts">
import type { Snippet } from 'svelte';
import type { ProblemLevel } from '#lib/components/canvas/graph.js';

// The card every node kind renders inside: header, optional comment, port rows.
let {
	title,
	kindLabel,
	comment,
	problem,
	background,
	width = 'w-[260px]',
	icon,
	children
}: {
	title: string;
	kindLabel: string;
	comment: string;
	problem: ProblemLevel;
	/** A `bg-canvas-*` utility backed by the theme tokens. */
	background: string;
	width?: string;
	icon: Snippet;
	children: Snippet;
} = $props();
</script>

<div
	class="{width} {background} rounded-lg border py-2 text-foreground shadow-sm"
	class:ring-2={problem !== 'none'}
	class:ring-destructive={problem === 'error'}
	class:ring-amber-500={problem === 'warning'}
>
	<div class="flex items-center gap-2 px-3">
		{@render icon()}
		<span class="truncate text-sm font-medium">{title}</span>
		<span class="ms-auto text-[10px] uppercase text-muted-foreground">{kindLabel}</span>
	</div>
	{#if comment}
		<p class="truncate px-3 pt-1 text-xs text-muted-foreground">{comment}</p>
	{/if}
	{@render children()}
</div>
