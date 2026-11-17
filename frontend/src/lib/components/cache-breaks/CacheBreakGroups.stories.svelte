<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { expect, within } from 'storybook/test';
	import CacheBreakGroups from './CacheBreakGroups.svelte';
	import { CACHE_BREAK_GROUPS } from '$lib/test-support/cacheBreakFixtures';

	const { Story } = defineMeta({
		title: 'CacheBreaks/CacheBreakGroups',
		component: CacheBreakGroups,
		tags: ['autodocs']
	});
</script>

<Story
	name="By cause"
	args={{ groups: CACHE_BREAK_GROUPS, showSessions: true }}
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);
		const table = canvas.getByRole('table', { name: 'Cache breaks by cause' });
		const rows = within(table).getAllByRole('row').slice(1);
		await expect(rows).toHaveLength(4);
		await expect(rows[0]).toHaveTextContent('self_identity');
		await expect(rows[0]).toHaveTextContent('3');
		await expect(rows[0]).toHaveTextContent('500.0K');
		await expect(rows[1]).toHaveTextContent('Claude Code · tools');
		await expect(rows[3]).toHaveTextContent('expired, woken by operator');
		await expect(rows[3]).toHaveTextContent('1.2M');
		await expect(canvas.getAllByRole('columnheader')).toHaveLength(4);
	}}
>
	{#snippet template(args)}
		<div style="max-width: 900px; padding: 12px;">
			<CacheBreakGroups {...args} />
		</div>
	{/snippet}
</Story>
