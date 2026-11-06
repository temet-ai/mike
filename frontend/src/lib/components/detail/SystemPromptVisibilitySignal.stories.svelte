<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import SystemPromptVisibilitySignal from './SystemPromptVisibilitySignal.svelte';
	import type { SystemBlocksProjection, SystemBlock } from '$lib/types';

	function buildSystemBlock(
		index: number,
		text: string,
		composition_match: SystemBlock['composition_match']
	): SystemBlock {
		return { index, text, cache_control: null, composition_match };
	}

	function buildProjection(
		prompt_sha256: string | null,
		blocks: SystemBlock[],
		composition_id: string | null = null,
		composition_version: number | null = null
	): SystemBlocksProjection {
		return { prompt_sha256, composition_id, composition_version, blocks };
	}

	const DRIFTED_PROJECTION = buildProjection(
		'a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2',
		[
			buildSystemBlock(0, 'You are Claude Code, the official Anthropic CLI.', 'matched'),
			buildSystemBlock(1, 'Modified system prompt text that no longer matches.', 'unknown'),
			buildSystemBlock(2, 'An extra block added after registration.', 'extra')
		],
		'comp-uuid-001',
		3
	);

	const { Story } = defineMeta({
		title: 'Components/Detail/SystemPromptVisibilitySignal',
		component: SystemPromptVisibilitySignal,
		tags: ['autodocs']
	});
</script>

<Story name="Drifted" args={{ systemBlocks: DRIFTED_PROJECTION, requestId: 'req-003' }}>
	{#snippet template(args)}
		<div class="p-4">
			<SystemPromptVisibilitySignal {...args} />
		</div>
	{/snippet}
</Story>
