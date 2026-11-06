<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { userEvent, within, expect, waitFor } from 'storybook/test';
	import SessionReconcilePanel from './SessionReconcilePanel.svelte';
	import type { ReconcileUsage, SessionReconcile } from '$lib/api/sessionReconcile';

	const { Story } = defineMeta({
		title: 'Shell/SessionReconcilePanel',
		component: SessionReconcilePanel,
		tags: ['autodocs']
	});

	const SESSION_ID = '265ad722-1ba8-497d-b6e4-4360488b01a6';
	const TURN_ID = '27a6ef2a-15ea-4126-9f27-6937cb486135';

	function usage(input: number, output: number, read: number, write: number): ReconcileUsage {
		return {
			input_tokens: input,
			output_tokens: output,
			cache_read_input_tokens: read,
			cache_creation_input_tokens: write
		};
	}

	const MISMATCHED: SessionReconcile = {
		available: true,
		reason: null,
		report: {
			session: SESSION_ID,
			continues: null,
			mod_steps: 62,
			wire_requests: 80,
			matched: 60,
			only_in_mod: [
				{
					turn: TURN_ID,
					index: 7,
					model: 'claude-opus-5-5',
					agent: null,
					usage: usage(2, 1703, 125755, 937)
				},
				{
					turn: TURN_ID,
					index: 8,
					model: 'claude-opus-5-5',
					agent: null,
					usage: usage(2, 783, 126692, 2256)
				}
			],
			only_on_wire: [
				{
					request_id: '708212',
					origin: 'quota_probe',
					model: 'claude-haiku-4-5-20251001',
					usage: null
				},
				{
					request_id: '708213',
					origin: 'title_sidecar',
					model: 'claude-haiku-5-5',
					usage: usage(1180, 26, 0, 0)
				},
				{
					request_id: '708214',
					origin: 'title_sidecar',
					model: 'claude-haiku-5-5',
					usage: usage(1180, 26, 0, 0)
				},
				{
					request_id: '708227',
					origin: 'monitor_sidecar',
					model: 'claude-haiku-5-5',
					usage: null
				},
				{
					request_id: '708246',
					origin: 'primary',
					model: 'claude-opus-5-5',
					usage: usage(689, 169, 31218, 39543)
				},
				{
					request_id: '708533',
					origin: 'monitor_sidecar',
					model: 'claude-haiku-5-5',
					usage: null
				},
				{
					request_id: '708572',
					origin: 'monitor_sidecar',
					model: 'claude-haiku-5-5',
					usage: null
				},
				{
					request_id: '708719',
					origin: 'primary',
					model: 'claude-opus-5-5',
					usage: null
				},
				{
					request_id: '709093',
					origin: 'primary',
					model: 'claude-opus-5-5',
					usage: null
				},
				{
					request_id: '709192',
					origin: 'primary',
					model: 'claude-opus-5-5',
					usage: null
				},
				{
					request_id: '709591',
					origin: 'primary',
					model: 'claude-opus-5-5',
					usage: null
				}
			],
			only_on_wire_by_origin: {
				quota_probe: 1,
				title_sidecar: 2,
				monitor_sidecar: 3,
				primary: 5
			}
		}
	};

	const UNAVAILABLE: SessionReconcile = {
		available: false,
		reason: 'tether is not running at http://127.0.0.1:9970',
		report: null
	};

	const NO_MISMATCHES: SessionReconcile = {
		available: true,
		reason: null,
		report: {
			session: SESSION_ID,
			continues: null,
			mod_steps: 62,
			wire_requests: 62,
			matched: 62,
			only_in_mod: [],
			only_on_wire: [],
			only_on_wire_by_origin: {}
		}
	};

	function stubFetch(status: number, body: unknown): void {
		globalThis.fetch = (async () =>
			new Response(JSON.stringify(body), {
				status,
				headers: { 'content-type': 'application/json' }
			})) as typeof fetch;
	}

	let calls = 0;

	function position(a: HTMLElement, b: HTMLElement): number {
		return a.compareDocumentPosition(b);
	}
</script>

<script lang="ts">
	import { setContext } from 'svelte';
	import { COLLAPSE_STORE_KEY } from '$lib/context-keys';
	import { createCollapseStore } from '$lib/stores/collapseState.svelte';

	setContext(COLLAPSE_STORE_KEY, createCollapseStore());
</script>

