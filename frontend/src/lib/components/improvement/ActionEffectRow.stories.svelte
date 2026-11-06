<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { expect, within } from 'storybook/test';
	import ActionEffectRow from './ActionEffectRow.svelte';
	import { ACTION_EFFECTS } from '$lib/test-support/improvementFixtures';

	const { Story } = defineMeta({
		title: 'Improvement/ActionEffectRow',
		component: ActionEffectRow,
		tags: ['autodocs']
	});
</script>

<Story
	name="Default"
	args={{ action: ACTION_EFFECTS[0]! }}
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);
		await expect(
			canvas.getByRole('link', {
				name: /Never claim a fix is verified without re-running the failing test/
			})
		).toBeInTheDocument();
		await expect(canvas.getByText('temet-proxy')).toBeInTheDocument();
		await expect(canvas.getByText(/Fewer corrections:/)).toBeInTheDocument();
		await expect(
			canvas.getByText(/both past the 30-turn read point this needed/)
		).toBeInTheDocument();
		await expect(
			canvas.getByText(/unlikely to be chance \(3–19 percentage points\)/)
		).toBeInTheDocument();
	}}
/>
