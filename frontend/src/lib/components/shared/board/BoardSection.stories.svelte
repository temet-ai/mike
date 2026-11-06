<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { expect, userEvent } from 'storybook/test';
	import BoardColumn from './BoardColumn.svelte';
	import BoardSection from './BoardSection.svelte';
	import NodeChip from './NodeChip.svelte';

	const { Story } = defineMeta({
		title: 'Shared/Board/BoardSection',
		component: BoardSection,
		tags: ['autodocs']
	});
</script>

<script lang="ts">
	let filtersOpen = $state(true);
</script>

<Story
	name="SectionsInColumn"
	play={async ({ canvas }) => {
		const control = canvas.getByRole('button', { name: /collapse filters/i });
		await expect(control).toHaveAttribute('aria-expanded', 'true');
		await userEvent.click(control);
		await expect(control).toHaveAttribute('aria-expanded', 'false');
	}}
>
	{#snippet template()}
		<div data-design="funk" style="width: 260px; background: var(--surface-bg);">
			<BoardColumn label="Mike · Proxy · the Boundary" count={16} state="quiet" height={220}>
				<BoardSection
					label="Filters"
					count={4}
					summary="1 active"
					state="busy"
					control={filtersOpen ? 'collapse' : 'expand'}
					onToggle={() => (filtersOpen = !filtersOpen)}
					y={48}
					divider={false}
				/>
				{#if filtersOpen}
					<NodeChip
						kind="service"
						name="filters"
						sub="busy"
						state="busy"
						icon="filters"
						x={16}
						y={72}
					/>
				{/if}
				<BoardSection label="Lessons" count={12} state="quiet" y={filtersOpen ? 128 : 76} />
			</BoardColumn>
		</div>
	{/snippet}
</Story>
