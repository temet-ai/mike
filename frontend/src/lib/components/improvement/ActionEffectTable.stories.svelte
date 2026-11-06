<!--
What each action has done, grouped into fixed-order bands. Measured results and guard questions
start open; "Not enough data yet" starts closed and opens in place.
-->
<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { expect, userEvent, within } from 'storybook/test';
	import ActionEffectTable from './ActionEffectTable.svelte';
	import { ACTION_EFFECTS } from '$lib/test-support/improvementFixtures';

	const { Story } = defineMeta({
		title: 'Improvement/ActionEffectTable',
		component: ActionEffectTable,
		tags: ['autodocs']
	});
</script>

<Story
	name="Default"
	args={{ actions: ACTION_EFFECTS, error: null, onRetry: () => {} }}
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);
		await expect(canvas.getByText(/What each action has done · 6/)).toBeInTheDocument();

		const toggles = canvas.getAllByRole('button');
		await expect(toggles.map((b) => b.textContent?.replace(/\s+/g, ' ').trim())).toEqual([
			'▸ Fewer corrections 1',
			'▸ No change seen 1',
			'▸ Guard questions, checked against held-back turns 1',
			'▸ Not enough data yet 2',
			'▸ Not computed yet 1'
		]);
		await expect(toggles[0]).toHaveAttribute('aria-expanded', 'true');
		await expect(toggles[3]).toHaveAttribute('aria-expanded', 'false');

		await expect(
			canvas.getByText('Effect not computed yet. No measurement is available.')
		).toBeInTheDocument();
		await expect(canvas.getByText(/Fewer corrections:/)).toBeInTheDocument();
		await expect(canvas.getByText(/No change we can see:/)).toBeInTheDocument();
		await expect(canvas.getByText(/12 of 34 held-back turns/)).toBeInTheDocument();
		await expect(canvas.queryByText(/Not enough data yet \(12 of 30/)).not.toBeInTheDocument();

		await userEvent.click(toggles[3]!);
		await expect(toggles[3]).toHaveAttribute('aria-expanded', 'true');
		const rows = canvas.getAllByRole('link', {
			name: /^A lesson: (Use the shared|Name the model)/
		});
		await expect(rows.map((r) => r.textContent)).toEqual([
			'A lesson: Use the shared button, never a hand-rolled one.',
			'A lesson: Name the model on every agent dispatch.'
		]);
		await expect(canvas.getByText(/Not enough data yet \(12 of 30/)).toBeInTheDocument();

		await userEvent.click(toggles[0]!);
		await expect(toggles[0]).toHaveAttribute('aria-expanded', 'false');
	}}
>
	{#snippet template(args)}
		<div class="w-[24rem] bg-[var(--surface-bg)] p-4">
			<ActionEffectTable {...args} />
		</div>
	{/snippet}
</Story>
