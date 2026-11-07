<script lang="ts">
	import { BookOpen, Search, X } from '@lucide/svelte';
	import SectionCard from '$lib/components/detail/SectionCard.svelte';
	import { Button } from '$lib/components/ui/button';
	import LoadErrorBanner from '$lib/components/shared/LoadErrorBanner.svelte';
	import { ApiError, readProjectKnowledge, searchProjectKnowledge } from '$lib/api';
	import type { KnowledgeReadResponse, KnowledgeSearchItem, KnowledgeSearchResponse } from '$lib/types';

	interface Props {
		projectPath: string;
	}

	const { projectPath }: Props = $props();
	const sectionKey = $derived(`project:${projectPath}:knowledge-open`);
	const PAGE_SIZE = 8;
	let query = $state('');
	let data = $state<KnowledgeSearchResponse | null>(null);
	let selected = $state<KnowledgeReadResponse | null>(null);
	let loading = $state(false);
	let reading = $state(false);
	let error = $state<string | null>(null);
	let loadedFor = $state<string | null>(null);
	let loadingMore = $state(false);
	let moreError = $state<string | null>(null);
	// The query the shown list answers; Load more continues it even if the box was edited since.
	let activeQuery = '';
	// Bumped by every new search so a slow page of an older search cannot append to a newer list.
	let generation = 0;

	async function search(): Promise<void> {
		const mine = ++generation;
		loading = true;
		loadingMore = false;
		error = null;
		moreError = null;
		activeQuery = query;
		try {
			const answer = await searchProjectKnowledge(projectPath, { query: activeQuery, limit: PAGE_SIZE });
			if (mine !== generation) return;
			data = answer;
			loadedFor = projectPath;
		} catch (err) {
			if (mine !== generation) return;
			data = null;
			error = err instanceof ApiError ? err.message : String(err);
		} finally {
			if (mine === generation) loading = false;
		}
	}

	async function loadMore(): Promise<void> {
		if (!data || !data.has_more || data.next_offset === null || loadingMore) return;
		const mine = generation;
		loadingMore = true;
		moreError = null;
		try {
			const page = await searchProjectKnowledge(projectPath, {
				query: activeQuery,
				limit: PAGE_SIZE,
				offset: data.next_offset
			});
			if (mine !== generation || !data) return;
			const seen = new Set(data.items.map((item) => item.reference));
			data = {
				...data,
				items: [...data.items, ...page.items.filter((item) => !seen.has(item.reference))],
				has_more: page.has_more,
				next_offset: page.next_offset
			};
		} catch (err) {
			if (mine === generation) moreError = err instanceof ApiError ? err.message : String(err);
		} finally {
			if (mine === generation) loadingMore = false;
		}
	}

	async function open(item: KnowledgeSearchItem): Promise<void> {
		reading = true;
		error = null;
		try {
			selected = await readProjectKnowledge(projectPath, item.reference);
		} catch (err) {
			error = err instanceof ApiError ? err.message : String(err);
		} finally {
			reading = false;
		}
	}

	$effect(() => {
		if (loadedFor !== projectPath && !loading) void search();
	});
</script>

