<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { expect, within } from 'storybook/test';
	import EffectVerdict from './EffectVerdict.svelte';
	import { EFFECT_FEWER_CORRECTIONS } from '$lib/test-support/improvementFixtures';

	const { Story } = defineMeta({
		title: 'Improvement/EffectVerdict',
		component: EffectVerdict,
		tags: ['autodocs']
	});
</script>

<Story
	name="Default"
	args={{ effect: EFFECT_FEWER_CORRECTIONS }}
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);
		await expect(canvas.getByText(/Fewer corrections:/)).toBeInTheDocument();
		await expect(canvas.getByText(/27% of 96 turns/)).toBeInTheDocument();
		await expect(canvas.getByText(/30-turn read point/)).toBeInTheDocument();
		await expect(canvas.getByText(/3–19/)).toBeInTheDocument();
	}}
/>
