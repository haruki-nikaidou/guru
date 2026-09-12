<script lang="ts">
import HeartPulseIcon from '@lucide/svelte/icons/heart-pulse';
import * as Empty from '#lib/components/ui/empty/index.js';
import { m } from '#lib/paraglide/messages.js';
</script>

<div class="flex flex-1 flex-col gap-6 p-4 md:p-8">
	<div>
		<h1 class="text-2xl font-semibold">{m.health_title()}</h1>
		<p class="text-sm text-muted-foreground">{m.health_description()}</p>
	</div>

	<Empty.Root>
		<Empty.Header>
			<Empty.Media variant="icon"><HeartPulseIcon /></Empty.Media>
			<Empty.Title>{m.health_placeholder_title()}</Empty.Title>
			<Empty.Description>{m.health_placeholder_description()}</Empty.Description>
		</Empty.Header>
	</Empty.Root>
</div>
