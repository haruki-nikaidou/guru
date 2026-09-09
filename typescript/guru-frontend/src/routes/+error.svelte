<script lang="ts">
import TriangleAlertIcon from '@lucide/svelte/icons/triangle-alert';
import { page } from '$app/state';
import { buttonVariants } from '#lib/components/ui/button/index.js';
import * as Empty from '#lib/components/ui/empty/index.js';
import { errorMessage } from '#lib/i18n/codes.js';
import { m } from '#lib/paraglide/messages.js';

const description = $derived(errorMessage(page.error?.code, page.error?.message ?? ''));
</script>

<div class="grid min-h-svh place-items-center p-4">
	<Empty.Root>
		<Empty.Header>
			<Empty.Media variant="icon"><TriangleAlertIcon /></Empty.Media>
			<Empty.Title>{m.error_title()}</Empty.Title>
			<Empty.Description>{description}</Empty.Description>
		</Empty.Header>
		<Empty.Content>
			<a href="/" class={buttonVariants({ variant: 'outline' })}>{m.common_back_home()}</a>
		</Empty.Content>
	</Empty.Root>
</div>
