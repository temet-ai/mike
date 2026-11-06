<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { within, expect, userEvent, waitFor } from 'storybook/test';
	import ProjectEmbeddingPanel from './ProjectEmbeddingPanel.svelte';
	import type { FleetProjectSummary } from '$lib/fleet/types';
	import type { ProjectEmbeddingGraph } from '$lib/api/embeddingGraph';
	import {
		stubFetchPending,
		stubFetchError,
		stubFetchJson
	} from '$lib/test-support/storyFetchStubs';
	import { __resetApiCacheForTests } from '$lib/stores/cache/apiCache.svelte';
	import CollapseStoreProvider from '$lib/components/__harness__/CollapseStoreProvider.svelte';

	const PROJECT_PATH = '/Users/vimes/projects/temet/temet-proxy';
	const PROJECT_ID = 12;
	const S1 = 'd1111111-2222-4333-8444-555555555555';
	const S2 = 'd2222222-3333-4444-8555-666666666666';

	function project(overrides: Partial<FleetProjectSummary> = {}): FleetProjectSummary {
		return {
			project_path: PROJECT_PATH,
			session_count: 143,
			request_count: 5820,
			first_seen: '2026-01-10T09:00:00Z',
			last_seen: '2026-09-01T13:05:00Z',
			pinned: false,
			custom_name: null,
			project_id: PROJECT_ID,
			...overrides
		};
	}

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
			}
		],
		edges: [{ from: S1, to: S2, similarity: 0.58 }],
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

	const { Story } = defineMeta({
		title: 'Graph/ProjectEmbeddingPanel',
		component: ProjectEmbeddingPanel,
		tags: ['autodocs']
	});
</script>

<Story
	name="Loading"
	args={{ project: project() }}
	loaders={[
		async () => {
			localStorage.clear();
			__resetApiCacheForTests();
			stubFetchPending();
			return {};
		}
	]}
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);
		const toggle = canvas.getByRole('button', { expanded: false });
		await userEvent.click(toggle);
		await waitFor(() => expect(canvasElement.querySelector('.pep-skeleton')).toBeInTheDocument());
	}}
>
	{#snippet template(args)}
		<div class="w-[520px] bg-[var(--surface-bg)]">
			<CollapseStoreProvider>
				<ProjectEmbeddingPanel {...args} />
			</CollapseStoreProvider>
		</div>
	{/snippet}
</Story>

<Story
	name="Error"
	args={{ project: project() }}
	loaders={[
		async () => {
			localStorage.clear();
			__resetApiCacheForTests();
			stubFetchError('inspect API unreachable', 503);
			return {};
		}
	]}
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);
		const toggle = canvas.getByRole('button', { expanded: false });
		await userEvent.click(toggle);
		await waitFor(() =>
			expect(canvas.getByText("Couldn't load the session map")).toBeInTheDocument()
		);
		await expect(canvas.getByText('inspect API unreachable')).toBeInTheDocument();
		await expect(canvas.getByRole('button', { name: /retry/i })).toBeInTheDocument();
	}}
>
	{#snippet template(args)}
		<div class="w-[520px] bg-[var(--surface-bg)]">
			<CollapseStoreProvider>
				<ProjectEmbeddingPanel {...args} />
			</CollapseStoreProvider>
		</div>
	{/snippet}
</Story>

<Story
	name="EmptyProject"
	args={{ project: project() }}
	loaders={[
		async () => {
			localStorage.clear();
			__resetApiCacheForTests();
			stubFetchJson<ProjectEmbeddingGraph>(EMPTY);
			return {};
		}
	]}
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);
		const toggle = canvas.getByRole('button', { expanded: false });
		await userEvent.click(toggle);
		const empty = await canvas.findByText('No embedded sessions for this project yet.');
		await expect(empty).toBeInTheDocument();
		expect(canvasElement.querySelector('[data-slot="embedding-graph"]')).toBeNull();
	}}
>
	{#snippet template(args)}
		<div class="w-[520px] bg-[var(--surface-bg)]">
			<CollapseStoreProvider>
				<ProjectEmbeddingPanel {...args} />
			</CollapseStoreProvider>
		</div>
	{/snippet}
</Story>

<Story
	name="PopulatedWithSiblings"
	args={{ project: project() }}
	loaders={[
		async () => {
			localStorage.clear();
			__resetApiCacheForTests();
			stubFetchJson<ProjectEmbeddingGraph>(POPULATED);
			return {};
		}
	]}
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);

		const toggle = canvas.getByRole('button', { expanded: false });
		expect(canvasElement.querySelector('#pep-body')).toBeNull();

		const expandLink = canvas.getByRole('link', { name: 'Open full session map for this project' });
		await expect(expandLink).toHaveAttribute(
			'href',
			`/projects/${encodeURIComponent(PROJECT_PATH)}/graph`
		);

		await userEvent.click(toggle);
		await waitFor(() => expect(toggle).toHaveAttribute('aria-expanded', 'true'));

		await waitFor(() => expect(canvas.getByText('2 of 143 sessions plotted')).toBeInTheDocument());
		await waitFor(() =>
			expect(canvasElement.querySelectorAll('[data-slot="embedding-graph-node"]')).toHaveLength(2)
		);

		expect(canvasElement.querySelector('.eg-node--centre')).toBeNull();

		await expect(canvas.getByText('Similar projects')).toBeInTheDocument();
		const siblingBtn = canvas.getByRole('button', {
			name: /temet-aide/
		});
		await expect(within(siblingBtn).getByText('64%')).toBeInTheDocument();
		await userEvent.click(siblingBtn);
	}}
>
	{#snippet template(args)}
		<div class="w-[520px] bg-[var(--surface-bg)]">
			<CollapseStoreProvider>
				<ProjectEmbeddingPanel {...args} />
			</CollapseStoreProvider>
		</div>
	{/snippet}
</Story>
