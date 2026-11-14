<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { expect, userEvent, waitFor, within } from 'storybook/test';
	import ReviewPage from './+page.svelte';
	import type { EvalReviewPageData } from './+page';
	import type { EvalLabelDto } from '$lib/api/eval';
	import type { EvalReviewDetailDto, EvalRunItemDto, ItemPrelabel } from '$lib/api/evalRuns';
	import type { OperatorTurnDto, SessionSummary } from '$lib/types';
	import {
		DETAIL_10487,
		DETAIL_10571,
		DETAIL_12180,
		FACTS_10487,
		FACTS_12180,
		QUOTE_10487,
		RUN,
		RUN3_DELIVERY_FIRST_10,
		SESSION,
		SESSION_10487,
		SUMMARY_24369,
		TURNS_24369,
		TURN_10487,
		TURN_10571,
		TURN_12180,
		VOCABULARY,
		detailFor,
		facts,
		label,
		runItemsPage,
		runRow
	} from '$lib/test-support/evalReviewFixtures';

	const DELIVERY: EvalRunItemDto[] = [
		...RUN3_DELIVERY_FIRST_10.map((r) =>
			r.operator_turn_id === 10487 ? TURN_10487 : r.operator_turn_id === 12180 ? TURN_12180 : r
		),
		...Array.from({ length: 53 }, (_, i) =>
			runRow(20000 + i, ['accepted', 'partial'], ['accepted', 'delivered'])
		)
	];
	const REACTION: EvalRunItemDto[] = [
		TURN_10571,
		TURN_12180,
		...Array.from({ length: 36 }, (_, i) =>
			runRow(30000 + i, ['corrected', 'delivered'], ['accepted', 'delivered'])
		)
	];
	const PRELABELS: Record<number, ItemPrelabel> = {
		10571: { label: 'redirected', probability: 0.55, uncertain: true },
		30034: { label: 'corrected', probability: 0.61, uncertain: true },
		...Object.fromEntries(
			Array.from({ length: 34 }, (_, i) => [
				30000 + i,
				{ label: 'corrected', probability: 0.93, uncertain: false } satisfies ItemPrelabel
			])
		)
	};
	const DETAILS: Record<number, EvalReviewDetailDto> = {
		10487: DETAIL_10487,
		10571: DETAIL_10571,
		12180: DETAIL_12180
	};

	function turnsOf(session: string, ids: number[]): OperatorTurnDto[] {
		return ids.map((turn_id, i) => ({
			...TURNS_24369[i % TURNS_24369.length]!,
			turn_id,
			session_id: session
		}));
	}

	const SESSIONS: Record<string, { summary: SessionSummary; turns: OperatorTurnDto[] }> = {
		[SESSION_10487]: {
			summary: {
				...SUMMARY_24369,
				id: SESSION_10487,
				title: 'Build a sign-in timing extension',
				project: null,
				cwd: '/work/signin-tracker'
			},
			turns: turnsOf(SESSION_10487, [10487, 10493, 10500, 10510, 10520, 10530, 10540, 10550])
		},
		[SESSION]: {
			summary: { ...SUMMARY_24369, id: SESSION },
			turns: turnsOf(SESSION, [10560, 10571, 10954, 12180, 12181])
		}
	};

	interface Post {
		subject: { kind: string; turn_id: number };
		labels: Array<{ axis: string; label: string }>;
		note: string | null;
		context: string;
		eval_run_id: number;
	}

	function json(body: unknown, status = 200) {
		return Promise.resolve(
			new Response(JSON.stringify(body), {
				status,
				headers: { 'content-type': 'application/json' }
			})
		);
	}

	function fakeServer() {
		const posts: Post[] = [];
		const reviews = new Map<number, EvalLabelDto[]>();

		function withReviews(row: EvalRunItemDto): EvalRunItemDto {
			const saved = reviews.get(row.operator_turn_id);
			if (!saved) return row;
			const now = (axis: string, fallback: string | null) =>
				saved.find((l) => l.axis === axis)?.label ?? fallback;
			return {
				...row,
				reviewed_at: saved[0]?.created_at ?? null,
				current_reaction: now('reaction', row.current_reaction ?? null),
				current_delivery: now('delivery', row.current_delivery ?? null)
			};
		}

		function detail(turn: number): EvalReviewDetailDto | null {
			const row = [...DELIVERY, ...REACTION].find((r) => r.operator_turn_id === turn);
			if (!row) return null;
			const base = DETAILS[turn] ?? detailFor(row);
			return { ...base, labels: { ...base.labels, reviews: reviews.get(turn) ?? [] } };
		}

		function fetchStub(input: RequestInfo | URL, init?: RequestInit): Promise<Response> {
			const url =
				typeof input === 'string' ? input : input instanceof URL ? input.toString() : input.url;
			const path = new URL(url, 'http://x').pathname;
			const session = /^\/api\/sessions\/([^/]+)(\/operator-turns)?$/.exec(path);
			if (session) {
				const found = SESSIONS[decodeURIComponent(session[1] ?? '')];
				if (!found) return json({ error: 'no session' }, 404);
				return session[2] ? json(found.turns) : json({ summary: found.summary, requests: [] });
			}
			if (url.startsWith('/api/operator-turns/')) {
				const turn = Number(url.split('/').pop());
				if (turn === 10571) return json({ error: 'internal error' }, 500);
				if (turn === 10487) return json(FACTS_10487);
				if (turn === 12180) return json(FACTS_12180);
				return json(facts(turn));
			}
			if (url.includes('/review')) {
				const found = detail(Number(url.split('/items/')[1]?.split('/')[0]));
				return found ? json(found) : json({ error: 'no item' }, 404);
			}
			if (url.startsWith('/api/eval/runs/3/items')) {
				const rows = (url.includes('disagreement=reaction') ? REACTION : DELIVERY)
					.map(withReviews)
					.map((r) => {
						const prelabel = PRELABELS[r.operator_turn_id];
						return prelabel ? { ...r, prelabel } : r;
					});
				const only = url.includes('review=uncertain');
				return json(
					runItemsPage(only ? rows.filter((r) => !r.prelabel || r.prelabel.uncertain) : rows)
				);
			}
			if (url.startsWith('/api/eval/labels') && init?.method === 'POST') {
				const body = JSON.parse(String(init.body)) as Post;
				posts.push(body);
				const turn = body.subject.turn_id;
				const at = new Date(2026, 8, 28, 11, 40 + posts.length).toISOString();
				const written = body.labels.map((l, i) =>
					label(turn, l.axis, l.label, {
						id: 900000 + posts.length * 10 + i,
						tier: 'gold',
						method: 'operator_click',
						evidence: null,
						severity: null,
						note: body.note,
						provenance: { context: 'eval_review', eval_run_id: 3 },
						created_at: at
					})
				);
				reviews.set(turn, [...written, ...(reviews.get(turn) ?? [])]);
				return json({ labels: written }, 201);
			}
			if (url.startsWith('/api/eval/labels')) {
				const turn = Number(new URL(url, 'http://x').searchParams.get('turn_id'));
				const d = detail(turn);
				return json({
					labels: d ? [...d.labels.reviews, ...d.labels.run_gold] : [],
					next_before_id: null,
					verdict_now: null
				});
			}
			return json({ error: 'unexpected fetch' }, 503);
		}

		return { posts, fetchStub };
	}

	let server = fakeServer();

	const DATA: EvalReviewPageData = {
		runId: 3,
		run: RUN,
		runError: null,
		vocabulary: VOCABULARY,
		vocabularyError: null
	};
	const BANNED =
		/\b(gold|answer key|ground truth|recorded label|axis|verdict|tier|seed|holdout|candidate|bundle|kappa|method|eval|rubric|adjudicated|test runs?|go-live|disagreements?)\b|κ|\u2026|characters/i;

	async function press(el: HTMLElement) {
		await waitFor(() => expect(getComputedStyle(el).pointerEvents).not.toBe('none'));
		await userEvent.click(el);
	}

	function edges(root: HTMLElement): number[] {
		return [...root.querySelectorAll<HTMLElement>('[data-testid="decision-bar"] button')].map(
			(b) => b.getBoundingClientRect().left
		);
	}

	const { Story } = defineMeta({
		title: 'Routes/Sessions/ReviewLabels',
		component: ReviewPage,
		parameters: {
			layout: 'fullscreen',
			sveltekit_experimental: {
				state: {
					page: {
						url: new URL(
							'http://localhost/sessions/labelling/accuracy/3/review?mode=delivery&turn=10487'
						),
						params: { runId: '3' }
					}
				}
			}
		},
		argTypes: { data: { control: false } }
	});
