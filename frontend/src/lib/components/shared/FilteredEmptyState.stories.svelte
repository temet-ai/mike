<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { expect, fn, userEvent } from 'storybook/test';
	import { Wrench } from '@lucide/svelte';
	import FilteredEmptyState from './FilteredEmptyState.svelte';

	const { Story } = defineMeta({
		title: 'Shared/FilteredEmptyState',
		component: FilteredEmptyState,
		tags: ['autodocs']
	});
</script>

<Story
	name="FilterExcludedEverything"
	tags={['interactive']}
	args={{
		icon: Wrench,
		filtered: true,
		emptyText: 'No tool uses recorded',
		emptySubtext: 'Tool calls appear here once sessions make their first tool use.',
		filteredText: 'No calls match these filters',
		filteredSubtext:
			'Try another project or tool, widen the time range, or clear your filters to see every captured call.',
		onClearFilters: fn()
	}}
	play={async ({ canvasElement, args }) => {
		const text = canvasElement.querySelector('[data-slot="filtered-empty-state-text"]');
		expect(text?.textContent).toBe('No calls match these filters');
		const sub = canvasElement.querySelector('[data-slot="filtered-empty-state-sub"]');
		expect(sub?.textContent).toContain('clear your filters');
		const clear = canvasElement.querySelector(
			'[data-slot="filtered-empty-state-clear"]'
		) as HTMLButtonElement | null;
		expect(clear).not.toBeNull();
		await userEvent.click(clear!);
		expect(args.onClearFilters).toHaveBeenCalledTimes(1);
	}}
>
	{#snippet template(args)}
		<div class="p-4" style="max-width: 480px; background: var(--surface-card);">
			<FilteredEmptyState {...args} />
		</div>
	{/snippet}
</Story>
