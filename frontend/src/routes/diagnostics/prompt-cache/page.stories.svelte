<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { userEvent, within, expect, waitFor } from 'storybook/test';
	import Page from './+page.svelte';
	import { buildCacheBreaksResponse } from '$lib/test-support/cacheBreakFixtures';

	const { Story } = defineMeta({
		title: 'Routes/Diagnostics/PromptCache',
		component: Page,
		tags: ['autodocs']
	});

	const requested: { url: string; method: string; body: string | null }[] = [];

	function stubFetch(): void {
		requested.length = 0;
		globalThis.fetch = (async (input: RequestInfo | URL, init?: RequestInit) => {
			const url = typeof input === 'string' ? input : input.toString();
			requested.push({ url, method: init?.method ?? 'GET', body: (init?.body as string) ?? null });
			if (url.includes('/api/cache-breaks/rescan')) {
				return new Response('{}', { status: 202 });
			}
			return new Response(JSON.stringify(buildCacheBreaksResponse()), {
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
	name="Default"
	loaders={[
		async () => {
			localStorage.clear();
			stubFetch();
			return {};
		}
	]}
	parameters={{
		sveltekit_experimental: {
			state: { page: { url: new URL('http://localhost/diagnostics/prompt-cache'), params: {} } }
		}
	}}
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);
		await expect(canvas.getByRole('heading', { level: 1, name: 'Prompt cache' })).toBeVisible();
		await waitFor(() =>
			expect(canvas.getByRole('table', { name: 'Cache breaks by cause' })).toBeVisible()
		);
		await expect(requested[0]!.url).toContain('/api/cache-breaks?hours=24');
		await expect(canvas.getByText('6 breaks · 1.8M tokens rewritten')).toBeVisible();
		await expect(canvas.getByRole('table', { name: 'Cache breaks' })).toBeVisible();
		await expect(canvas.getAllByRole('link', { name: 'Fix the deploy script' })[0]).toBeVisible();

		await userEvent.selectOptions(canvas.getByLabelText('Window'), '168');
		await waitFor(() => expect(requested.some((r) => r.url.includes('hours=168'))).toBe(true));

		await userEvent.click(canvas.getByRole('button', { name: /rescan last 7 days/i }));
		await waitFor(() =>
			expect(canvas.getByText(/The scan restarts within a minute/)).toBeVisible()
		);
		const post = requested.find((r) => r.method === 'POST')!;
		await expect(post.url).toBe('/api/cache-breaks/rescan');
		await expect(post.body).toBe('{"days":7}');
	}}
>
	{#snippet template()}
		<div class="h-[900px] w-[1100px] overflow-y-auto bg-[var(--surface-bg)]">
			<Page />
		</div>
	{/snippet}
</Story>
