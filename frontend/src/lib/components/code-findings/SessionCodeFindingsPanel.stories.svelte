<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { userEvent, within, expect, waitFor } from 'storybook/test';
	import SessionCodeFindingsPanel from './SessionCodeFindingsPanel.svelte';
	import {
		buildCodeFindingsResponse,
		CODE_FINDINGS_SESSION_ID
	} from '$lib/test-support/codeFindingsFixtures';

	const { Story } = defineMeta({
		title: 'CodeFindings/SessionCodeFindingsPanel',
		component: SessionCodeFindingsPanel,
		tags: ['autodocs']
	});

	const NOW = Date.parse('2026-11-14T10:00:00Z');
	const requested: string[] = [];

	function stubFetch(): void {
		requested.length = 0;
		globalThis.fetch = (async (input: RequestInfo | URL) => {
			const url = typeof input === 'string' ? input : input.toString();
			requested.push(url);
			const more = url.includes('limit=400');
			return new Response(
				JSON.stringify(buildCodeFindingsResponse({ findings_truncated: !more })),
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
	name="Findings with more to load"
	args={{ sessionId: CODE_FINDINGS_SESSION_ID, now: NOW }}
	loaders={[
		async () => {
			localStorage.clear();
			stubFetch();
			return {};
		}
	]}
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);
		await waitFor(() => expect(canvas.getByText('1 open')).toBeInTheDocument());
		await expect(canvas.queryByText('scripts/deploy-proxy.sh')).not.toBeInTheDocument();
		await userEvent.click(canvas.getByRole('button', { name: /code findings/i }));

		await waitFor(() => expect(canvas.getByText('scripts/deploy-proxy.sh')).toBeInTheDocument());
		for (const label of [
			'Files scanned',
			'Found',
			'Told',
			'Fixed after told',
			'Fixed by other',
			'Ignored',
			'Open'
		]) {
			await expect(canvas.getByText(label)).toBeVisible();
		}
		await expect(canvas.getByText('rust-no-comments')).toBeVisible();
		await expect(canvas.getAllByText(/A fixed \/tmp path collides/)).toHaveLength(2);
		await expect(canvas.getByText(/against the session that edited it/)).toBeVisible();
		await expect(canvas.getByText('mktemp -t deploy.XXXXXX')).toBeVisible();
		await expect(canvas.getByText('file deleted')).toBeVisible();
		await expect(canvas.getByText('fixed by other')).toBeVisible();
		const link = canvas.getAllByRole('link', { name: 'request #4821' })[0]!;
		await expect(link).toHaveAttribute(
			'href',
			`/sessions/${CODE_FINDINGS_SESSION_ID}/requests/4821`
		);
		await expect(canvas.getByRole('link', { name: 'All sessions' })).toHaveAttribute(
			'href',
			'/code-findings'
		);
		await expect(requested[0]).toContain(`session=${CODE_FINDINGS_SESSION_ID}`);
		await expect(requested[0]).toContain('hours=720');
		await expect(requested[0]).toContain('limit=200');

		await expect(canvas.getByText('Showing the newest 5 findings.')).toBeVisible();
		await userEvent.click(canvas.getByRole('button', { name: 'Load more' }));
		await waitFor(() => expect(requested.some((url) => url.includes('limit=400'))).toBe(true));
		await waitFor(() => expect(canvas.queryByRole('button', { name: 'Load more' })).toBeNull());
	}}
>
	{#snippet template(args)}
		<div style="max-width: 820px; padding: 12px;">
			<SessionCodeFindingsPanel {...args} />
		</div>
	{/snippet}
</Story>
