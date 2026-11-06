<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import * as Tooltip from './index';
	import { Button } from '../button';

	const { Story } = defineMeta({
		title: 'UI/Tooltip',
		component: Tooltip.Root,
		tags: ['autodocs']
	});
</script>

<Story name="Default">
	{#snippet template()}
		<div class="flex items-center justify-center p-16">
			<Tooltip.Root open>
				<Tooltip.Trigger>
					{#snippet child({ props })}
						<Button variant="outline" size="sm" {...props}>Copy token</Button>
					{/snippet}
				</Tooltip.Trigger>
				<Tooltip.Content>Copy the API token to clipboard</Tooltip.Content>
			</Tooltip.Root>
		</div>
	{/snippet}
</Story>

<Story name="Anchored">
	{#snippet template()}
		<div class="flex items-start justify-center pt-8 pb-24">
			<Tooltip.Root open>
				<Tooltip.Trigger>
					{#snippet child({ props })}
						<Button variant="ghost" size="sm" {...props}>Session actions</Button>
					{/snippet}
				</Tooltip.Trigger>
				<Tooltip.Content side="bottom" align="center">
					Pin, archive, or delete this session
				</Tooltip.Content>
			</Tooltip.Root>
		</div>
	{/snippet}
</Story>

<Story name="WithIcon">
	{#snippet template()}
		<div class="flex items-center justify-center gap-4 p-16">
			<Tooltip.Root open>
				<Tooltip.Trigger>
					{#snippet child({ props })}
						<Button variant="ghost" size="icon" aria-label="Copy request body" {...props}>
							<svg
								class="size-4"
								xmlns="http://www.w3.org/2000/svg"
								fill="none"
								viewBox="0 0 24 24"
								stroke="currentColor"
								aria-hidden="true"
							>
								<path
									stroke-linecap="round"
									stroke-linejoin="round"
									stroke-width="2"
									d="M8 16H6a2 2 0 01-2-2V6a2 2 0 012-2h8a2 2 0 012 2v2m-6 12h8a2 2 0 002-2v-8a2 2 0 00-2-2h-8a2 2 0 00-2 2v8a2 2 0 002 2z"
								/>
							</svg>
						</Button>
					{/snippet}
				</Tooltip.Trigger>
				<Tooltip.Content>Copy request body</Tooltip.Content>
			</Tooltip.Root>

			<Tooltip.Root>
				<Tooltip.Trigger>
					{#snippet child({ props })}
						<Button variant="ghost" size="icon" aria-label="Collapse all messages" {...props}>
							<svg
								class="size-4"
								xmlns="http://www.w3.org/2000/svg"
								fill="none"
								viewBox="0 0 24 24"
								stroke="currentColor"
								aria-hidden="true"
							>
								<path
									stroke-linecap="round"
									stroke-linejoin="round"
									stroke-width="2"
									d="M5 15l7-7 7 7"
								/>
							</svg>
						</Button>
					{/snippet}
				</Tooltip.Trigger>
				<Tooltip.Content>Collapse all messages</Tooltip.Content>
			</Tooltip.Root>
		</div>
	{/snippet}
</Story>

<Story name="Delayed">
	{#snippet template()}
		<div class="flex items-center justify-center gap-6 p-16">
			<Tooltip.Provider delayDuration={600}>
				<Tooltip.Root open>
					<Tooltip.Trigger>
						{#snippet child({ props })}
							<Button variant="outline" size="sm" {...props}>Pin session</Button>
						{/snippet}
					</Tooltip.Trigger>
					<Tooltip.Content>Pin to keep this session beyond the TTL (600ms delay)</Tooltip.Content>
				</Tooltip.Root>

				<Tooltip.Root>
					<Tooltip.Trigger>
						{#snippet child({ props })}
							<Button variant="outline" size="sm" {...props}>Archive</Button>
						{/snippet}
					</Tooltip.Trigger>
					<Tooltip.Content>Archive hides the session from the default view</Tooltip.Content>
				</Tooltip.Root>
			</Tooltip.Provider>
		</div>
	{/snippet}
</Story>
