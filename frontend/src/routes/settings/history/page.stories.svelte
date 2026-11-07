<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { expect, userEvent, within, waitFor } from 'storybook/test';
	import HistoryPage from './+page.svelte';
	import type { ChangeLogRow } from '$lib/api/configHistory';
	import type { HistoryPageData } from './+page';

	const LONG_PROMPT =
		'You judge whether the operator had to correct the assistant. '.repeat(120) + 'END-OF-PROMPT';

	function row(id: number, o: Partial<ChangeLogRow> = {}): ChangeLogRow {
		return {
			id,
			changed_at: new Date(Date.UTC(2026, 8, 24, 12, 0, id)).toISOString(),
			xact_id: String(1000 + id),
			subject_kind: 'setting',
			subject_key: 'judge_worker.model',
			op: 'update',
			old_value: 'openai/gpt-4o-mini',
			new_value: 'anthropic/claude-haiku-4-5',
			redacted: false,
			via: 'spa',
			actor: 'Mozilla/5.0',
			request_id: `req-${id}`,
			build_sha: 'cb761820aa',
			db_user: 'devuser',
			application_name: 'claude-proxy',
			detail: {},
			...o
		};
	}

	const FIRST_PAGE: ChangeLogRow[] = [
		row(30, {
			subject_kind: 'steerer',
			subject_key: '4',
			op: 'update',
			old_value: { id: 4, name: 'verifier', prompt: LONG_PROMPT },
			new_value: { id: 4, name: 'verifier', prompt: LONG_PROMPT + ' v2' }
		}),
		row(29, { via: 'sql', actor: null }),
		row(28, {
			subject_key: 'integrations.slack.token',
			redacted: true,
			old_value: { redacted: true },
			new_value: { redacted: true }
		})
	];

	const OLDER_PAGE: ChangeLogRow[] = [
		row(27, { via: 'api' }),
		row(26, { op: 'insert', old_value: null })
	];

	function buildData(): HistoryPageData {
		return { page: { changes: FIRST_PAGE, next_before_id: 28 }, loadError: null };
	}

	function fetchStub(input: RequestInfo | URL): Promise<Response> {
		const url =
			typeof input === 'string'
				? input
				: input instanceof URL
					? input.href
					: (input as Request).url;
		const body = url.includes('before_id=28')
			? { changes: OLDER_PAGE, next_before_id: null }
			: { changes: FIRST_PAGE, next_before_id: 28 };
		return Promise.resolve(
			new Response(JSON.stringify(body), {
				status: 200,
				headers: { 'content-type': 'application/json' }
			})
		);
	}

	const { Story } = defineMeta({
		title: 'Routes/Settings/History',
		component: HistoryPage,
		tags: ['autodocs'],
		parameters: { layout: 'fullscreen' }
	});
</script>

<Story
	name="Default"
	loaders={[
		async () => {
			globalThis.fetch = ((input: RequestInfo | URL) =>
				fetchStub(input)) as typeof globalThis.fetch;
			return {};
		}
	]}
	args={{ data: buildData() }}
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);
		await expect(canvasElement.textContent).toContain('Configuration history');
		await expect(canvasElement.textContent).toContain('END-OF-PROMPT v2');
		await expect(canvasElement.textContent).toContain('value withheld: secret-shaped key');
		await expect(canvasElement.textContent).toContain('changed by a direct database edit (psql)');
		const before = [...canvasElement.querySelectorAll('[data-change-id]')].map((el) =>
			el.getAttribute('data-change-id')
		);
		await expect(before).toEqual(['30', '29', '28']);
		await userEvent.click(canvas.getByRole('button', { name: 'Load older' }));
		await waitFor(() => expect(canvasElement.querySelectorAll('[data-change-id]').length).toBe(5));
		const after = [...canvasElement.querySelectorAll('[data-change-id]')].map((el) =>
			el.getAttribute('data-change-id')
		);
		await expect(after).toEqual(['30', '29', '28', '27', '26']);
		await expect(canvasElement.textContent).toContain('This is the start of the change log.');
	}}
/>
