<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { within, expect, userEvent, waitFor, fn, fireEvent } from 'storybook/test';
	import EmbeddingGraph from './EmbeddingGraph.svelte';
	import { EmbeddingGraphStore } from '$lib/graph/embeddingGraphStore.svelte';
	import type { SessionEmbeddingGraph } from '$lib/api/embeddingGraph';
	import type { SessionFetcher } from './SessionPreviewModal.svelte';
	import type { SessionDetail } from '$lib/types';

	const stubFetchSession: SessionFetcher = async (id) =>
		({
			summary: {
				id,
				project: '/Users/vimes/projects/temet/temet-proxy',
				project_id: 12,
				cwd: null,
				project_resolution: 'operator',
				first_seen: '2026-08-28T15:00:00Z',
				last_seen: '2026-08-28T15:40:00Z',
				request_count: 42,
				models: ['claude-opus-4'],
				last_status: 'Complete',
				title: null
			},
			requests: [],
			quality: {
				outcome: 'success',
				quality_score: 0.96,
				rationale: null,
				judge_model: null,
				rubric_version: null,
				judged_at: null
			},
			token_usage: {
				input_tokens: 12000,
				output_tokens: 6200,
				cache_read_tokens: 0,
				cache_write_tokens: 0,
				response_count: 42,
				by_model: []
			},
			total_requests: 42
		}) as SessionDetail;

	const CENTRE = 'c0000000-0000-4000-8000-000000000000';
	const N1 = 'c0000000-0000-4000-8000-000000000001';
	const N2 = 'c0000000-0000-4000-8000-000000000002';
	const N3 = 'c0000000-0000-4000-8000-000000000003';
	const NEW_NEIGHBOUR = 'c0000000-0000-4000-8000-000000000099';
	const manyNodesId = (i: number): string =>
		`b0000000-0000-4000-8000-${String(i).padStart(12, '0')}`;

	function baseGraph(): SessionEmbeddingGraph {
		return {
			session_id: CENTRE,
			embedding: {
				present: true,
				source: 'overview',
				model: 'nomic-embed-text-v1.5',
				embedded_at: '2026-09-01T12:00:00Z',
				chunk_count: 14
			},
			nodes: [
				{
					session_id: CENTRE,
					title: 'Embedding graph on the session and project pages',
					project_id: 12,
					project_path: '/Users/vimes/projects/temet/temet-proxy',
					outcome: 'accepted',
					last_seen: '2026-09-01T13:05:38Z',
					last_status: 'Complete',
					similarity: 1,
					source: 'overview',
					topics: ['embeddings', 'session page'],
					is_centre: true
				},
				{
					session_id: N1,
					title: 'Session vector backfill sweep',
					project_id: 12,
					project_path: '/Users/vimes/projects/temet/temet-proxy',
					outcome: 'shipped',
					last_seen: '2026-08-30T09:12:00Z',
					last_status: 'Complete',
					similarity: 0.86,
					source: 'centroid',
					topics: ['embeddings'],
					is_centre: false
				},
				{
					session_id: N2,
					title: 'Judge neighbour context and topics/learnings output',
					project_id: 12,
					project_path: '/Users/vimes/projects/temet/temet-proxy',
					outcome: 'corrected',
					last_seen: '2026-08-28T15:40:00Z',
					last_status: 'Complete',
					similarity: 0.71,
					source: 'overview',
					topics: ['judge'],
					is_centre: false
				},
				{
					session_id: N3,
					title: 'Session-level embedding audit',
					project_id: 12,
					project_path: '/Users/vimes/projects/temet/temet-proxy',
					outcome: null,
					last_seen: '2026-08-20T10:00:00Z',
					last_status: 'Complete',
					similarity: 0.42,
					source: 'centroid',
					topics: [],
					is_centre: false
				}
			],
			edges: [
				{ from: CENTRE, to: N1, similarity: 0.86 },
				{ from: CENTRE, to: N2, similarity: 0.71 },
				{ from: CENTRE, to: N3, similarity: 0.42 },
				{ from: N1, to: N2, similarity: 0.61 }
			]
		};
	}

	function neighbourGraphOf(id: string): SessionEmbeddingGraph {
		return {
			session_id: id,
			embedding: {
				present: true,
				source: 'overview',
				model: 'nomic-embed-text-v1.5',
				embedded_at: '2026-09-01T12:00:00Z',
				chunk_count: 14
			},
			nodes: [
				{
					session_id: id,
					title: 'Session vector backfill sweep',
					project_id: 12,
					project_path: '/Users/vimes/projects/temet/temet-proxy',
					outcome: 'shipped',
					last_seen: '2026-08-30T09:12:00Z',
					last_status: 'Complete',
					similarity: 1,
					source: 'overview',
					topics: ['embeddings'],
					is_centre: true
				},
				{
					session_id: CENTRE,
					title: 'Embedding graph on the session and project pages',
					project_id: 12,
					project_path: '/Users/vimes/projects/temet/temet-proxy',
					outcome: 'accepted',
					last_seen: '2026-09-01T13:05:38Z',
					last_status: 'Complete',
					similarity: 0.86,
					source: 'overview',
					topics: ['embeddings'],
					is_centre: false
				},
				{
					session_id: NEW_NEIGHBOUR,
					title: 'Brand-new neighbour pulled in by expand',
					project_id: 12,
					project_path: '/Users/vimes/projects/temet/temet-proxy',
					outcome: 'shipped',
					last_seen: '2026-08-31T10:00:00Z',
					last_status: 'Complete',
					similarity: 0.68,
					source: 'centroid',
					topics: ['embeddings'],
					is_centre: false
				}
			],
			edges: [
				{ from: id, to: CENTRE, similarity: 0.86 },
				{ from: id, to: NEW_NEIGHBOUR, similarity: 0.68 }
			]
		};
	}

	function manyNodesGraph(count: number): SessionEmbeddingGraph {
		const titles = [
			'Session vector backfill sweep',
			'Judge neighbour context and topics/learnings output',
			'Session-level embedding audit',
			'Filter-rule dry-run coverage pass across the fleet',
			'Steerer firing-timing study',
			'Mailbox hardening phase one',
			'Fleet coordination race repair',
			'Prompt composition splice review',
			'Weasel intercept false-positive sweep',
			'Capture-DB migration squash',
			'Lesson-relations evidence backfill',
			'Diagnostics RSS sampling fix'
		];
		const nodes: SessionEmbeddingGraph['nodes'] = [
			{
				session_id: CENTRE,
				title: 'Embedding graph on the session and project pages',
				project_id: 12,
				project_path: '/Users/vimes/projects/temet/temet-proxy',
				outcome: 'accepted',
				last_seen: '2026-09-01T13:05:38Z',
				last_status: 'Complete',
				similarity: 1,
				source: 'overview',
				topics: ['embeddings', 'session page'],
				is_centre: true
			}
		];
		const edges: SessionEmbeddingGraph['edges'] = [];
		for (let i = 0; i < count - 1; i++) {
			const id = manyNodesId(i);
			const similarity = 0.35 + ((i * 7) % 60) / 100;
			nodes.push({
				session_id: id,
				title: titles[i % titles.length]!,
				project_id: 12,
				project_path: '/Users/vimes/projects/temet/temet-proxy',
				outcome: i % 3 === 0 ? 'shipped' : i % 3 === 1 ? 'corrected' : null,
				last_seen: '2026-08-30T09:12:00Z',
				last_status: 'Complete',
				similarity,
				source: 'overview',
				topics: i % 2 === 0 ? ['embeddings'] : ['embeddings', 'judge'],
				is_centre: false
			});
			edges.push({ from: CENTRE, to: id, similarity });
			if (i > 0) edges.push({ from: manyNodesId(i - 1), to: id, similarity: 0.2 + (i % 4) * 0.1 });
		}
		return {
			session_id: CENTRE,
			embedding: {
				present: true,
				source: 'overview',
				model: 'nomic-embed-text-v1.5',
				embedded_at: '2026-09-01T12:00:00Z',
				chunk_count: 14
			},
			nodes,
			edges
		};
	}

	function activeNeighbourGraph(): SessionEmbeddingGraph {
		const recentIso = new Date(Date.now() - 2 * 60 * 1000).toISOString();
		return {
			session_id: CENTRE,
			embedding: {
				present: true,
				source: 'overview',
				model: 'nomic-embed-text-v1.5',
				embedded_at: '2026-09-01T12:00:00Z',
				chunk_count: 5
			},
			nodes: [
				{
					session_id: CENTRE,
					title: 'Embedding graph on the session and project pages',
					project_id: 12,
					project_path: '/Users/vimes/projects/temet/temet-proxy',
					outcome: 'success',
					last_seen: '2026-09-01T13:05:38Z',
					last_status: 'Complete',
					similarity: 1,
					source: 'overview',
					topics: [],
					is_centre: true
				},
				{
					session_id: N1,
					title: 'Session vector backfill sweep',
					project_id: 12,
					project_path: '/Users/vimes/projects/temet/temet-proxy',
					outcome: 'success',
					last_seen: recentIso,
					last_status: 'Streaming',
					similarity: 0.86,
					source: 'centroid',
					topics: [],
					is_centre: false
				},
				{
					session_id: N2,
					title: 'Judge neighbour context and topics/learnings output',
					project_id: 12,
					project_path: '/Users/vimes/projects/temet/temet-proxy',
					outcome: 'partial',
					last_seen: '2026-08-28T15:40:00Z',
					last_status: 'Complete',
					similarity: 0.71,
					source: 'overview',
					topics: [],
					is_centre: false
				}
			],
			edges: [
				{ from: CENTRE, to: N1, similarity: 0.86 },
				{ from: CENTRE, to: N2, similarity: 0.71 },
				{ from: N1, to: N2, similarity: 0.61 }
			]
		};
	}

	function assertNoOverlappingCards(canvasElement: HTMLElement): void {
		const TOLERANCE = 2;
		const cards = Array.from(
			canvasElement.querySelectorAll('[data-slot="embedding-graph-node"]')
		) as HTMLElement[];
		const rects = cards.map((el) => el.getBoundingClientRect());
		const offenders: string[] = [];
		for (let i = 0; i < rects.length; i++) {
			for (let j = i + 1; j < rects.length; j++) {
				const a = rects[i]!;
				const b = rects[j]!;
				const overlapsX = a.left < b.right - TOLERANCE && b.left < a.right - TOLERANCE;
				const overlapsY = a.top < b.bottom - TOLERANCE && b.top < a.bottom - TOLERANCE;
				if (overlapsX && overlapsY) {
					offenders.push(
						`${cards[i]!.getAttribute('data-session-id')}<->${cards[j]!.getAttribute('data-session-id')}`
					);
				}
			}
		}
		expect(offenders).toEqual([]);
	}

	function assertNodesWithinContainer(canvasElement: HTMLElement): void {
		const TOLERANCE = 2;
		const container = canvasElement.querySelector('.eg-canvas') as HTMLElement;
		const containerRect = container.getBoundingClientRect();
		const cards = Array.from(
			canvasElement.querySelectorAll('[data-slot="embedding-graph-node"]')
		) as HTMLElement[];
		const offenders: string[] = [];
		for (const card of cards) {
			const rect = card.getBoundingClientRect();
			const fits =
				rect.left >= containerRect.left - TOLERANCE &&
				rect.right <= containerRect.right + TOLERANCE &&
				rect.top >= containerRect.top - TOLERANCE &&
				rect.bottom <= containerRect.bottom + TOLERANCE;
			if (!fits) offenders.push(card.getAttribute('data-session-id') ?? '(no id)');
		}
		expect(offenders).toEqual([]);
	}

	function buildStore(
		fetcher?: (id: string) => Promise<SessionEmbeddingGraph>
	): EmbeddingGraphStore {
		const store = new EmbeddingGraphStore(fetcher);
		store.load(baseGraph(), { width: 420, height: 240 });
		return store;
	}

	function singleNodeGraph(): SessionEmbeddingGraph {
		return {
			session_id: CENTRE,
			embedding: {
				present: true,
				source: 'centroid',
				model: 'nomic-embed-text-v1.5',
				embedded_at: '2026-09-01T12:00:00Z',
				chunk_count: 3
			},
			nodes: [
				{
					session_id: CENTRE,
					title: 'A session with no neighbours yet',
					project_id: 12,
					project_path: '/Users/vimes/projects/temet/temet-proxy',
					outcome: 'accepted',
					last_seen: '2026-09-01T13:05:38Z',
					last_status: 'Complete',
					similarity: 1,
					source: 'centroid',
					topics: [],
					is_centre: true
				}
			],
			edges: []
		};
	}

	function buildSingleNodeStore(): EmbeddingGraphStore {
		const store = new EmbeddingGraphStore();
		store.load(singleNodeGraph(), { width: 420, height: 240 });
		return store;
	}

	function buildManyNodesCompactStore(count: number): EmbeddingGraphStore {
		const store = new EmbeddingGraphStore();
		store.load(manyNodesGraph(count), { width: 420, height: 240 });
		return store;
	}

	const { Story } = defineMeta({
		title: 'Graph/EmbeddingGraph',
		component: EmbeddingGraph,
		tags: ['autodocs']
	});
