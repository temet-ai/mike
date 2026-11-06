<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { expect, within } from 'storybook/test';
	import SkillStateBadge from './SkillStateBadge.svelte';
	import { STATE_TEXT, type SkillState } from '$lib/api/skills';

	const STATES = Object.keys(STATE_TEXT) as SkillState[];

	const { Story } = defineMeta({
		title: 'Skills/SkillStateBadge',
		component: SkillStateBadge,
		tags: ['autodocs']
	});
</script>

<Story
	name="Every state"
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);
		for (const state of STATES) {
			await expect(canvas.getByText(STATE_TEXT[state])).toBeInTheDocument();
		}
	}}
>
	{#snippet template()}
		<div class="flex flex-wrap gap-2 bg-[var(--surface-bg)] p-4">
			{#each STATES as state (state)}
				<SkillStateBadge {state} />
			{/each}
		</div>
	{/snippet}
</Story>
