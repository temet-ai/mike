<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { expect, userEvent, within, waitFor } from 'storybook/test';
	import GoldLabelControl from './GoldLabelControl.svelte';
	import type { EvalLabelDto } from '$lib/api/eval';
	import type { SessionNoteDto } from '$lib/comms/sessionNotes';

	const VOCABULARY = {
		axes: [
			{
				axis: 'reaction',
				subject_kinds: ['turn'],
				labels: ['corrected', 'redirected', 'accepted', 'no_response', 'unclear'].map(
					(label, i) => ({
						label,
						sort_order: i + 1,
						description: `${label} description`
					})
				)
			},
			{
				axis: 'delivery',
				subject_kinds: ['turn'],
				labels: ['shipped', 'delivered', 'partial', 'none', 'question', 'not_applicable'].map(
					(label, i) => ({ label, sort_order: i + 1, description: `${label} description` })
				)
			}
		]
	};

	let stored: EvalLabelDto[] = [];
	let storedNotes: SessionNoteDto[] = [];
	let nextId = 10;

	function gold(axis: string, label: string): EvalLabelDto {
		nextId += 1;
		return {
			id: nextId,
			subject_kind: 'turn',
			subject_id: 7,
			operator_turn_id: 7,
			session_uuid: 's',
			axis,
			label,
			action: 'assert',
			tier: 'gold',
			method: 'operator_click',
			split: 'dev',
			tags: [],
			severity: null,
			confidence: null,
			evidence: null,
			note: null,
			source: 'spa',
			provenance: {},
			verdict_snapshot: null,
			supersedes_id: null,
			created_at: '2026-09-24T12:00:00Z',
			is_current: true
		};
	}

	function json(body: unknown, status = 200) {
		return Promise.resolve(
			new Response(JSON.stringify(body), {
				status,
				headers: { 'content-type': 'application/json' }
			})
		);
	}

	function stub(input: RequestInfo | URL, init?: RequestInit): Promise<Response> {
		const url = typeof input === 'string' ? input : input instanceof URL ? input.href : input.url;
		if (url.includes('/api/eval/vocabulary')) return json(VOCABULARY);
		if (url.includes('/api/sessions/') && url.includes('/notes')) {
			if (init?.method === 'POST') {
				const body = JSON.parse(String(init.body)) as Partial<SessionNoteDto>;
				const created: SessionNoteDto = {
					note_uuid: `note-${storedNotes.length + 1}`,
					kind: body.kind ?? 'note',
					title: body.title ?? '',
					body: body.body ?? '',
					status: null,
					tags: body.tags ?? [],
					data: body.data ?? null,
					created_at: '2026-09-29T12:00:00Z',
					updated_at: '2026-09-29T12:00:00Z'
				};
				storedNotes = [...storedNotes, created];
				return json(created, 201);
			}
			return json(storedNotes);
		}
		if (url.includes('/withdraw')) {
			const id = Number(url.split('/').at(-2));
			stored = stored.map((l) => (l.id === id ? { ...l, is_current: false } : l));
			return json({ label: { ...gold('reaction', 'accepted'), action: 'withdraw' } }, 201);
		}
		if (url.includes('/api/eval/labels') && init?.method === 'POST') {
			const body = JSON.parse(String(init.body)) as {
				labels: Array<{ axis: string; label: string }>;
			};
			const rows = body.labels.map((l) => gold(l.axis, l.label));
			stored = [...stored.map((l) => ({ ...l, is_current: false })), ...rows];
			return json({ labels: rows }, 201);
		}
		if (url.includes('/api/eval/labels')) {
			return json({
				labels: stored,
				next_before_id: null,
				verdict_now: {
					outcome: 'accepted',
					judge_model: 'google/gemini-3.8-flash',
					rubric_version: 'r3',
					bundle_id: 12
				}
			});
		}
		return json({ error: 'unexpected' }, 503);
	}

	const { Story } = defineMeta({
		title: 'Eval/GoldLabelControl',
		component: GoldLabelControl,
		tags: ['autodocs']
	});
</script>

<Story
	name="Default"
	loaders={[
		async () => {
			stored = [];
			storedNotes = [];
			globalThis.fetch = stub as typeof globalThis.fetch;
			return {};
		}
	]}
	args={{
		subject: { kind: 'turn', turnId: 7 },
		axes: ['reaction', 'delivery'],
		context: 'turn_card',
		sessionId: 'session-1'
	}}
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);
		await waitFor(() => expect(canvasElement.textContent).toContain('bundle #12'));
		const save = canvas.getByRole('button', { name: 'Save' });
		await expect(save).toBeDisabled();
		await userEvent.type(canvas.getByRole('textbox', { name: 'Note' }), 'Looks fine to me');
		await expect(save).toBeEnabled();
		await userEvent.click(save);
		await waitFor(() => expect(canvas.getByText('Your notes')).toBeTruthy());
		await waitFor(() => expect(canvas.getByText('Looks fine to me')).toBeTruthy());
		await expect(canvas.getByRole('link', { name: 'session notebook' })).toBeTruthy();
		await expect(save).toBeDisabled();
		await userEvent.click(canvas.getByRole('radio', { name: 'Corrected' }));
		await userEvent.click(canvas.getByRole('radio', { name: 'Partial' }));
		await expect(save).toBeEnabled();
		await userEvent.click(save);
		await waitFor(() => expect(canvas.getByRole('status').textContent).toMatch(/^Saved \d/));
		await waitFor(() => expect(canvasElement.querySelectorAll('[data-label-id]').length).toBe(2));
		await expect(canvasElement.textContent).not.toContain('()');
		await expect(canvasElement.textContent).not.toContain('reaction');
		await expect(canvasElement.textContent).not.toContain('delivery');
		const [firstWithdraw] = canvas.getAllByRole('button', { name: 'Withdraw' });
		if (!firstWithdraw) throw new Error('expected a Withdraw button');
		await userEvent.click(firstWithdraw);
		await waitFor(() =>
			expect(canvas.queryAllByRole('button', { name: 'Withdraw' }).length).toBe(1)
		);
	}}
/>
