<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { within, expect, userEvent, waitFor } from 'storybook/test';
	import SessionEmbeddingPanel from './SessionEmbeddingPanel.svelte';
	import CollapseStoreProvider from '$lib/components/__harness__/CollapseStoreProvider.svelte';
	import type { SessionEmbeddingGraph } from '$lib/api/embeddingGraph';
	import {
		stubFetchPending,
		stubFetchError,
		stubFetchJson
	} from '$lib/test-support/storyFetchStubs';
	import { __resetApiCacheForTests } from '$lib/stores/cache/apiCache.svelte';

	const SESSION_ID = 'a1111111-2222-4333-8444-555555555555';
	const NEIGHBOUR_1 = 'b2222222-3333-4444-8555-666666666666';
	const NEIGHBOUR_2 = 'c3333333-4444-4555-8666-777777777777';

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
			}
		],
		edges: [
			{ from: SESSION_ID, to: NEIGHBOUR_1, similarity: 0.83 },
			{ from: SESSION_ID, to: NEIGHBOUR_2, similarity: 0.64 }
		]
	};

	const NO_VECTOR: SessionEmbeddingGraph = {
		session_id: SESSION_ID,
		embedding: { present: false, reason: 'no_embedded_turns' },
		nodes: [
			{
				session_id: SESSION_ID,
				title: 'A brand-new session with no captured turns yet',
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
		title: 'Graph/SessionEmbeddingPanel',
		component: SessionEmbeddingPanel,
		tags: ['autodocs']
	});
</script>

<Story
	name="Loading"
	args={{ sessionId: SESSION_ID }}
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
		const toggle = await waitFor(() => canvas.getByRole('button', { expanded: false }));
		await userEvent.click(toggle);
		await waitFor(() => expect(canvasElement.querySelector('.sep-skeleton')).toBeInTheDocument());
	}}
>
	{#snippet template(args)}
		<CollapseStoreProvider>
			{#snippet children()}
				<div class="w-[480px] bg-[var(--surface-bg)]">
					<SessionEmbeddingPanel {...args} />
				</div>
			{/snippet}
		</CollapseStoreProvider>
	{/snippet}
</Story>

<Story
	name="Error"
	args={{ sessionId: SESSION_ID }}
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
		const toggle = await waitFor(() => canvas.getByRole('button', { expanded: false }));
		await userEvent.click(toggle);
		await waitFor(() =>
			expect(canvas.getByText("Couldn't load related sessions")).toBeInTheDocument()
		);
		await expect(canvas.getByText('inspect API unreachable')).toBeInTheDocument();
		await expect(canvas.getByRole('button', { name: /retry/i })).toBeInTheDocument();
	}}
>
	{#snippet template(args)}
		<CollapseStoreProvider>
			{#snippet children()}
				<div class="w-[480px] bg-[var(--surface-bg)]">
					<SessionEmbeddingPanel {...args} />
				</div>
			{/snippet}
		</CollapseStoreProvider>
	{/snippet}
</Story>

<Story
	name="NoVector"
	args={{ sessionId: SESSION_ID }}
	loaders={[
		async () => {
			localStorage.clear();
			__resetApiCacheForTests();
			stubFetchJson<SessionEmbeddingGraph>(NO_VECTOR);
			return {};
		}
	]}
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);
		const toggle = await waitFor(() => canvas.getByRole('button', { expanded: false }));
		await userEvent.click(toggle);
		const noVector = await canvas.findByText(/doesn't have any turns with captured text to embed/);
		await expect(noVector).toBeInTheDocument();
		expect(canvasElement.querySelector('[data-slot="embedding-graph"]')).toBeNull();
	}}
>
	{#snippet template(args)}
		<CollapseStoreProvider>
			{#snippet children()}
				<div class="w-[480px] bg-[var(--surface-bg)]">
					<SessionEmbeddingPanel {...args} />
				</div>
			{/snippet}
		</CollapseStoreProvider>
	{/snippet}
</Story>

<Story
	name="Populated"
	args={{ sessionId: SESSION_ID }}
	loaders={[
		async () => {
			localStorage.clear();
			__resetApiCacheForTests();
			stubFetchJson<SessionEmbeddingGraph>(POPULATED);
			return {};
		}
	]}
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);

		const toggle = await waitFor(() => canvas.getByRole('button', { expanded: false }));
		expect(canvasElement.querySelector('.dsc-body')).toBeNull();

		const helpIcon = canvasElement.querySelector('.dsc-title-help');
		await expect(helpIcon).toBeInTheDocument();
		await expect(toggle.querySelector('.dsc-title-help')).toBeNull();
		const expandLink = canvas.getByRole('link', { name: 'Open full graph for this session' });
		await expect(expandLink).toHaveAttribute('href', `/sessions/${SESSION_ID}/graph`);
		await expect(toggle.contains(expandLink)).toBe(false);

		await userEvent.click(toggle);
		await waitFor(() => expect(toggle).toHaveAttribute('aria-expanded', 'true'));

		await waitFor(() => expect(canvas.getByText('2 related sessions')).toBeInTheDocument());
		await waitFor(() =>
			expect(canvasElement.querySelectorAll('[data-slot="embedding-graph-node"]')).toHaveLength(3)
		);

		expect(canvasElement.querySelector('.eg-corner-btn')).toBeNull();

		const neighbour = canvasElement.querySelector(
			`[data-session-id="${NEIGHBOUR_1}"]`
		) as HTMLElement;
		await userEvent.click(neighbour);
		const caption = canvasElement.querySelector(
			'[data-slot="embedding-graph-caption"]'
		) as HTMLElement;
		await waitFor(() =>
			expect(within(caption).getByRole('button', { name: 'Open' })).toBeInTheDocument()
		);

		await userEvent.click(within(caption).getByRole('button', { name: 'Open' }));
	}}
>
	{#snippet template(args)}
		<CollapseStoreProvider>
			{#snippet children()}
				<div class="w-[480px] bg-[var(--surface-bg)]">
					<SessionEmbeddingPanel {...args} />
				</div>
			{/snippet}
		</CollapseStoreProvider>
	{/snippet}
</Story>
