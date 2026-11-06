<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { userEvent, waitFor, expect, within } from 'storybook/test';
	import SessionLaunchPanel from './SessionLaunchPanel.svelte';
	import CollapseStoreProvider from '$lib/components/__harness__/CollapseStoreProvider.svelte';
	import { buildLaunchRow } from '$lib/components/__fixtures__/launch';

	const STORY_SESSION = '6ed89307-ecac-4248-99ba-babd6b75c185';
	const STATIC_NOW = Date.parse('2026-09-17T09:30:00Z');

	function stub(): void {
		let closed = false;
		globalThis.fetch = (async (input: RequestInfo | URL, init?: RequestInit) => {
			const url = typeof input === 'string' ? input : input.toString();
			const method = init?.method ?? 'GET';
			if (method === 'DELETE' && url.includes(`/api/launches/${STORY_SESSION}`)) {
				closed = true;
				return new Response(
					JSON.stringify({
						session: STORY_SESSION,
						surface: 'herdr:pane-1',
						closed: true,
						process_exited: true
					}),
					{ status: 200, headers: { 'content-type': 'application/json' } }
				);
			}
			if (url.includes(`/api/launches/${STORY_SESSION}`)) {
				const row = buildLaunchRow({
					id: 604,
					backend: 'herdr',
					session_uuid: STORY_SESSION,
					result: 'success',
					channel_check: 'found',
					opened: true,
					delivered: true,
					confirmed: true,
					created_at: '2026-09-17T09:12:00Z',
					opened_at: '2026-09-17T09:12:05Z',
					closed_at: closed ? '2026-09-17T09:20:00Z' : null,
					close_reason: closed ? 'kill --session' : null,
					close_process_exited: closed ? true : null,
					model: 'claude-sonnet-4-5',
					claude_args: ['--session-id', STORY_SESSION, '--remote-control', '--model', 'sonnet'],
					prompt_present: true,
					brief_mode: 'inline',
					brief_text: 'reply with just the word warm-ok',
					requested_placement: 'tab',
					actual_placement: 'tab:workspace-2'
				});
				return new Response(JSON.stringify(row), {
					status: 200,
					headers: { 'content-type': 'application/json' }
				});
			}
			return new Response(
				JSON.stringify({ error: 'not_found', message: 'no launch record', remedy: null }),
				{
					status: 404,
					headers: { 'content-type': 'application/json' }
				}
			);
		}) as typeof fetch;
	}

	const { Story } = defineMeta({
		title: 'Shell/SessionLaunchPanel',
		component: SessionLaunchPanel,
		tags: ['autodocs']
	});
</script>

<Story
	name="Populated"
	loaders={[
		async () => {
			localStorage.clear();
			stub();
			return {};
		}
	]}
	play={async ({ canvasElement }) => {
		const canvas = canvasElement;
		const canvasQ = within(canvasElement);

		const header = await waitFor(() => canvasQ.getByRole('button', { name: /\blaunch\b/i }));
		await expect(header).toHaveAttribute('aria-expanded', 'false');
		await waitFor(() => expect(canvas.textContent).toContain('success'));

		await userEvent.click(header);
		await expect(header).toHaveAttribute('aria-expanded', 'true');

		await expect(canvas.textContent).toContain('herdr');
		await expect(canvas.textContent).toContain('placement tab:workspace-2');
		await expect(canvas.textContent).toContain('channel: found');
		await expect(canvas.textContent).toContain('opened');
		await expect(canvas.textContent).toContain('delivered');
		await expect(canvas.textContent).toContain('confirmed');

		const detailToggle = canvas.querySelector('.slp-detail-toggle') as HTMLButtonElement;
		await expect(detailToggle).toBeInTheDocument();
		await expect(canvas.textContent).toContain('click to expand full launch detail');
		await expect(canvas.textContent).not.toContain('warm-ok');
		await userEvent.click(detailToggle);
		await waitFor(() => expect(canvas.textContent).toContain('reply with just the word warm-ok'));
		await expect(canvas.textContent).toContain('--session-id');
		await expect(canvas.textContent).toContain(STORY_SESSION);

		const killButton = await waitFor(() => {
			const btn = Array.from(canvas.querySelectorAll('button')).find(
				(b) => b.textContent?.trim() === 'Kill'
			);
			if (!btn) throw new Error('Kill button not yet rendered');
			return btn as HTMLButtonElement;
		});
		await userEvent.click(killButton);
		const confirmButton = await waitFor(() => {
			const btn = Array.from(canvas.querySelectorAll('button')).find(
				(b) => b.textContent?.trim() === 'Confirm'
			);
			if (!btn) throw new Error('inline confirm not yet rendered');
			return btn as HTMLButtonElement;
		});
		await userEvent.click(confirmButton);
		await waitFor(() => {
			const stillOpen = Array.from(canvas.querySelectorAll('button')).some((b) =>
				['Kill', 'Confirm'].includes(b.textContent?.trim() ?? '')
			);
			if (stillOpen) throw new Error('still shows a live kill control after closing');
		});
	}}
>
	{#snippet template()}
		<div style="max-width: 640px; background: var(--surface-bg); padding: 16px;">
			<CollapseStoreProvider>
				<SessionLaunchPanel sessionId={STORY_SESSION} now={STATIC_NOW} />
			</CollapseStoreProvider>
		</div>
	{/snippet}
</Story>
