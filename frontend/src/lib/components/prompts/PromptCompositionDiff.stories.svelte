<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import PromptCompositionDiff from './PromptCompositionDiff.svelte';

	const BLOCK_VOLATILE =
		'HUMAN_TURN\n\nHuman: <system-reminder>Operating inside temet-proxy harness.</system-reminder>';

	const BLOCK_PREAMBLE =
		'You are Claude, an AI assistant built by Anthropic. You operate inside a reverse proxy that captures every /v1/messages request for later inspection.';

	const BLOCK_SKELETON =
		'You are a frontend development expert specialising in Svelte 5, TypeScript, and Tailwind v4.\n\nPrefer composable components. Use typed factory fixtures for Storybook stories. Never truncate block.text in display.';

	const BLOCK_SKELETON_DRIFTED =
		'You are a frontend development expert specialising in Svelte 5 and React.\n\nPrefer composable components. Use typed factory fixtures for Storybook stories.';

	const EXTRA_BLOCK =
		'Additional context: the proxy is running on localhost:9988 with inspect API on 9989.';

	function joinBlocks(blocks: string[]): string {
		return blocks.join('\n\n');
	}

	const COMPOSITION_MATCH = joinBlocks([BLOCK_VOLATILE, BLOCK_PREAMBLE, BLOCK_SKELETON]);
	const COMPOSITION_BASE = joinBlocks([BLOCK_VOLATILE, BLOCK_PREAMBLE, BLOCK_SKELETON]);
	const OBSERVED_DRIFTED = joinBlocks([BLOCK_VOLATILE, BLOCK_PREAMBLE, BLOCK_SKELETON_DRIFTED]);
	const OBSERVED_EXTRA = joinBlocks([BLOCK_VOLATILE, BLOCK_PREAMBLE, BLOCK_SKELETON, EXTRA_BLOCK]);
	const OBSERVED_DROPPED = joinBlocks([BLOCK_VOLATILE, BLOCK_PREAMBLE]);

	function buildDiff(overrides: {
		composition: string | null;
		observed: string | null;
		loading?: boolean;
		error?: string | null;
	}): {
		composition: string | null;
		observed: string | null;
		loading?: boolean;
		error?: string | null;
	} {
		return {
			loading: false,
			error: null,
			...overrides
		};
	}

	const IDENTICAL = buildDiff({ composition: COMPOSITION_MATCH, observed: COMPOSITION_MATCH });

	const MODIFIED = buildDiff({
		composition: COMPOSITION_BASE,
		observed: OBSERVED_DRIFTED
	});

	const ADDED_BLOCK = buildDiff({
		composition: COMPOSITION_BASE,
		observed: OBSERVED_EXTRA
	});

	const REMOVED_BLOCK = buildDiff({
		composition: COMPOSITION_BASE,
		observed: OBSERVED_DROPPED
	});

	const LOADING = buildDiff({ composition: null, observed: null, loading: true });

	const ERROR_STATE = buildDiff({
		composition: COMPOSITION_BASE,
		observed: null,
		error: 'Failed to load observed system prompt: 404 Not Found'
	});

	const EMPTY = buildDiff({ composition: null, observed: null });

	const { Story } = defineMeta({
		title: 'Routes/Prompts/PromptCompositionDiff',
		component: PromptCompositionDiff,
		tags: ['autodocs']
	});
</script>

<Story name="Identical" args={IDENTICAL}>
	{#snippet template(args)}
		<div class="p-4 bg-[var(--surface-card)]">
			<PromptCompositionDiff {...args} />
		</div>
	{/snippet}
</Story>

<Story name="ModifiedBlock" args={MODIFIED}>
	{#snippet template(args)}
		<div class="p-4 bg-[var(--surface-card)]">
			<PromptCompositionDiff {...args} />
		</div>
	{/snippet}
</Story>

<Story name="AddedBlock" args={ADDED_BLOCK}>
	{#snippet template(args)}
		<div class="p-4 bg-[var(--surface-card)]">
			<PromptCompositionDiff {...args} />
		</div>
	{/snippet}
</Story>

<Story name="RemovedBlock" args={REMOVED_BLOCK}>
	{#snippet template(args)}
		<div class="p-4 bg-[var(--surface-card)]">
			<PromptCompositionDiff {...args} />
		</div>
	{/snippet}
</Story>

<Story name="Loading" args={LOADING}>
	{#snippet template(args)}
		<div class="p-4 bg-[var(--surface-card)]">
			<PromptCompositionDiff {...args} />
		</div>
	{/snippet}
</Story>

<Story name="Error" args={ERROR_STATE}>
	{#snippet template(args)}
		<div class="p-4 bg-[var(--surface-card)]">
			<PromptCompositionDiff {...args} />
		</div>
	{/snippet}
</Story>

<Story name="Empty" args={EMPTY}>
	{#snippet template(args)}
		<div class="p-4 bg-[var(--surface-card)]">
			<PromptCompositionDiff {...args} />
		</div>
	{/snippet}
</Story>
