<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { expect, within } from 'storybook/test';
	import CacheBreakTable from './CacheBreakTable.svelte';
	import { CACHE_BREAKS, CACHE_BREAK_SESSION_ID } from '$lib/test-support/cacheBreakFixtures';

	const { Story } = defineMeta({
		title: 'CacheBreaks/CacheBreakTable',
		component: CacheBreakTable,
		tags: ['autodocs']
	});

	const NOW = Date.parse('2026-11-17T10:00:00Z');
</script>

<Story
	name="Breaks"
	args={{ breaks: CACHE_BREAKS, now: NOW, showSession: true }}
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);
		const table = canvas.getByRole('table', { name: 'Cache breaks' });
		const rows = within(table).getAllByRole('row').slice(1);
		await expect(rows).toHaveLength(4);
		await expect(rows[0]).toHaveTextContent('self_identity');
		await expect(rows[0]).toHaveTextContent('168.0K');
		await expect(rows[0]).toHaveTextContent('42.0s');
		await expect(rows[1]).toHaveTextContent('tools added: mcp__mike__get_stats');
		await expect(rows[1]).toHaveTextContent('tools removed: WebSearch');
		await expect(rows[3]).toHaveTextContent('1h 30m');
		await expect(canvas.getAllByRole('link', { name: 'Fix the deploy script' })[0]).toHaveAttribute(
			'href',
			`/sessions/${CACHE_BREAK_SESSION_ID}`
		);
		await expect(canvas.getByRole('link', { name: 'request #5120' })).toHaveAttribute(
			'href',
			`/sessions/${CACHE_BREAK_SESSION_ID}/requests/5120`
		);
		await expect(canvas.getByRole('link', { name: 'source #5119' })).toHaveAttribute(
			'href',
			`/sessions/${CACHE_BREAK_SESSION_ID}/requests/5119`
		);
		await expect(canvas.queryByRole('link', { name: 'source #5099' })).toBeNull();
	}}
>
	{#snippet template(args)}
		<div style="max-width: 1100px; padding: 12px;">
			<CacheBreakTable {...args} />
		</div>
	{/snippet}
</Story>
