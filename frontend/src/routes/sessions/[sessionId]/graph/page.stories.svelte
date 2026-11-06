<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { within, expect, userEvent, waitFor } from 'storybook/test';
	import GraphPage from './+page.svelte';
	import type { SessionEmbeddingGraph } from '$lib/api/embeddingGraph';
	import { stubFetchJson } from '$lib/test-support/storyFetchStubs';
	import { __resetApiCacheForTests } from '$lib/stores/cache/apiCache.svelte';

	const SESSION_ID = 'e1111111-2222-4333-8444-555555555555';
	const NEIGHBOUR_1 = 'e2222222-3333-4444-8555-666666666666';
	const NEIGHBOUR_2 = 'e3333333-4444-4555-8666-777777777777';
	const NEIGHBOUR_3 = 'e4444444-5555-4666-8777-888888888888';

	const POPULATED: SessionEmbeddingGraph = {
		session_id: SESSION_ID,
		embedding: {
			present: true,
			source: 'overview',
			model: 'nomic-embed-text-v1.5',
			embedded_at: '2026-09-01T12:00:00Z',
			chunk_count: 14
		},
		nodes: [
			{
				session_id: SESSION_ID,
				title: 'Session and project embedding graphs',
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
				session_id: NEIGHBOUR_1,
				title: 'Embedding worker sweep and judge neighbour context',
				project_id: 12,
				project_path: '/Users/vimes/projects/temet/temet-proxy',
				outcome: 'shipped',
				last_seen: '2026-08-30T09:12:00Z',
				last_status: 'Complete',
				similarity: 0.83,
				source: 'centroid',
				topics: ['embeddings'],
				is_centre: false
			},
			{
				session_id: NEIGHBOUR_2,
				title: 'Session judge overview and knowledge rows',
				project_id: 12,
				project_path: '/Users/vimes/projects/temet/temet-proxy',
				outcome: 'corrected',
				last_seen: '2026-08-28T15:40:00Z',
				last_status: 'Complete',
				similarity: 0.64,
				source: 'overview',
				topics: ['judge'],
				is_centre: false
			},
			{
				session_id: NEIGHBOUR_3,
				title: 'Lesson relations evidence backfill',
				project_id: 12,
				project_path: '/Users/vimes/projects/temet/temet-proxy',
				outcome: null,
				last_seen: '2026-08-20T10:00:00Z',
				last_status: 'Complete',
				similarity: 0.51,
				source: 'centroid',
				topics: [],
				is_centre: false
			}
		],
		edges: [
			{ from: SESSION_ID, to: NEIGHBOUR_1, similarity: 0.83 },
			{ from: SESSION_ID, to: NEIGHBOUR_2, similarity: 0.64 },
			{ from: SESSION_ID, to: NEIGHBOUR_3, similarity: 0.51 },
			{ from: NEIGHBOUR_1, to: NEIGHBOUR_2, similarity: 0.6 }
		]
	};

	const NO_VECTOR: SessionEmbeddingGraph = {
		session_id: SESSION_ID,
		embedding: { present: false, reason: 'embedding_disabled' },
		nodes: [
			{
				session_id: SESSION_ID,
				title: 'A session captured while the embedding backend was off',
				project_id: 12,
				project_path: '/Users/vimes/projects/temet/temet-proxy',
				outcome: null,
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

	const { Story } = defineMeta({
		title: 'Routes/Sessions/Graph',
		component: GraphPage,
		tags: ['autodocs']
	});
</script>

<Story
	name="Populated"
	loaders={[
		async () => {
			__resetApiCacheForTests();
			stubFetchJson<SessionEmbeddingGraph>(POPULATED);
			return {};
		}
	]}
	parameters={{
		sveltekit_experimental: {
			state: {
				page: {
					url: new URL(`http://localhost/sessions/${SESSION_ID}/graph`),
					params: { sessionId: SESSION_ID }
				}
			}
		}
	}}
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);

		const backLink = canvas.getByRole('link', { name: 'Back to session' });
		await expect(backLink).toHaveAttribute('href', `/sessions/${SESSION_ID}`);

		await waitFor(() =>
			expect(canvasElement.querySelector('.seg-subtitle')?.textContent).toContain(
				'3 related sessions'
			)
		);
		await waitFor(() =>
			expect(canvasElement.querySelectorAll('[data-slot="embedding-graph-node"]')).toHaveLength(4)
		);

		const legend = canvasElement.querySelector(
			'[data-slot="embedding-graph-legend"]'
		) as HTMLElement;
		await expect(within(legend).getByText(/Similarity \d+%–\d+%/)).toBeInTheDocument();
		const labels = Array.from(canvasElement.querySelectorAll('.sfn-title')).map(
			(el) => el.textContent
		);
		expect(labels).toHaveLength(4);

		const neighbour = canvasElement.querySelector(
			`[data-session-id="${NEIGHBOUR_1}"]`
		) as HTMLElement;
		await userEvent.click(neighbour);
		const details = canvasElement.querySelector(
			'[data-slot="embedding-graph-details"]'
		) as HTMLElement;
		await waitFor(() =>
			expect(within(details).getByRole('button', { name: 'Open session' })).toBeInTheDocument()
		);

		await userEvent.click(within(details).getByRole('button', { name: 'Open session' }));
	}}
>
	{#snippet template()}
		<div class="h-[700px] w-[1100px] bg-[var(--surface-bg)]">
			<GraphPage />
		</div>
	{/snippet}
</Story>

<Story
	name="NoVector"
	loaders={[
		async () => {
			__resetApiCacheForTests();
			stubFetchJson<SessionEmbeddingGraph>(NO_VECTOR);
			return {};
		}
	]}
	parameters={{
		sveltekit_experimental: {
			state: {
				page: {
					url: new URL(`http://localhost/sessions/${SESSION_ID}/graph`),
					params: { sessionId: SESSION_ID }
				}
			}
		}
	}}
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);
		const noVector = await canvas.findByText(/embedding backend is currently disabled/);
		await expect(noVector).toBeInTheDocument();
		expect(canvasElement.querySelector('[data-slot="embedding-graph"]')).toBeNull();
	}}
>
	{#snippet template()}
		<div class="h-[700px] w-[1100px] bg-[var(--surface-bg)]">
			<GraphPage />
		</div>
	{/snippet}
</Story>