</script>

<Story
	name="Reviewing labels"
	loaders={[
		async () => {
			server = fakeServer();
			try {
				sessionStorage.removeItem('judges:session-band');
			} catch {
				// storage can be blocked; the page falls back to its defaults
			}
			globalThis.fetch = server.fetchStub as typeof globalThis.fetch;
			return {};
		}
	]}
	args={{ data: DATA }}
	play={async ({ canvasElement }) => {
		const c = within(canvasElement);
		const page = within(document.body);
		const bar = () => within(c.getByTestId('decision-bar'));

		await expect(c.getByRole('heading', { level: 1 }).textContent?.trim()).toBe('Review labels');
		await expect(c.getByTestId('run-link').getAttribute('href')).toBe(
			'/sessions/labelling/accuracy/3'
		);
		await expect(c.getByTestId('run-link').textContent).toMatch(
			/^Labelling accuracy, 25 Sep( 2026)? \d\d:\d\d · configuration 15 \(gemini-3\.8-flash\)$/
		);
		await expect(c.getByTestId('intro-count').textContent?.replace(/\s+/g, ' ').trim()).toBe(
			"The judge's answer and the turn's label differ on 63 turns in this accuracy run. One of them is wrong."
		);
		await waitFor(() =>
			expect(
				c.getByRole('heading', { level: 2, name: 'What did the assistant deliver?' })
			).toBeInTheDocument()
		);
		await waitFor(() => expect(c.getByTestId('turn-meta').textContent).toMatch(/Turn 1 of 8/));
		await waitFor(() =>
			expect(c.getByTestId('running-words').textContent).toBe('still running when you replied')
		);
		await expect(c.queryByTestId('just-before')).toBeNull();
		await expect(c.getByTestId('session-band').querySelector('[aria-expanded]')).toHaveAttribute(
			'aria-expanded',
			'true'
		);
		await expect(c.getByRole('tab', { name: /What was delivered\s*63/ })).toHaveAttribute(
			'data-state',
			'active'
		);
		await expect(c.getByTestId('review-progress').textContent).toMatch(/0\s+of\s+63\s+reviewed/);
		await expect(bar().getByRole('button', { name: /^Partial\s*the label$/i })).toBeInTheDocument();
		await expect(
			bar().getByRole('button', { name: /^Delivered\s*the judge$/i })
		).toBeInTheDocument();
		const answers = [
			...c.getByTestId('decision-bar').querySelectorAll<HTMLElement>('button[data-label]')
		];
		await expect(answers.map((b) => b.getAttribute('data-label'))).toEqual([
			'partial',
			'delivered',
			'shipped',
			'none',
			'question',
			'not_applicable'
		]);
		await expect(
			new Set(answers.map((b) => Math.round(b.getBoundingClientRect().height))).size
		).toBe(1);
		await expect(c.getByTestId('other-axis-note').textContent).toBe(
			'How you reacted is not in dispute: the label and the judge both said Accepted when the judge was tested. The label has since been changed to Redirected.'
		);
		await expect(c.getByTestId('judge-saw').textContent?.replace(/\s+/g, ' ').trim()).toBe(
			"The judge did not see the assistant's last message to you. Its answer rests on incomplete input, so it says little about how good the judge is."
		);
		await expect(c.getByTestId('judge-missed')).toBeInTheDocument();
		await expect(canvasElement.querySelector('mark')?.textContent).toBe(QUOTE_10487);
		await expect(c.getByTestId('turn-link').getAttribute('href')).toBe(
			`/sessions/${SESSION_10487}?request=212616`
		);
		await expect(canvasElement.querySelectorAll('a[href*="/requests/"]')).toHaveLength(0);
		await expect(c.getByTestId('open-session').getAttribute('href')).toBe(
			`/sessions/${SESSION_10487}`
		);
		await expect(canvasElement.textContent).not.toMatch(BANNED);
		const labelled = [...canvasElement.querySelectorAll('[aria-label]')].map(
			(el) => el.getAttribute('aria-label') ?? ''
		);
		await expect(labelled.filter((l) => BANNED.test(l))).toEqual([]);

		const before = edges(canvasElement);
		await press(bar().getByRole('button', { name: /^Partial/i }));
		await waitFor(() => expect(server.posts).toHaveLength(1));
		await expect(server.posts[0]).toEqual({
			subject: { kind: 'turn', turn_id: 10487 },
			labels: [{ axis: 'delivery', label: 'partial' }],
			note: null,
			context: 'eval_review',
			eval_run_id: 3
		});
		await waitFor(() => expect(c.getByTestId('turn-meta').textContent).toMatch(/Turn 3 of 5/));
		await waitFor(() =>
			expect(c.getByTestId('decision-status').textContent).toMatch(
				/Saved the last turn as Partial\.\s*Go back/
			)
		);
		await waitFor(() =>
			expect(c.getByTestId('review-progress').textContent).toMatch(/1\s+of\s+63\s+reviewed/)
		);
		await expect(c.getByTestId('just-before').textContent).toContain('Just before · turn 2');
		await expect(document.activeElement?.textContent).toBe('What did the assistant deliver?');

		await press(c.getByRole('button', { name: 'Go back' }));
		await waitFor(() => expect(c.getByTestId('turn-meta').textContent).toMatch(/Turn 1 of 8/));
		await waitFor(() =>
			expect(bar().getByRole('button', { name: /^Partial/i })).toHaveAttribute(
				'aria-pressed',
				'true'
			)
		);
		await expect(c.getByTestId('session-band').querySelector('[aria-expanded]')).toHaveAttribute(
			'aria-expanded',
			'false'
		);
		await expect(c.getByTestId('decision-status').textContent).toMatch(
			/^You chose Partial on \d{1,2} Sep 2026 \d\d:\d\d\. Pick another to change it\.$/
		);
		await expect(edges(canvasElement)).toEqual(before);
		await press(bar().getByRole('button', { name: /^Partial/i }));
		await expect(c.getByTestId('decision-status').textContent).toBe('Already saved as Partial.');
		await expect(server.posts).toHaveLength(1);

		await press(c.getByRole('button', { name: /All turns/ }));
		await waitFor(() =>
			expect(
				page.getByText(/^What was delivered · Labelling accuracy, 25 Sep( 2026)? \d\d:\d\d$/)
			).toBeInTheDocument()
		);
		await waitFor(() =>
			expect((document.querySelector('[data-turn="10487"]') as HTMLElement).textContent).toContain(
				'Build a sign-in timing extension'
			)
		);
		await userEvent.click(document.querySelector('[data-turn="12180"]') as HTMLElement);
		await waitFor(() => expect(c.getByTestId('turn-meta').textContent).toMatch(/Turn 4 of 5/));
		const h2s = c.getAllByRole('heading', { level: 2 }).map((h) => h.textContent);
		await expect(h2s).toEqual([
			'What did the assistant deliver?',
			'How did you react in your next message?'
		]);
		const rows = c.getAllByRole('group', { name: /What was delivered|How you reacted/ });
		await expect(rows).toHaveLength(2);
		await press(within(rows[0] as HTMLElement).getByRole('button', { name: /^Partial/i }));
		await expect(c.getByTestId('two-questions')).toBeInTheDocument();
		await expect(c.getByTestId('decision-status').textContent).toBe(
			'Now answer the other row; the turn saves once both have an answer.'
		);
		await expect(server.posts).toHaveLength(1);
		await press(within(rows[1] as HTMLElement).getByRole('button', { name: /^Corrected/i }));
		await waitFor(() => expect(server.posts).toHaveLength(2));
		await expect(server.posts[1]?.labels).toEqual([
			{ axis: 'delivery', label: 'partial' },
			{ axis: 'reaction', label: 'corrected' }
		]);

		await press(c.getByRole('tab', { name: /How you reacted\s*38/ }));
		await waitFor(() =>
			expect(
				c.getByRole('heading', { level: 2, name: 'How did you react in your next message?' })
			).toBeInTheDocument()
		);
		await waitFor(() => expect(c.getByTestId('turn-meta').textContent).toMatch(/Turn 2 of 5/));
		await waitFor(() =>
			expect(c.getByTestId('facts-error').textContent).toMatch(
				/could not be loaded \(server error 500\)\.\s*Try again/
			)
		);
		await expect(c.getByTestId('assistant-text').textContent?.trim()).toBe(
			'The assistant wrote no text in this turn.'
		);
		await expect(c.getByTestId('next-message').querySelector('mark')?.textContent).toBe(
			'What is the best month to visit Kyoto?'
		);
		await expect(bar().getByRole('button', { name: /^Unclear\s*the label$/i })).toBeInTheDocument();
		await expect(
			bar().getByRole('button', { name: /^Corrected\s*the judge$/i })
		).toBeInTheDocument();

		await userEvent.keyboard('j');
		await waitFor(() => expect(c.getByTestId('turn-meta').textContent).toMatch(/Turn 4 of 5/));
		await userEvent.keyboard('k');
		await waitFor(() => expect(c.getByTestId('turn-meta').textContent).toMatch(/Turn 2 of 5/));
		await userEvent.keyboard('{Alt>}1{/Alt}');
		await waitFor(() => expect(server.posts).toHaveLength(3));
		await expect(server.posts[2]?.labels).toEqual([{ axis: 'reaction', label: 'unclear' }]);
	}}