</script>

<Story
	name="Compact"
	args={{
		store: buildStore((id: string) => Promise.resolve(neighbourGraphOf(id))),
		mode: 'compact',
		onOpenSession: fn(),
		expandHref: `/sessions/${CENTRE}/graph`
	}}
	play={async ({ canvasElement, args }) => {
		const canvas = within(canvasElement);
		const store = args.store as EmbeddingGraphStore;

		await waitFor(() =>
			expect(canvasElement.querySelectorAll('[data-slot="embedding-graph-node"]')).toHaveLength(4)
		);
		const centreEl = canvasElement.querySelector('.eg-node--centre') as HTMLElement;
		await expect(centreEl).toBeInTheDocument();

		const canvasEl = canvasElement.querySelector('.eg-canvas') as HTMLElement;
		expect(canvasEl.clientHeight).toBeGreaterThanOrEqual(150);

		await waitFor(() => assertNodesWithinContainer(canvasElement), { timeout: 3000 });

		const n1 = canvasElement.querySelector(`[data-session-id="${N1}"]`) as HTMLElement;
		await userEvent.click(n1);
		await waitFor(() => expect(store.selectedId).toBe(N1));
		const caption = canvasElement.querySelector(
			'[data-slot="embedding-graph-caption"]'
		) as HTMLElement;
		await waitFor(() =>
			expect(within(caption).getByText('Session vector backfill sweep')).toBeInTheDocument()
		);
		await expect(within(caption).getByRole('button', { name: 'Open' })).toBeInTheDocument();

		await userEvent.click(within(caption).getByRole('button', { name: 'Open' }));
		await expect(args.onOpenSession).toHaveBeenCalledWith(N1);

		const openPreviewBtn = canvas.getByRole('button', {
			name: 'Open preview of Session vector backfill sweep'
		});
		await userEvent.click(openPreviewBtn);
		await waitFor(() =>
			expect(document.body.querySelector('[data-slot="session-preview-modal"]')).toBeInTheDocument()
		);
		const previewModal = document.body.querySelector(
			'[data-slot="session-preview-modal"]'
		) as HTMLElement;
		await expect(
			within(previewModal).getByText('Session vector backfill sweep')
		).toBeInTheDocument();
		const previewClose = document.body.querySelector(
			'[data-slot="session-preview-close"]'
		) as HTMLElement;
		await userEvent.click(previewClose);
		await waitFor(
			() => expect(document.body.querySelector('[data-slot="session-preview-modal"]')).toBeNull(),
			{ timeout: 2000 }
		);

		const before = { x: store.nodes.get(N1)!.x, y: store.nodes.get(N1)!.y };
		const n3Before = { x: store.nodes.get(N3)!.x, y: store.nodes.get(N3)!.y };
		const rect = n1.getBoundingClientRect();
		const startX = rect.x + rect.width / 2;
		const startY = rect.y + rect.height / 2;
		const n3Rect = (
			canvasElement.querySelector(`[data-session-id="${N3}"]`) as HTMLElement
		).getBoundingClientRect();
		const targetX = n3Rect.x + n3Rect.width / 2;
		const targetY = n3Rect.y + n3Rect.height / 2;
		await userEvent.pointer([
			{ keys: '[MouseLeft>]', target: n1, coords: { x: startX, y: startY } },
			{ coords: { x: startX + (targetX - startX) / 3, y: startY + (targetY - startY) / 3 } },
			{
				coords: {
					x: startX + ((targetX - startX) * 2) / 3,
					y: startY + ((targetY - startY) * 2) / 3
				}
			},
			{ coords: { x: targetX, y: targetY } },
			{ keys: '[/MouseLeft]' }
		]);
		await waitFor(() => {
			const after = store.nodes.get(N1)!;
			expect(after.pinned).toBe(true);
			expect(after.x !== before.x || after.y !== before.y).toBe(true);
			const n3After = store.nodes.get(N3)!;
			expect(n3After.x !== n3Before.x || n3After.y !== n3Before.y).toBe(true);
			expect(n3After.pinned).toBe(false);
		});

		const n1PinnedPosition = { x: store.nodes.get(N1)!.x, y: store.nodes.get(N1)!.y };
		const n2Drag = canvasElement.querySelector(`[data-session-id="${N2}"]`) as HTMLElement;
		const n2Rect = n2Drag.getBoundingClientRect();
		const n2StartX = n2Rect.x + n2Rect.width / 2;
		const n2StartY = n2Rect.y + n2Rect.height / 2;
		const n1PinnedEl = canvasElement.querySelector(`[data-session-id="${N1}"]`) as HTMLElement;
		const n1Rect = n1PinnedEl.getBoundingClientRect();
		const n1TargetX = n1Rect.x + n1Rect.width / 2;
		const n1TargetY = n1Rect.y + n1Rect.height / 2;
		await userEvent.pointer([
			{ keys: '[MouseLeft>]', target: n2Drag, coords: { x: n2StartX, y: n2StartY } },
			{
				coords: {
					x: n2StartX + (n1TargetX - n2StartX) / 3,
					y: n2StartY + (n1TargetY - n2StartY) / 3
				}
			},
			{
				coords: {
					x: n2StartX + ((n1TargetX - n2StartX) * 2) / 3,
					y: n2StartY + ((n1TargetY - n2StartY) * 2) / 3
				}
			},
			{ coords: { x: n1TargetX, y: n1TargetY } },
			{ keys: '[/MouseLeft]' }
		]);
		await waitFor(() => {
			const n2After = store.nodes.get(N2)!;
			expect(n2After.pinned).toBe(true);
			const n1After = store.nodes.get(N1)!;
			expect(n1After.x).toBe(n1PinnedPosition.x);
			expect(n1After.y).toBe(n1PinnedPosition.y);
			expect(n1After.pinned).toBe(true);
		});

		const n2 = canvasElement.querySelector(`[data-session-id="${N2}"]`) as HTMLElement;
		await userEvent.dblClick(n2);
		await waitFor(() => expect(store.nodes.has(NEW_NEIGHBOUR)).toBe(true));
		await waitFor(() =>
			expect(canvasElement.querySelectorAll('[data-slot="embedding-graph-node"]')).toHaveLength(5)
		);
		await waitFor(() => assertNodesWithinContainer(canvasElement), { timeout: 3000 });
	}}
