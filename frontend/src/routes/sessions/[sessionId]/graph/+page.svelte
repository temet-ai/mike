<script lang="ts">
	import { untrack } from 'svelte';
	import { page } from '$app/state';
	import { resolve } from '$app/paths';
	import { goto } from '$app/navigation';
	import { FoldHorizontal } from '@lucide/svelte';
	import { shortSessionId } from '$lib/format';
	import LoadErrorBanner from '$lib/components/shared/LoadErrorBanner.svelte';
	import EmbeddingGraph from '$lib/components/graph/EmbeddingGraph.svelte';
	import { EmbeddingGraphStore } from '$lib/graph/embeddingGraphStore.svelte';
	import { ApiError } from '$lib/api';
	import {
		getSessionEmbeddingGraph,
		type EmbeddingAbsentReason,
		type SessionEmbeddingGraph
	} from '$lib/api/embeddingGraph';

	const FULL_VIEWPORT = { width: 960, height: 560 };

	const sessionId = $derived(page.params.sessionId ?? '');
	const backHref = $derived(
		resolve(`sessions/${encodeURIComponent(sessionId)}` as `sessions/${string}`)
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
			data = await getSessionEmbeddingGraph(id, { k: 24 });
			store.load(data, FULL_VIEWPORT);
			store.similarityThreshold = 0.6;
		} catch (err) {
			error = err instanceof ApiError ? err.message : String(err);
		} finally {
			loadedForSession = id;
			loading = false;
		}
	}

	$effect(() => {
		const id = sessionId;
		untrack(() => {
			if (!id) return;
			if (loadedForSession === id || loading) return;
			void load(id);
		});
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

	const neighbourCount = $derived(data ? data.nodes.filter((n) => !n.is_centre).length : 0);

	function goToSession(id: string): void {
		void goto(resolve(`sessions/${encodeURIComponent(id)}` as `sessions/${string}`));
	}
</script>

<svelte:head>
	<title>Related sessions — {shortSessionId(sessionId)} — mike</title>
</svelte:head>

<section class="seg-page" aria-label="Session embedding graph">
	<header class="seg-header">
		<span class="seg-back-backing">
			<a
				href={backHref}
				class="seg-back"
				data-augmented-ui="tr-clip bl-clip border"
				aria-label="Back to session"
			>
				<FoldHorizontal size={14} aria-hidden="true" />
			</a>
		</span>
		<div class="seg-header__titles">
			<h2 class="seg-title">Related sessions</h2>
			<span class="seg-subtitle">
				{shortSessionId(sessionId)}
				{#if data && data.embedding.present}
					· {neighbourCount} related session{neighbourCount === 1 ? '' : 's'}
				{/if}
			</span>
		</div>
	</header>

	<div class="seg-body">
		{#if loading && !data}
			<div class="seg-skeleton" aria-hidden="true">
				<div class="seg-skel-row"></div>
				<div class="seg-skel-row seg-skel-row--short"></div>
			</div>
		{:else if error}
			<LoadErrorBanner
				title="Couldn't load the session graph"
				detail={error}
				hint="The embedding-graph endpoint may not be reachable yet."
				onRetry={retry}
				backHref="/sessions/{sessionId}"
				backLabel="Back to session"
			/>
		{:else if data && !data.embedding.present}
			<p class="seg-no-vector" data-slot="session-embedding-no-vector">
				{noVectorSentence(data.embedding.reason)}
			</p>
		{:else if data}
			<EmbeddingGraph {store} mode="full" onOpenSession={goToSession} />
		{/if}
	</div>
</section>

<style>
	.seg-page {
		display: flex;
		flex-direction: column;
		min-height: calc(100vh - 44px);
	}

	.seg-header {
		display: flex;
		align-items: center;
		gap: 12px;
		padding: 12px 24px;
		flex-shrink: 0;
	}
	.seg-back-backing {
		display: inline-flex;
		flex-shrink: 0;
		background: var(--surface-bg);
	}
	.seg-back {
		--edge: 6px;
		--aug-tr: var(--edge);
		--aug-bl: var(--edge);
		--aug-border-all: 1px;
		--aug-border-bg: var(--border-subtle);
		display: flex;
		align-items: center;
		justify-content: center;
		width: 28px;
		height: 28px;
		color: var(--text-2);
		text-decoration: none;
		transition: color var(--dur-1, 120ms) ease;
	}
	.seg-back:hover {
		--aug-border-bg: var(--border-default);
		color: var(--text-strong);
	}
	.seg-back:focus-visible {
		outline: 2px solid var(--focus);
		outline-offset: 2px;
	}
	.seg-header__titles {
		display: flex;
		flex-direction: column;
		gap: 2px;
		min-width: 0;
	}
	.seg-title {
		margin: 0;
		font-family: var(--font-display);
		font-size: 1.05rem;
		font-weight: 700;
		letter-spacing: -0.01em;
		color: var(--text-1);
	}
	.seg-subtitle {
		font-family: var(--font-mono, monospace);
		font-size: 11px;
		color: var(--text-3);
	}

	.seg-body {
		flex: 1 1 auto;
		min-height: 0;
		display: flex;
		flex-direction: column;
		padding: 0 24px 24px;
		container-type: inline-size;
		container-name: embedding-graph-full;
	}

	.seg-no-vector {
		margin: 24px;
		font-size: var(--text-sm, 13px);
		color: var(--text-2);
	}

	.seg-skeleton {
		display: flex;
		flex-direction: column;
		gap: 8px;
		padding: 24px;
	}
	.seg-skel-row {
		height: 14px;
		border-radius: 2px;
		background: var(--surface-2);
	}
	.seg-skel-row--short {
		width: 40%;
	}
</style>
