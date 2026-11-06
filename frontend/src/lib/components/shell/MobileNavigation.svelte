<script lang="ts">
	import { Menu } from '@lucide/svelte';
	import * as Sheet from '$lib/components/ui/sheet';
	import { Button } from '$lib/components/ui/button';
	import DestinationNav from './DestinationNav.svelte';

	let open = $state(false);
</script>

<Sheet.Root bind:open>
	<Sheet.Trigger>
		{#snippet child({ props })}
			<Button {...props} variant="ghost" size="icon" aria-label="Open navigation">
				<Menu class="size-4" aria-hidden="true" />
			</Button>
		{/snippet}
	</Sheet.Trigger>
	<Sheet.Content class="mobile-navigation-sheet max-w-sm">
		<div class="navigation-heading">
			<Sheet.Title>Navigate</Sheet.Title>
			<Sheet.Description>Explore your workspace.</Sheet.Description>
		</div>
		<DestinationNav showCollapse={false} onNavigate={() => (open = false)} />
	</Sheet.Content>
</Sheet.Root>

<style>
	.navigation-heading {
		padding: var(--space-5) var(--space-7) var(--space-4) var(--space-4);
		border-bottom: var(--hairline) solid var(--border-subtle);
	}
</style>
