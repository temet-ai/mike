<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { within, expect, waitFor } from 'storybook/test';
	import GraphPage from './+page.svelte';
	import type { FleetProjectSummary } from '$lib/fleet/types';
	import type { ProjectEmbeddingGraph } from '$lib/api/embeddingGraph';
	import { stubFetchJsonRouter, typed } from '$lib/test-support/storyFetchStubs';
	import { __resetApiCacheForTests } from '$lib/stores/cache/apiCache.svelte';

	const PROJECT_PATH = '/Users/vimes/projects/temet/temet-proxy';
	const PROJECT_ID = 12;
	const S1 = 'f1111111-2222-4333-8444-555555555555';
	const S2 = 'f2222222-3333-4444-8555-666666666666';
	const S3 = 'f3333333-4444-4555-8666-777777777777';

	const PROJECTS: FleetProjectSummary[] = [
		{
			project_path: PROJECT_PATH,
			session_count: 143,
			request_count: 5820,
			first_seen: '2026-01-10T09:00:00Z',
			last_seen: '2026-09-01T13:05:00Z',
			pinned: false,
			custom_name: null,
			project_id: PROJECT_ID
		}
	];

	const POPULATED: ProjectEmbeddingGraph = {
		project_id: PROJECT_ID,
		project_path: PROJECT_PATH,
		centroid: { present: true, session_count: 143, embedded_at: '2026-09-01T12:00:00Z' },
		nodes: [
			{
				session_id: S1,
				title: 'Embedding worker sweep and judge neighbour context',
				outcome: 'shipped',
				last_seen: '2026-08-30T09:12:00Z',
				last_status: 'Complete',
				similarity_to_centroid: 0.81,
				source: 'centroid',
				topics: ['embeddings']
			},
			{
				session_id: S2,
				title: 'Session judge overview and knowledge rows',
				outcome: 'corrected',
				last_seen: '2026-08-28T15:40:00Z',
				last_status: 'Complete',
				similarity_to_centroid: 0.66,
				source: 'overview',
				topics: ['judge']
			},
			{
				session_id: S3,
				title: 'Lesson relations evidence backfill',
				outcome: null,
				last_seen: '2026-08-20T10:00:00Z',
				last_status: 'Complete',
				similarity_to_centroid: 0.48,
				source: 'centroid',
				topics: []
			}
		],
		edges: [
			{ from: S1, to: S2, similarity: 0.58 },
			{ from: S2, to: S3, similarity: 0.4 }
		],
		sibling_projects: [
			{
				project_id: 7,
				project_path: '/Users/vimes/projects/temet/temet-aide',
				similarity: 0.64
			}
		]
	};

	const EMPTY: ProjectEmbeddingGraph = {
		project_id: PROJECT_ID,
		project_path: PROJECT_PATH,
		centroid: { present: false, session_count: 0, embedded_at: null },
		nodes: [],
		edges: [],
		sibling_projects: []
	};

	function stubProjectRoutes(embeddingGraph: ProjectEmbeddingGraph): void {
		stubFetchJsonRouter((_method, url) => {
			if (url.includes('/embedding-graph')) return typed<ProjectEmbeddingGraph>(embeddingGraph);
			if (url.endsWith('/api/projects')) return typed<FleetProjectSummary[]>(PROJECTS);
			return typed<Record<string, never>>({});
		});
	}

	const { Story } = defineMeta({
		title: 'Routes/Projects/Graph',
		component: GraphPage,
		tags: ['autodocs']
	});
</script>

<Story
	name="Populated"
	loaders={[
		async () => {
			__resetApiCacheForTests();
			stubProjectRoutes(POPULATED);
			return {};
		}
	]}
	parameters={{
		sveltekit_experimental: {
			state: {
				page: {
					url: new URL(`http://localhost/projects/${encodeURIComponent(PROJECT_PATH)}/graph`),
					params: { projectPath: PROJECT_PATH }
				}
			}
		}
	}}
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);

		const backLink = canvas.getByRole('link', { name: 'Back to project' });
		await expect(backLink).toHaveAttribute('href', `/projects/${encodeURIComponent(PROJECT_PATH)}`);

		await waitFor(() =>
			expect(canvasElement.querySelector('.peg-subtitle')?.textContent).toContain(
				'3 of 143 sessions plotted'
			)
		);
		await waitFor(() =>
			expect(canvasElement.querySelectorAll('[data-slot="embedding-graph-node"]')).toHaveLength(3)
		);

		expect(canvasElement.querySelector('.eg-node--centre')).toBeNull();

		await expect(canvas.getByText('Similar projects')).toBeInTheDocument();
		await expect(canvas.getByText(/temet-aide/)).toBeInTheDocument();
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
			stubProjectRoutes(EMPTY);
			return {};
		}
	]}
	parameters={{
		sveltekit_experimental: {
			state: {
				page: {
					url: new URL(`http://localhost/projects/${encodeURIComponent(PROJECT_PATH)}/graph`),
					params: { projectPath: PROJECT_PATH }
				}
			}
		}
	}}
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);
		const empty = await canvas.findByText('No embedded sessions for this project yet.');
		await expect(empty).toBeInTheDocument();
		expect(canvasElement.querySelector('[data-slot="embedding-graph"]')).toBeNull();
	}}
>
	{#snippet template()}
		<div class="h-[700px] w-[1100px] bg-[var(--surface-bg)]">
			<GraphPage />
		</div>
	{/snippet}
</Story>