/>

<Story
	name="Only uncertain pre-labels"
	parameters={{
		sveltekit_experimental: {
			state: {
				page: {
					url: new URL('http://localhost/sessions/labelling/accuracy/3/review?mode=reaction'),
					params: { runId: '3' }
				}
			}
		}
	}}
	loaders={[
		async () => {
			server = fakeServer();
			globalThis.fetch = server.fetchStub as typeof globalThis.fetch;
			return {};
		}
	]}
	args={{ data: DATA }}
	play={async ({ canvasElement }) => {
		const c = within(canvasElement);
		const hidden = () => c.getByTestId('hidden-confident');
		const only = () => c.getByRole('switch', { name: /Only uncertain/ });

		await waitFor(() => expect(only()).toHaveAttribute('aria-checked', 'true'));
		await waitFor(() => expect(hidden().textContent).toBe('34 confident pre-labels hidden'));
		await waitFor(() =>
			expect(c.getByTestId('review-progress').textContent).toMatch(/of\s+4\s+reviewed/)
		);
		await waitFor(() =>
			expect(c.getByTestId('prelabel-suggestion').textContent).toBe('Jev suggests Redirected, 55%')
		);
		await expect(
			canvasElement.querySelectorAll('[data-testid="decision-bar"] button[aria-pressed="true"]')
		).toHaveLength(0);

		const slot = hidden().getBoundingClientRect();
		await press(only());
		await waitFor(() => expect(only()).toHaveAttribute('aria-checked', 'false'));
		await waitFor(() =>
			expect(c.getByTestId('review-progress').textContent).toMatch(/of\s+38\s+reviewed/)
		);
		await expect(hidden().textContent).toBe('');
		await expect(hidden().getBoundingClientRect().width).toBe(slot.width);
		await expect(server.posts).toHaveLength(0);
	}}
/>