<div data-slot="project-knowledge-panel">
	<SectionCard
		title="Project knowledge"
		titleIcon={BookOpen}
		collapsible
		bordered={false}
		sectionKey={sectionKey}
		defaultCollapsed={false}
		titleTooltip="Search the project's retained artifacts, lessons, notebook entries and captured turns."
	>
		{#snippet headerActions()}
			{#if data?.embedding}
				<span class="pk-status" title="Embedding coverage for this project">
					{data.embedding.coverage?.ready ?? 0}/{data.embedding.coverage?.total ?? 0} semantic
				</span>
			{/if}
		{/snippet}

		<div class="pk-body">
			<form class="pk-search" onsubmit={(event) => { event.preventDefault(); void search(); }}>
			<label class="sr-only" for="project-knowledge-query">Search project knowledge</label>
			<input id="project-knowledge-query" bind:value={query} placeholder="Find decisions, designs, lessons…" />
			<button type="submit" aria-label="Search project knowledge" disabled={loading}>
				<Search size={14} aria-hidden="true" />
			</button>
			</form>

			{#if error}
				<LoadErrorBanner title="Couldn't load project knowledge" detail={error} onRetry={() => void search()} />
			{:else if loading && !data}
				<p class="pk-muted">Searching retained project sources…</p>
			{:else if data}
				<div class="pk-meta">
					<span>
						{data.items.length}
						{data.has_more ? 'results so far' : data.items.length === 1 ? 'result' : 'results'}
					</span>
					<span>{data.retrieval_path === 'hybrid' ? 'text + semantic' : 'text search'}</span>
					{#if data.embedding?.fallback}<span>semantic fallback: {data.embedding.fallback}</span>{/if}
				</div>
				{#if data.items.length === 0}
					<p class="pk-muted">No current project knowledge matches this query.</p>
				{:else}
					<ul class="pk-results" role="list">
						{#each data.items as item (item.reference)}
							<li>
								<button class="pk-result" type="button" onclick={() => void open(item)} aria-label={`Read ${item.title}`}>
									<span class="pk-result__kind">{item.source_kind}</span>
									<span class="pk-result__title">{item.title || item.reference}</span>
									<span class="pk-result__path">{item.path ?? item.reference}</span>
								</button>
							</li>
						{/each}
					</ul>
					{#if moreError}
						<p class="pk-more-error" role="alert">Couldn't load more results: {moreError}</p>
					{/if}
					{#if data.has_more}
						<div class="pk-more">
							<Button variant="outline" size="sm" disabled={loadingMore} onclick={() => void loadMore()}>
								{loadingMore ? 'Loading' : 'Load more'}
							</Button>
						</div>
					{/if}
				{/if}
			{/if}
		</div>
	</SectionCard>
</div>

{#if selected}
	<div class="pk-drawer" role="dialog" aria-modal="true" aria-label="Project knowledge source">
		<div class="pk-drawer__head">
			<div>
				<span class="pk-result__kind">{selected.kind ?? selected.reference}</span>
				<h3>{selected.title ?? selected.reference}</h3>
				{#if selected.path}<p>{selected.path}</p>{/if}
			</div>
			<button type="button" aria-label="Close source" onclick={() => (selected = null)}><X size={16} /></button>
		</div>
		{#if reading}<p class="pk-muted">Reading retained source…</p>{:else}<pre>{selected.body}</pre>{/if}
		<div class="pk-drawer__meta">
			<span>{selected.status ?? 'unclassified'}</span>
			{#if selected.current === false}<span>historical revision</span>{/if}
			{#if selected.content_hash}<span class="pk-hash">{selected.content_hash}</span>{/if}
		</div>
	</div>
{/if}

<style>
	.pk-body { display: flex; flex-direction: column; gap: 10px; }
	.pk-search { display: flex; gap: 6px; }
	.pk-search input { min-width: 0; flex: 1; border: 1px solid var(--border-default); background: var(--surface-1); color: var(--text-strong); padding: 7px 9px; font-size: 12px; }
	.pk-search button, .pk-drawer__head button { display: inline-flex; align-items: center; justify-content: center; border: 1px solid var(--border-default); color: var(--text-2); padding: 0 9px; }
	.pk-status, .pk-meta, .pk-result__path, .pk-drawer__meta { color: var(--text-3); font: 11px var(--font-mono, monospace); }
	.pk-meta, .pk-drawer__meta { display: flex; flex-wrap: wrap; gap: 10px; }
	.pk-results { display: flex; flex-direction: column; gap: 2px; margin: 0; padding: 0; list-style: none; }
	.pk-result { display: grid; grid-template-columns: 72px minmax(0, 1fr); gap: 2px 8px; width: 100%; padding: 8px; text-align: left; border: 1px solid transparent; color: var(--text-strong); }
	.pk-result:hover, .pk-result:focus-visible { border-color: var(--interactive-active-border); background: var(--surface-2); }
	.pk-result__kind { color: var(--status-info-fg); font: 10px var(--font-mono, monospace); text-transform: uppercase; }
	.pk-result__title { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-size: 12px; }
	.pk-result__path { grid-column: 2; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
	.pk-more { display: flex; }
	.pk-more-error { margin: 0; color: var(--status-danger-fg); font-size: 12px; }
	.pk-muted { color: var(--text-3); font-size: 12px; }
	.pk-drawer { position: fixed; inset: 12% 8% 8% auto; z-index: 20; display: flex; width: min(720px, 84vw); flex-direction: column; gap: 12px; border: 1px solid var(--border-default); background: var(--surface-card); padding: 18px; box-shadow: var(--shadow-3); }
	.pk-drawer__head { display: flex; justify-content: space-between; gap: 12px; border-bottom: 1px solid var(--border-default); padding-bottom: 10px; }
	.pk-drawer h3 { margin: 3px 0; color: var(--text-strong); font-size: 16px; }
	.pk-drawer p { margin: 0; color: var(--text-3); font: 11px var(--font-mono, monospace); }
	.pk-drawer pre { min-height: 0; flex: 1; overflow: auto; white-space: pre-wrap; color: var(--text-1); font: 12px/1.55 var(--font-mono, monospace); }
	.pk-drawer__meta { border-top: 1px solid var(--border-default); padding-top: 8px; }
	.pk-hash { min-width: 0; overflow-wrap: anywhere; }
</style>