>
	{#snippet template(args)}
		<EmbeddingGraph {...args} />
	{/snippet}
</Story>

<Story
	name="FitOnLoad"
	args={{
		store: buildManyNodesCompactStore(24),
		mode: 'compact',
		onOpenSession: fn()
	}}
	play={async ({ canvasElement }) => {
		await waitFor(() =>
			expect(canvasElement.querySelectorAll('[data-slot="embedding-graph-node"]')).toHaveLength(24)
		);
		await waitFor(() => assertNodesWithinContainer(canvasElement), { timeout: 3000 });
	}}
>
	{#snippet template(args)}
		<EmbeddingGraph {...args} />
	{/snippet}
</Story>

<Story
	name="Full"
	args={{
		store: buildStore(),
		mode: 'full',
		onOpenSession: fn(),
		collapseHref: `/sessions/${CENTRE}`,
		fetchSession: stubFetchSession
	}}
	play={async ({ canvasElement, args }) => {
		const canvas = within(canvasElement);
		const store = args.store as EmbeddingGraphStore;

		await waitFor(() =>
			expect(canvasElement.querySelectorAll('[data-slot="embedding-graph-node"]')).toHaveLength(4)
		);

		await expect(canvas.getByText('Select a session to see its details.')).toBeInTheDocument();

		const n2 = canvasElement.querySelector(`[data-session-id="${N2}"]`) as HTMLElement;
		await userEvent.click(n2);
		const details = canvasElement.querySelector(
			'[data-slot="embedding-graph-details"]'
		) as HTMLElement;
		await waitFor(() =>
			expect(
				within(details).getByText('Judge neighbour context and topics/learnings output')
			).toBeInTheDocument()
		);
		await expect(within(details).getByText('71%')).toBeInTheDocument();
		await expect(within(details).getByRole('button', { name: 'Open session' })).toBeInTheDocument();
		await expect(
			within(details).getByRole('button', { name: 'Expand neighbours' })
		).toBeInTheDocument();

		await userEvent.click(within(details).getByRole('button', { name: 'Open session' }));
		await expect(args.onOpenSession).toHaveBeenCalledWith(N2);

		const legend = canvasElement.querySelector(
			'[data-slot="embedding-graph-legend"]'
		) as HTMLElement;
		await expect(within(legend).getByText(/Similarity \d+%–\d+%/)).toBeInTheDocument();

		const n3EdgeSelector = `[data-id="${[CENTRE, N3].sort().join('::')}"]`;
		await waitFor(() => expect(canvasElement.querySelector(n3EdgeSelector)).toBeInTheDocument());
		const slider = canvas.getByRole('slider', { name: 'Minimum similarity' });
		slider.focus();
		for (let i = 0; i < 3; i++) {
			await userEvent.keyboard('{ArrowRight}');
		}
		await waitFor(() => expect(store.similarityThreshold).toBeGreaterThanOrEqual(0.45));
		await waitFor(() => expect(canvasElement.querySelector(n3EdgeSelector)).toBeNull());
		expect(canvasElement.querySelector(`[data-session-id="${N3}"]`)).toBeInTheDocument();

		store.load(activeNeighbourGraph(), { width: 420, height: 240 });
		await waitFor(() =>
			expect(canvasElement.querySelectorAll('[data-slot="embedding-graph-node"]')).toHaveLength(3)
		);
		const n2Again = canvasElement.querySelector(`[data-session-id="${N2}"]`) as HTMLElement;
		await userEvent.click(n2Again);
		await waitFor(() => expect(store.selectedId).toBe(N2));

		const n1n2Edge = () =>
			canvasElement.querySelector(
				`[data-id="${[N1, N2].sort().join('::')}"] path.svelte-flow__edge-path`
			) as SVGPathElement | null;
		const centreN2Edge = () =>
			canvasElement.querySelector(
				`[data-id="${[CENTRE, N2].sort().join('::')}"] path.svelte-flow__edge-path`
			) as SVGPathElement | null;
		await waitFor(() => expect(n1n2Edge()?.style.stroke).toBe('var(--funk-magenta)'));
		expect(centreN2Edge()?.style.stroke).toBe('var(--funk-cyan)');

		const n1El = canvasElement.querySelector(`[data-session-id="${N1}"]`) as HTMLElement;
		const n2El = canvasElement.querySelector(`[data-session-id="${N2}"]`) as HTMLElement;
		expect(n1El.classList.contains('sfn--incident')).toBe(true);
		expect(n2El.classList.contains('sfn--incident')).toBe(false);

		expect(n2El.classList.contains('sfn--selected')).toBe(true);
		const n2Style = getComputedStyle(n2El);
		expect(n2Style.transitionProperty.split(',').map((p) => p.trim())).toContain('box-shadow');
		const boxShadowIndex = n2Style.transitionProperty
			.split(',')
			.findIndex((p) => p.trim() === 'box-shadow');
		const boxShadowDuration = n2Style.transitionDuration.split(',')[boxShadowIndex]?.trim();
		expect(boxShadowDuration).toBe('0.24s');

		const openPreviewBtn = canvas.getByRole('button', {
			name: 'Open preview of Judge neighbour context and topics/learnings output'
		});
		await userEvent.click(openPreviewBtn);
		await waitFor(() =>
			expect(document.body.querySelector('[data-slot="session-preview-modal"]')).toBeInTheDocument()
		);
		const previewModal = document.body.querySelector(
			'[data-slot="session-preview-modal"]'
		) as HTMLElement;
		const withinPreview = within(previewModal);
		await expect(
			withinPreview.getByText('Judge neighbour context and topics/learnings output')
		).toBeInTheDocument();
		await expect(withinPreview.getByText('Partial')).toBeInTheDocument();
		await expect(withinPreview.getByText('71% match')).toBeInTheDocument();
		await waitFor(() => expect(withinPreview.getByText('42')).toBeInTheDocument());
		await expect(withinPreview.getByText('96%')).toBeInTheDocument();
		const previewClose = document.body.querySelector(
			'[data-slot="session-preview-close"]'
		) as HTMLElement;
		await userEvent.click(previewClose);
		await waitFor(
			() => expect(document.body.querySelector('[data-slot="session-preview-modal"]')).toBeNull(),
			{ timeout: 2000 }
		);

		store.load(manyNodesGraph(12), { width: 960, height: 620 });
		await waitFor(() =>
			expect(canvasElement.querySelectorAll('[data-slot="embedding-graph-node"]')).toHaveLength(12)
		);
		await waitFor(() => assertNoOverlappingCards(canvasElement), { timeout: 3000 });

		await waitFor(() => assertNodesWithinContainer(canvasElement), { timeout: 3000 });

		const hoverNodeId = manyNodesId(0);
		const hoverEdgeSelector = `[data-id="${[CENTRE, hoverNodeId].sort().join('::')}"] path.svelte-flow__edge-path`;
		const readOpacity = (): number => {
			const path = canvasElement.querySelector(hoverEdgeSelector) as SVGPathElement | null;
			return path ? Number(path.style.opacity) : NaN;
		};
		await waitFor(() => expect(Number.isNaN(readOpacity())).toBe(false));
		const restingOpacity = readOpacity();

		const hoverNodeEl = canvasElement.querySelector(
			`[data-session-id="${hoverNodeId}"]`
		) as HTMLElement;
		await userEvent.hover(hoverNodeEl);
		await waitFor(() => expect(readOpacity()).toBeGreaterThan(restingOpacity));
		await userEvent.unhover(hoverNodeEl);
		await waitFor(() => expect(readOpacity()).toBeCloseTo(restingOpacity, 5));

		const relayoutBtn = canvas.getByRole('button', { name: 'Relayout' });
		const positionsOf = (): Map<string, { x: number; y: number }> =>
			new Map(Array.from(store.nodes.values(), (n) => [n.id, { x: n.x, y: n.y }]));

		await userEvent.click(relayoutBtn);
		await waitFor(() => assertNodesWithinContainer(canvasElement), { timeout: 3000 });
		const afterFirstPress = positionsOf();

		await userEvent.click(relayoutBtn);
		await waitFor(() => assertNodesWithinContainer(canvasElement), { timeout: 3000 });
		const afterSecondPress = positionsOf();

		const moved = Array.from(afterSecondPress.entries()).some(([id, pos]) => {
			if (id === store.centreId) return false;
			const before = afterFirstPress.get(id);
			if (!before) return false;
			return Math.hypot(pos.x - before.x, pos.y - before.y) > 5;
		});
		expect(moved).toBe(true);
	}}
>
	{#snippet template(args)}
		<div class="h-[620px] w-[960px] bg-[var(--surface-bg)] p-4">
			<EmbeddingGraph {...args} />
		</div>
	{/snippet}
</Story>

<Story
	name="ExpandedState"
	args={{
		store: buildStore((id: string) => Promise.resolve(neighbourGraphOf(id))),
		mode: 'full',
		onOpenSession: () => {}
	}}
	play={async ({ canvasElement, args }) => {
		const store = args.store as EmbeddingGraphStore;

		await waitFor(() =>
			expect(canvasElement.querySelectorAll('[data-slot="embedding-graph-node"]')).toHaveLength(4)
		);

		const n1 = canvasElement.querySelector(`[data-session-id="${N1}"]`) as HTMLElement;
		await userEvent.dblClick(n1);

		await waitFor(() => expect(store.nodes.has(NEW_NEIGHBOUR)).toBe(true));
		await waitFor(() =>
			expect(canvasElement.querySelectorAll('[data-slot="embedding-graph-node"]')).toHaveLength(5)
		);
		expect(store.nodes.get(NEW_NEIGHBOUR)?.origin).toBe(`expanded-from:${N1}`);
		const betweenN1AndCentre = Array.from(store.edges.values()).filter(
			(e) => (e.from === N1 && e.to === CENTRE) || (e.from === CENTRE && e.to === N1)
		);
		expect(betweenN1AndCentre).toHaveLength(1);

		await userEvent.click(n1);
		const collapseBtn = within(
			canvasElement.querySelector('[data-slot="embedding-graph-details"]') as HTMLElement
		).getByRole('button', { name: 'Collapse' });
		await userEvent.click(collapseBtn);
		await waitFor(() => expect(store.nodes.has(NEW_NEIGHBOUR)).toBe(false));
	}}
>
	{#snippet template(args)}
		<div class="h-[620px] w-[960px] bg-[var(--surface-bg)] p-4">
			<EmbeddingGraph {...args} />
		</div>
	{/snippet}
</Story>

<Story
	name="KeyboardAccess"
	args={{
		store: buildStore((id: string) => Promise.resolve(neighbourGraphOf(id))),
		mode: 'compact',
		onOpenSession: fn(),
		expandHref: `/sessions/${CENTRE}/graph`
	}}
	play={async ({ canvasElement, args }) => {
		const store = args.store as EmbeddingGraphStore;

		await waitFor(() =>
			expect(canvasElement.querySelectorAll('[data-slot="embedding-graph-node"]')).toHaveLength(4)
		);

		const n1 = canvasElement.querySelector(`[data-session-id="${N1}"]`) as HTMLElement;

		expect(n1.getAttribute('role')).toBe('button');
		expect(n1.tabIndex).toBe(0);

		n1.focus();
		await fireEvent.keyDown(n1, { key: 'Enter' });
		await waitFor(() => expect(store.selectedId).toBe(N1));
		const caption = canvasElement.querySelector(
			'[data-slot="embedding-graph-caption"]'
		) as HTMLElement;
		await waitFor(() =>
			expect(within(caption).getByText('Session vector backfill sweep')).toBeInTheDocument()
		);

		n1.focus();
		await fireEvent.keyDown(n1, { key: 'Enter', shiftKey: true });
		await waitFor(() => expect(store.nodes.has(NEW_NEIGHBOUR)).toBe(true));
		await waitFor(() =>
			expect(canvasElement.querySelectorAll('[data-slot="embedding-graph-node"]')).toHaveLength(5)
		);
		expect(store.nodes.get(NEW_NEIGHBOUR)?.origin).toBe(`expanded-from:${N1}`);
	}}
>
	{#snippet template(args)}
		<div class="w-[420px] bg-[var(--surface-bg)] p-3">
			<EmbeddingGraph {...args} />
		</div>
	{/snippet}
</Story>

<Story
	name="SingleNode"
	args={{
		store: buildSingleNodeStore(),
		mode: 'compact',
		onOpenSession: fn()
	}}
	play={async ({ canvasElement }) => {
		await waitFor(() =>
			expect(canvasElement.querySelectorAll('[data-slot="embedding-graph-node"]')).toHaveLength(1)
		);
		expect(canvasElement.querySelectorAll('.svelte-flow__edge')).toHaveLength(0);
		const centreEl = canvasElement.querySelector('.eg-node--centre') as HTMLElement;
		await expect(centreEl).toBeInTheDocument();
		await expect(
			within(centreEl).getByText('A session with no neighbours yet')
		).toBeInTheDocument();

		const caption = canvasElement.querySelector(
			'[data-slot="embedding-graph-caption"]'
		) as HTMLElement;
		await expect(within(caption).getByText('click a session to inspect it')).toBeInTheDocument();
	}}
>
	{#snippet template(args)}
		<div class="w-[420px] bg-[var(--surface-bg)] p-3">
			<EmbeddingGraph {...args} />
		</div>
	{/snippet}
</Story>

<Story
	name="RevealedFromHidden"
	args={{
		store: buildStore((id: string) => Promise.resolve(neighbourGraphOf(id))),
		mode: 'compact',
		onOpenSession: fn()
	}}
	play={async ({ canvasElement }) => {
		const wrapper = canvasElement.querySelector('[data-testid="eg-hidden-wrapper"]') as HTMLElement;
		await expect(wrapper).toBeInTheDocument();
		expect(wrapper.hidden).toBe(true);

		await waitFor(() =>
			expect(canvasElement.querySelectorAll('[data-slot="embedding-graph-node"]')).toHaveLength(4)
		);
		await new Promise((resolve) => setTimeout(resolve, 200));

		wrapper.hidden = false;

		for (const deltaMs of [250, 150, 300, 500]) {
			await new Promise((resolve) => setTimeout(resolve, deltaMs));
			assertNodesWithinContainer(canvasElement);
		}
	}}
>
	{#snippet template(args)}
		<div hidden data-testid="eg-hidden-wrapper">
			<EmbeddingGraph {...args} />
		</div>
	{/snippet}
</Story>
