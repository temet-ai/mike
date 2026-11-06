<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import EmptyStateOnboarding from './EmptyStateOnboarding.svelte';
	import 'augmented-ui/augmented-ui.min.css';
	import type { Snippet } from 'svelte';

	interface EmptyStateProps {
		headline?: string;
		body?: string;
		command?: string | null;
		confirmation?: string;
		onOpenHelp?: () => void;
		actions?: Snippet;
	}

	function buildEmptyStateProps(overrides: Partial<EmptyStateProps> = {}): EmptyStateProps {
		return {
			headline: 'No requests captured yet',
			body: 'Point Claude Code at the proxy and run a query.',
			command: 'mike claude',
			confirmation: 'Captured requests will appear here in real time.',
			...overrides
		};
	}

	const { Story } = defineMeta({
		title: 'Shared/EmptyStateOnboarding',
		component: EmptyStateOnboarding,
		tags: ['autodocs']
	});
</script>

<Story name="Default" args={buildEmptyStateProps()}>
	{#snippet template(args)}
		<EmptyStateOnboarding {...args} />
	{/snippet}
</Story>

<Story name="WithHelpCta" args={buildEmptyStateProps({ onOpenHelp: () => {} })}>
	{#snippet template(args)}
		<EmptyStateOnboarding {...args} />
	{/snippet}
</Story>

<Story
	name="WithoutCommand"
	args={buildEmptyStateProps({ command: null, confirmation: undefined })}
>
	{#snippet template(args)}
		<EmptyStateOnboarding {...args} />
	{/snippet}
</Story>

<Story
	name="SessionsVariant"
	args={buildEmptyStateProps({
		headline: 'No sessions yet',
		body: 'Launch Claude Code and send a prompt to capture a session here.',
		command: 'mike claude',
		confirmation: 'New sessions will appear here as Claude Code routes through the proxy.'
	})}
>
	{#snippet template(args)}
		<EmptyStateOnboarding {...args} />
	{/snippet}
</Story>

<Story
	name="PromptsVariant"
	args={buildEmptyStateProps({
		headline: 'No compositions captured',
		body: 'System-prompt compositions appear here once Claude Code traffic passes through the proxy.',
		command: null,
		confirmation: 'Compositions will be extracted automatically from incoming requests.'
	})}
>
	{#snippet template(args)}
		<EmptyStateOnboarding {...args} />
	{/snippet}
</Story>
