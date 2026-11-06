<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { userEvent, waitFor, expect } from 'storybook/test';
	import ProjectLaunchesPanel from './ProjectLaunchesPanel.svelte';
	import CollapseStoreProvider from '$lib/components/__harness__/CollapseStoreProvider.svelte';
	import { buildLaunchRow } from '$lib/components/__fixtures__/launch';
	import type { LaunchRow } from '$lib/api/launches';
	import type { FleetProjectSummary } from '$lib/fleet/types';
	import 'augmented-ui/augmented-ui.min.css';

	const PROJECT_PATH = '/home/vimes/projects/temet/temet-proxy';
	const OTHER_PROJECT_PATH = '/home/vimes/projects/temet/some-other-repo';

	const PROJECT: FleetProjectSummary = {
		project_path: PROJECT_PATH,
		session_count: 12,
		request_count: 900,
		first_seen: '2026-08-01T00:00:00Z',
		last_seen: '2026-09-17T09:00:00Z',
		pinned: false,
		custom_name: null
	};

	const SESSION_OPEN = '6ed89307-ecac-4248-99ba-babd6b75c185';
	const SESSION_FAILED = '7fae2b31-4c9a-4e2e-9d7a-2a2f4a6e1c30';

	function rows(openClosed: boolean): LaunchRow[] {
		return [
			buildLaunchRow({
				id: 604,
				backend: 'herdr',
				folder: PROJECT_PATH,
				repo_root: PROJECT_PATH,
				session_uuid: SESSION_OPEN,
				result: 'success',
				channel_check: 'found',
				opened: true,
				delivered: true,
				confirmed: true,
				created_at: '2026-09-17T09:12:00Z',
				opened_at: '2026-09-17T09:12:05Z',
				closed_at: openClosed ? '2026-09-17T09:20:00Z' : null,
				close_reason: openClosed ? 'kill --session' : null,
				close_process_exited: openClosed ? true : null,
				model: 'claude-sonnet-4-5',
				claude_args: ['--session-id', SESSION_OPEN, '--remote-control', '--model', 'sonnet'],
				prompt_present: true,
				brief_mode: 'inline',
				brief_text: 'reply with just the word warm-ok',
				requested_placement: 'tab',
				actual_placement: 'tab:workspace-2'
			}),
			buildLaunchRow({
				id: 603,
				backend: 'tmux',
				folder: PROJECT_PATH,
				repo_root: PROJECT_PATH,
				session_uuid: SESSION_FAILED,
				result: 'failed',
				channel_check: 'timeout',
				opened: true,
				delivered: false,
				confirmed: false,
				reason: 'the launched session never linked a Remote Control channel within the 90s budget',
				stderr_text:
					'tmux: session mike-7fae2b31 created but no client attached\nwarning: pane exited with status 1',
				created_at: '2026-09-17T09:04:00Z',
				opened_at: '2026-09-17T09:04:02Z',
				closed_at: '2026-09-17T09:06:00Z',
				close_reason: 'launch failed',
				close_process_exited: false,
				claude_args: ['--session-id', SESSION_FAILED, '--remote-control']
			}),
			buildLaunchRow({
				id: 602,
				backend: 'herdr',
				folder: OTHER_PROJECT_PATH,
				repo_root: OTHER_PROJECT_PATH,
				result: 'pending',
				opened: true,
				created_at: '2026-09-17T08:40:00Z',
				opened_at: '2026-09-17T08:40:03Z'
			})
		];
	}

	function stub(): void {
		let openClosed = false;
		globalThis.fetch = (async (input: RequestInfo | URL, init?: RequestInit) => {
			const url = typeof input === 'string' ? input : input.toString();
			const method = init?.method ?? 'GET';
			if (method === 'DELETE' && url.includes(`/api/launches/${SESSION_OPEN}`)) {
				openClosed = true;
				return new Response(
					JSON.stringify({
						session: SESSION_OPEN,
						surface: 'herdr:pane-1',
						closed: true,
						process_exited: true
					}),
					{ status: 200, headers: { 'content-type': 'application/json' } }
				);
			}
			if (url.includes('/api/launches')) {
				return new Response(JSON.stringify(rows(openClosed)), {
					status: 200,
					headers: { 'content-type': 'application/json' }
				});
			}
			return new Response('Not Found', { status: 404 });
		}) as typeof fetch;
	}

	const { Story } = defineMeta({
		title: 'Projects/ProjectLaunchesPanel',
		component: ProjectLaunchesPanel,
		tags: ['autodocs']
	});
