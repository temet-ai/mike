<script lang="ts">
	import { Network } from '@lucide/svelte';
	import { getContext } from 'svelte';
	import { goto } from '$app/navigation';
	import { resolve } from '$app/paths';
	import { UnfoldHorizontal } from '@lucide/svelte';
	import SectionCard from '$lib/components/detail/SectionCard.svelte';
	import LoadErrorBanner from '$lib/components/shared/LoadErrorBanner.svelte';
	import EmbeddingGraph from '$lib/components/graph/EmbeddingGraph.svelte';
	import { EmbeddingGraphStore } from '$lib/graph/embeddingGraphStore.svelte';
	import { ApiError } from '$lib/api';
	import {
		getSessionEmbeddingGraph,
		type EmbeddingAbsentReason,
		type SessionEmbeddingGraph
	} from '$lib/api/embeddingGraph';
	import { COLLAPSE_STORE_KEY } from '$lib/context-keys';
	import type { CollapseStore } from '$lib/stores/collapseState.svelte';

	const COMPACT_VIEWPORT = { width: 420, height: 240 };

	interface Props {
		sessionId: string;
		grouped?: boolean;
	}

	let { sessionId, grouped = false }: Props = $props();

	const collapseStore = getContext<CollapseStore | undefined>(COLLAPSE_STORE_KEY);
	const sectionKey = $derived(`session:${sessionId}:embedding-graph-open`);
	let sectionExpandedLocal = $state(false);
	const sectionExpanded = $derived(
		collapseStore ? collapseStore.isExpanded(sectionKey) : sectionExpandedLocal
	);

	let data = $state<SessionEmbeddingGraph | null>(null);
	let loading = $state(false);
	let error = $state<string | null>(null);
	let loadedForSession = $state<string | null>(null);
	const store = new EmbeddingGraphStore();

	async function load(id: string): Promise<void> {
		loading = true;
		error = null;
		try {
			data = await getSessionEmbeddingGraph(id);
			store.load(data, COMPACT_VIEWPORT);
		} catch (err) {
			error = err instanceof ApiError ? err.message : String(err);
		} finally {
			loadedForSession = id;
			loading = false;
		}
	}

	$effect(() => {
		const id = sessionId;
		if (!sectionExpanded) return;
		if (loadedForSession === id || loading) return;
		void load(id);
	});

	function retry(): void {
		void load(sessionId);
	}

	function noVectorSentence(reason: EmbeddingAbsentReason): string {
		if (reason === 'embedding_disabled') {
			return 'This session has no embedding yet — the embedding backend is currently disabled.';
		}
		return "This session has no embedding yet — it doesn't have any turns with captured text to embed.";
	}

	const graphHref = $derived(
		resolve(`/sessions/${encodeURIComponent(sessionId)}/graph` as `/sessions/${string}/graph`)
	);

	const neighbourCount = $derived(data ? data.nodes.filter((n) => !n.is_centre).length : 0);

	function goToSession(id: string): void {
		void goto(resolve(`/sessions/${encodeURIComponent(id)}` as `/sessions/${string}`));
	}
</script>

<SectionCard
	title="Related sessions"
	titleIcon={Network}
	collapsible
	bordered={!grouped}
	{sectionKey}
	defaultCollapsed
	titleTooltip="The sessions whose embedding is nearest to this one's — the judge's overview vector once this session has been judged, otherwise the mean of its turn chunks."
>
	{#snippet headerActions()}
		<a
			href={graphHref}
			class="sep-expand-link"
			aria-label="Open full graph for this session"
			title="Open full graph"
		>
			<UnfoldHorizontal size={13} aria-hidden="true" />
		</a>
	{/snippet}
	{#snippet children()}
		<div class="sep-body">
			{#if loading && !data}
				<div class="sep-skeleton" aria-hidden="true">
					<div class="sep-skel-row"></div>
					<div class="sep-skel-row sep-skel-row--short"></div>
				</div>
			{:else if error}
				<LoadErrorBanner
					title="Couldn't load related sessions"
					detail={error}
					hint="The embedding-graph endpoint may not be reachable yet."
					onRetry={retry}
				/>
			{:else if data && !data.embedding.present}
				<p class="sep-no-vector" data-slot="session-embedding-no-vector">
					{noVectorSentence(data.embedding.reason)}
				</p>
			{:else if data}
				<p class="sep-caption">{neighbourCount} related session{neighbourCount === 1 ? '' : 's'}</p>
				<EmbeddingGraph {store} mode="compact" onOpenSession={goToSession} />
			{/if}
		</div>
	{/snippet}
</SectionCard>

<style>
	.sep-expand-link {
		display: inline-flex;
		flex-shrink: 0;
		align-self: center;
		align-items: center;
		justify-content: center;
		width: 24px;
		height: 24px;
		border-radius: 2px;
		color: var(--text-3);
		transition:
			color var(--dur-1, 120ms) ease,
			background-color var(--dur-1, 120ms) ease;
	}
	.sep-expand-link:hover {
		background: var(--surface-2);
		color: var(--text-strong);
	}
	.sep-expand-link:focus-visible {
		outline: none;
		box-shadow: 0 0 0 2px var(--ring);
	}

	.sep-no-vector {
		margin: 0;
		font-size: var(--text-sm);
		color: var(--text-2);
	}

	.sep-caption {
		margin: var(--space-2, 8px) 0 0;
		font-size: var(--text-caption, 11px);
		color: var(--text-3);
		font-family: var(--font-mono, monospace);
	}

	.sep-skeleton {
		display: flex;
		flex-direction: column;
		gap: var(--space-2);
	}
	.sep-skel-row {
		height: 14px;
		border-radius: var(--r-tol);
		background: var(--surface-2);
	}
	.sep-skel-row--short {
		width: 60%;
	}
</style>