<Story
	name="Mismatched"
	args={{ sessionId: SESSION_ID, grouped: false }}
	loaders={[
		async () => {
			localStorage.clear();
			stubFetch(200, MISMATCHED);
			return {};
		}
	]}
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);

		await expect(canvas.queryByText(/Steps with no captured request/)).not.toBeInTheDocument();
		await userEvent.click(
			canvas.getByRole('button', { name: /claude code steps and captured requests/i })
		);

		await waitFor(() =>
			expect(canvas.getByText('Steps with no captured request (2)')).toBeInTheDocument()
		);
		await expect(canvas.getByText('62 steps · 80 requests · 60 matched')).toBeInTheDocument();
		await expect(canvas.getAllByText(TURN_ID)).toHaveLength(2);
		await expect(canvas.getByText('Captured requests no step explains (11)')).toBeInTheDocument();

		const main = canvas.getByText('main · 5');
		const monitor = canvas.getByText('monitor · 3');
		const title = canvas.getByText('title-gen · 2');
		const quota = canvas.getByText('quota-probe · 1');
		await expect(position(main, monitor) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
		await expect(position(monitor, title) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
		await expect(position(title, quota) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();

		const link = canvas.getByRole('link', { name: '#708246' });
		await expect(link.getAttribute('href')).toContain(
			`/sessions/${SESSION_ID}/requests/708246`
		);
		await expect(canvas.getAllByRole('link')).toHaveLength(11);
		await expect(canvas.getByText('689')).toBeInTheDocument();
		await expect(canvas.getByText('31,218')).toBeInTheDocument();
		await expect(canvas.getAllByText('no usage').length).toBeGreaterThanOrEqual(5);
	}}
>
	{#snippet template(args)}
		<div style="max-width: 920px; padding: 12px;">
			<SessionReconcilePanel {...args} />
		</div>
	{/snippet}
</Story>

<Story
	name="Unavailable"
	args={{ sessionId: SESSION_ID, grouped: false }}
	loaders={[
		async () => {
			localStorage.clear();
			stubFetch(200, UNAVAILABLE);
			return {};
		}
	]}
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);
		await userEvent.click(
			canvas.getByRole('button', { name: /claude code steps and captured requests/i })
		);
		await waitFor(() =>
			expect(
				canvas.getByText('tether is not running at http://127.0.0.1:9970', {
					selector: 'p'
				})
			).toBeInTheDocument()
		);
		await expect(canvas.queryByRole('alert')).not.toBeInTheDocument();
		await expect(canvas.queryByRole('table')).not.toBeInTheDocument();
		await expect(canvas.queryByText(/ matched$/)).not.toBeInTheDocument();
		await expect(canvas.getByRole('button', { name: /reload/i })).toBeInTheDocument();
	}}
>
	{#snippet template(args)}
		<div style="max-width: 920px; padding: 12px;">
			<SessionReconcilePanel {...args} />
		</div>
	{/snippet}
</Story>

<Story
	name="No mismatches"
	args={{ sessionId: SESSION_ID, grouped: false }}
	loaders={[
		async () => {
			localStorage.clear();
			stubFetch(200, NO_MISMATCHES);
			return {};
		}
	]}
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);
		await userEvent.click(
			canvas.getByRole('button', { name: /claude code steps and captured requests/i })
		);
		await waitFor(() =>
			expect(canvas.getByText('Every step matched a captured request.')).toBeInTheDocument()
		);
		await expect(canvas.getByText('62 steps · 62 requests · 62 matched')).toBeInTheDocument();
		await expect(canvas.queryByRole('table')).not.toBeInTheDocument();
		await expect(canvas.queryByText(/Steps with no captured request/)).not.toBeInTheDocument();
	}}
>
	{#snippet template(args)}
		<div style="max-width: 920px; padding: 12px;">
			<SessionReconcilePanel {...args} />
		</div>
	{/snippet}
</Story>

<Story
	name="Error"
	args={{ sessionId: SESSION_ID, grouped: false }}
	loaders={[
		async () => {
			localStorage.clear();
			calls = 0;
			globalThis.fetch = (async () => {
				calls += 1;
				return new Response('down', { status: 503, statusText: 'Service Unavailable' });
			}) as typeof fetch;
			return {};
		}
	]}
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);
		await userEvent.click(
			canvas.getByRole('button', { name: /claude code steps and captured requests/i })
		);

		await waitFor(() =>
			expect(canvas.getByText("Couldn't load the step comparison")).toBeInTheDocument()
		);
		await new Promise((resolve) => setTimeout(resolve, 1200));
		await expect(calls).toBe(1);

		await userEvent.click(canvas.getByRole('button', { name: /retry/i }));
		await waitFor(() => expect(calls).toBe(2));
		await new Promise((resolve) => setTimeout(resolve, 1200));
		await expect(calls).toBe(2);
		await expect(canvas.getByText("Couldn't load the step comparison")).toBeInTheDocument();
	}}
>
	{#snippet template(args)}
		<div style="max-width: 920px; padding: 12px;">
			<SessionReconcilePanel {...args} />
		</div>
	{/snippet}
</Story>