</script>

<Story
	name="Populated"
	args={{ project: PROJECT }}
	loaders={[
		async () => {
			localStorage.clear();
			stub();
			return {};
		}
	]}
	play={async ({ canvasElement }) => {
		const header = await waitFor(() => {
			const btn = canvasElement.querySelector('button[aria-expanded]');
			if (!btn) throw new Error('header toggle not yet rendered');
			return btn as HTMLButtonElement;
		});
		await expect(header).toHaveAttribute('aria-expanded', 'false');
		await waitFor(() => expect(canvasElement.textContent).toContain('2 launches'));

		await userEvent.click(header);
		await expect(header).toHaveAttribute('aria-expanded', 'true');

		await waitFor(() => {
			const list = canvasElement.querySelectorAll('.plp-row');
			if (list.length !== 2) throw new Error(`expected 2 rows, found ${list.length}`);
		});
		await expect(canvasElement.textContent).toContain('success');
		await expect(canvasElement.textContent).toContain('failed');
		await expect(canvasElement.querySelector('button[aria-label="Expand launch #602"]')).toBeNull();

		const failedToggle = await waitFor(() => {
			const el = canvasElement.querySelector('button[aria-label="Expand launch #603"]');
			if (!el) throw new Error('failed-row toggle not yet rendered');
			return el as HTMLButtonElement;
		});
		await userEvent.click(failedToggle);
		await expect(canvasElement.textContent).toContain(
			'the launched session never linked a Remote Control channel'
		);
		await expect(canvasElement.textContent).toContain('no client attached');
		await expect(canvasElement.textContent).toContain('--session-id');

		const openOnlyToggle = canvasElement.querySelector('.plp-toggle input') as HTMLInputElement;
		await userEvent.click(openOnlyToggle);
		await waitFor(() => {
			const list = canvasElement.querySelectorAll('.plp-row');
			if (list.length !== 1) throw new Error(`expected 1 open row, found ${list.length}`);
		});
		await expect(canvasElement.textContent).not.toContain('failed');
		await userEvent.click(openOnlyToggle);
		await waitFor(() => {
			const list = canvasElement.querySelectorAll('.plp-row');
			if (list.length !== 2)
				throw new Error(`expected 2 rows after clearing the filter, found ${list.length}`);
		});

		function row604(): HTMLElement {
			const toggle = canvasElement.querySelector(
				'button[aria-label="Expand launch #604"], button[aria-label="Collapse launch #604"]'
			);
			const li = toggle?.closest('li');
			if (!li) throw new Error('row #604 not yet rendered');
			return li as HTMLElement;
		}
		const killButton = await waitFor(() => {
			const btn = Array.from(row604().querySelectorAll('button')).find(
				(b) => b.textContent?.trim() === 'Kill'
			);
			if (!btn) throw new Error('Kill button not yet rendered on row #604');
			return btn as HTMLButtonElement;
		});
		await userEvent.click(killButton);
		const confirmButton = await waitFor(() => {
			const btn = Array.from(row604().querySelectorAll('button')).find(
				(b) => b.textContent?.trim() === 'Confirm'
			);
			if (!btn) throw new Error('inline confirm not yet rendered on row #604');
			return btn as HTMLButtonElement;
		});
		await userEvent.click(confirmButton);
		await waitFor(() => {
			const stillOpen = Array.from(row604().querySelectorAll('button')).some((b) =>
				['Kill', 'Confirm'].includes(b.textContent?.trim() ?? '')
			);
			if (stillOpen) throw new Error('row #604 still shows a live kill control after closing');
		});
	}}
>
	{#snippet template(args)}
		<div style="max-width: 760px; background: var(--surface-bg); padding: 16px;">
			<CollapseStoreProvider>
				<ProjectLaunchesPanel {...args} />
			</CollapseStoreProvider>
		</div>
	{/snippet}
</Story>
