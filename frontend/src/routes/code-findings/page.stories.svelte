<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { userEvent, within, expect, waitFor } from 'storybook/test';
	import Page from './+page.svelte';
	import { buildCodeFindingsResponse } from '$lib/test-support/codeFindingsFixtures';

	const { Story } = defineMeta({
		title: 'Routes/CodeFindings',
		component: Page,
		tags: ['autodocs']
	});

	const requested: string[] = [];

	function stubFetch(): void {
		requested.length = 0;
		globalThis.fetch = (async (input: RequestInfo | URL) => {
			const url = typeof input === 'string' ? input : input.toString();
			requested.push(url);
			return new Response(
				JSON.stringify(buildCodeFindingsResponse({ session_counts: null, findings: [] })),
				{ status: 200, headers: { 'content-type': 'application/json' } }
			);
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
			state: { page: { url: new URL('http://localhost/code-findings'), params: {} } }
		}
	}}
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);
		await expect(canvas.getByRole('heading', { level: 1, name: 'Code findings' })).toBeVisible();
		await waitFor(() => expect(canvas.getByText('1,710')).toBeVisible());
		await expect(canvas.getByText('640 µs')).toBeVisible();
		await expect(canvas.getByText('48 ms')).toBeVisible();
		await expect(canvas.getByText(/Scanning on, telling sessions on, pushing off\./)).toBeVisible();
		await expect(requested[0]).toContain('hours=168');
		await expect(canvas.getByRole('table', { name: 'Rules that fired' })).toBeVisible();
		await expect(canvas.getByText('4 rules · 171 found')).toBeVisible();

		await expect(canvas.queryByRole('table', { name: 'Rule catalogue' })).toBeNull();
		await userEvent.click(canvas.getByRole('button', { name: /rule catalogue/i }));
		await waitFor(() =>
			expect(canvas.getByRole('table', { name: 'Rule catalogue' })).toBeVisible()
		);
		await expect(canvas.getByText('Grammars: bash, python, rust, typescript.')).toBeVisible();

		await userEvent.click(canvas.getByRole('button', { name: /checkouts and project rules/i }));
		await waitFor(() =>
			expect(canvas.getByRole('table', { name: 'Checkouts and project rules' })).toBeVisible()
		);
		await expect(canvas.getByText(/3 watched/)).toBeVisible();
		await expect(canvas.getByText(/1 with errors/)).toBeVisible();
		await expect(canvas.getByText(/unknown field 'langauge'/)).toBeVisible();

		await userEvent.selectOptions(canvas.getByLabelText('Window'), '720');
		await waitFor(() => expect(requested.some((url) => url.includes('hours=720'))).toBe(true));
	}}
>
	{#snippet template()}
		<div class="h-[900px] w-[1100px] overflow-y-auto bg-[var(--surface-bg)]">
			<Page />
		</div>
	{/snippet}
</Story>
