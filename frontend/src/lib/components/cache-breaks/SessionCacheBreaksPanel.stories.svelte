<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { userEvent, within, expect, waitFor } from 'storybook/test';
	import SessionCacheBreaksPanel from './SessionCacheBreaksPanel.svelte';
	import {
		CACHE_BREAKS,
		CACHE_BREAK_GROUPS,
		CACHE_BREAK_SESSION_ID,
		buildCacheBreak,
		buildCacheBreaksResponse
	} from '$lib/test-support/cacheBreakFixtures';

	const { Story } = defineMeta({
		title: 'CacheBreaks/SessionCacheBreaksPanel',
		component: SessionCacheBreaksPanel,
		tags: ['autodocs']
	});

	const NOW = Date.parse('2026-11-17T10:00:00Z');
	const requested: string[] = [];

	function stubFetch(): void {
		requested.length = 0;
		globalThis.fetch = (async (input: RequestInfo | URL) => {
			const url = typeof input === 'string' ? input : input.toString();
			requested.push(url);
			const second = url.includes('offset=4');
			const body = second
				? buildCacheBreaksResponse({
						session_id: CACHE_BREAK_SESSION_ID,
						breaks: [buildCacheBreak({ id: 9, request_id: 4000, source_request_id: null })],
						has_more: false
					})
				: buildCacheBreaksResponse({
						session_id: CACHE_BREAK_SESSION_ID,
						groups: CACHE_BREAK_GROUPS,
						breaks: CACHE_BREAKS,
						has_more: true
					});
			return new Response(JSON.stringify(body), {
				status: 200,
				headers: { 'content-type': 'application/json' }
			});
		}) as typeof fetch;
	}
</script>

<script lang="ts">
	import { setContext } from 'svelte';
	import { COLLAPSE_STORE_KEY } from '$lib/context-keys';
	import { createCollapseStore } from '$lib/stores/collapseState.svelte';

	setContext(COLLAPSE_STORE_KEY, createCollapseStore());
</script>

<Story
	name="Breaks with more to load"
	args={{ sessionId: CACHE_BREAK_SESSION_ID, now: NOW }}
	loaders={[
		async () => {
			localStorage.clear();
			stubFetch();
			return {};
		}
	]}
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);
		await waitFor(() => expect(canvas.getByText('6 breaks · 1.8M tokens rewritten')).toBeVisible());
		await expect(canvas.getByRole('table', { name: 'Cache breaks by cause' })).toBeVisible();
		await expect(canvas.getByRole('table', { name: 'Cache breaks in this session' })).toBeVisible();
		await expect(canvas.getByRole('link', { name: 'All sessions' })).toHaveAttribute(
			'href',
			'/diagnostics/prompt-cache'
		);
		await expect(requested[0]).toContain(`/api/sessions/${CACHE_BREAK_SESSION_ID}/cache-breaks`);
		await expect(requested[0]).not.toContain('hours=');

		await userEvent.click(canvas.getByRole('button', { name: 'More' }));
		await waitFor(() => expect(requested.some((url) => url.includes('offset=4'))).toBe(true));
		await waitFor(() => expect(canvas.getByRole('link', { name: 'request #4000' })).toBeVisible());
		await waitFor(() => expect(canvas.queryByRole('button', { name: 'More' })).toBeNull());
	}}
>
	{#snippet template(args)}
		<div style="max-width: 1000px; padding: 12px;">
			<SessionCacheBreaksPanel {...args} />
		</div>
	{/snippet}
</Story>
