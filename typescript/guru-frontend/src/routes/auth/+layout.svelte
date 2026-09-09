<script lang="ts">
import LocaleMenu from '#lib/components/nav/locale-menu.svelte';

let { children } = $props();
</script>

<div class="relative grid min-h-svh place-items-center p-4">
	<div class="absolute top-4 right-4"><LocaleMenu /></div>
	{@render children()}
</div>
