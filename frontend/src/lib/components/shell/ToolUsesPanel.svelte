<script module lang="ts">
	function ordinalSuffix(n: number): string {
		const v = n % 100;
		if (v >= 11 && v <= 13) return 'th';
		switch (n % 10) {
			case 1:
				return 'st';
			case 2:
				return 'nd';
			case 3:
				return 'rd';
			default:
				return 'th';
		}
	}
</script>

<script lang="ts">
	import { untrack } from 'svelte';
	import { Wrench } from '@lucide/svelte';
	import SectionCard from '$lib/components/detail/SectionCard.svelte';
	import ToolCallCard from '$lib/components/tool-calls/ToolCallCard.svelte';
	import { listSessionToolUses, ApiError } from '$lib/api';
	import type { ToolUseDto } from '$lib/types';
	import { formatRelative } from '$lib/time';
	import { onApiCacheUpdate } from '$lib/stores/cache/apiCache.svelte';

	interface Props {
		sessionId: string;
		now: number;
		embedded?: boolean;
	}

	let { sessionId, now, embedded = false }: Props = $props();

	const sectionOpenKey = $derived(`session:${sessionId}:tool-uses-open`);

	let rows = $state<ToolUseDto[]>([]);
	let totalCount = $state(0);
	let loading = $state(false);
	let errorMessage = $state<string | null>(null);
	let loadedSessionId = $state<string | null>(null);
	let loadingMore = $state(false);
	let loadMoreError = $state<string | null>(null);

	const PAGE_SIZE = 1000;

	$effect(() => {
		const id = sessionId;

		function refetch(): void {
			if (!id) return;
			loading = true;
			errorMessage = null;
			loadingMore = false;
			loadMoreError = null;
			rows = [];
			totalCount = 0;
			listSessionToolUses(id, { order: 'asc', limit: PAGE_SIZE })
				.then((envelope) => {
					if (sessionId !== id) return;
					rows = envelope.rows;
					totalCount = envelope.total_count;
					loadedSessionId = id;
				})
				.catch((err) => {
					if (sessionId !== id) return;
					errorMessage = err instanceof ApiError ? err.message : String(err);
					rows = [];
					totalCount = 0;
					loadedSessionId = null;
				})
				.finally(() => {
					if (sessionId !== id) return;
					loading = false;
				});
		}

		untrack(() => {
			if (!id) {
				rows = [];
				totalCount = 0;
				loading = false;
				errorMessage = null;
				loadedSessionId = null;
				loadingMore = false;
				loadMoreError = null;
				return;
			}
			refetch();
		});

		const unsub = onApiCacheUpdate(
			`/api/sessions/${id}/tool-uses`,
			() => {
				void refetch();
			},
			{ prefix: true }
		);
		return unsub;
	});

	const FILE_TOOLS: ReadonlySet<string> = new Set([
		'Edit',
		'Write',
		'MultiEdit',
		'NotebookEdit',
		'NotebookEditCell'
	]);

	function isFileTool(name: string): boolean {
		return FILE_TOOLS.has(name);
	}

	function shortToolId(id: string): string {
		if (id.length <= 12) return id;
		return `${id.slice(0, 12)}…`;
	}

	const remaining = $derived(Math.max(0, totalCount - rows.length));

	async function loadMore(): Promise<void> {
		const id = sessionId;
		if (!id || loadingMore) return;
		loadingMore = true;
		loadMoreError = null;
		try {
			const envelope = await listSessionToolUses(id, {
				order: 'asc',
				offset: rows.length,
				limit: PAGE_SIZE
			});
			if (sessionId !== id) return;
			rows = [...rows, ...envelope.rows];
			totalCount = envelope.total_count;
		} catch (err) {
			if (sessionId !== id) return;
			loadMoreError = err instanceof ApiError ? err.message : String(err);
		} finally {
			if (sessionId === id) loadingMore = false;
		}
	}
</script>

<SectionCard
	title="Tool uses"
	titleIcon={Wrench}
	ariaLabel="Session tool uses"
	collapsible
	bordered={!embedded}
	sectionKey={sectionOpenKey}
	defaultCollapsed
>
	{#snippet headerActions()}
		<span class="tup-meta">{totalCount} {totalCount === 1 ? 'call' : 'calls'}</span>
	{/snippet}
	{#snippet children()}
		{#if loading && loadedSessionId !== sessionId}
			<div class="tool-uses-status" role="status" aria-live="polite">Loading…</div>
		{:else if errorMessage}
			<div class="tool-uses-status tool-uses-status--error" role="alert">
				Couldn't load tool uses: {errorMessage}
			</div>
		{:else if rows.length === 0}
			<div class="tool-uses-empty" role="status" aria-live="polite">
				<Wrench size={24} aria-hidden="true" class="empty-icon" />
				<span class="tool-uses-empty__text">No tool uses recorded</span>
				<span class="tool-uses-empty__sub">
					Tool calls appear here once the session makes its first tool use.
				</span>
			</div>
		{:else}
			<ul class="tool-list" role="list">
				{#each rows as row (row.tool_id)}
					<li class="tool-row" class:tool-row--file={isFileTool(row.tool_name)}>
						<ToolCallCard
							compressionTarget={row.use_id && row.result?.request_id
								? {
										kind: 'tool_result',
										request_id: row.result.request_id,
										tool_use_id: row.use_id
									}
								: undefined}
							name={row.tool_name}
							input={row.input}
							collapsible
							result={row.result != null
								? {
										toolName: row.tool_name,
										content: row.result.content,
										isError: row.result.is_error
									}
								: undefined}
						>
							{#snippet meta()}
								<span class="tool-row__meta">
									<span
										class="tool-row__ord"
										title="{row.ord + 1}{ordinalSuffix(
											row.ord + 1
										)} tool call in request #{row.request_id}">#{row.ord + 1}</span
									>
									<span class="tool-row__ts" title={row.created_at}>
										{formatRelative(row.created_at, now)}
									</span>
									<span class="tool-row__req">req #{row.request_id}</span>
									<span
										class="tool-row__tool-id"
										title="Anthropic tool call ID — matches the toolu_xxx ID in the raw SSE stream. Full ID: {row.tool_id}"
										>{shortToolId(row.tool_id)}</span
									>
								</span>
							{/snippet}
						</ToolCallCard>
					</li>
				{/each}
			</ul>
			{#if remaining > 0}
				<div class="tool-uses-footer">
					<span class="tool-uses-footer__count" role="status">
						Showing {rows.length} of {totalCount}; {remaining} more not loaded.
					</span>
					<span class="tool-uses-footer__load-more-backing">
						<button
							type="button"
							class="tool-uses-footer__load-more"
							data-augmented-ui="tr-clip bl-clip border"
							disabled={loadingMore}
							onclick={loadMore}
						>
							{loadingMore ? 'Loading…' : 'Load more tool calls'}
						</button>
					</span>
					{#if loadMoreError}
						<span class="tool-uses-footer__error" role="alert">
							{loadMoreError}
						</span>
					{/if}
				</div>
			{/if}
		{/if}
	{/snippet}
</SectionCard>

<style>
	.tup-meta {
		font-family: var(--font-mono, monospace);
		font-size: var(--text-caption, 11px);
		color: var(--text-3);
		white-space: nowrap;
	}
	.tool-uses-status {
		padding: 8px 0;
		font-size: 12px;
		color: var(--text-3);
	}
	.tool-uses-status--error {
		color: var(--status-danger-fg);
	}
	.tool-uses-empty {
		display: flex;
		flex-direction: column;
		align-items: center;
		justify-content: center;
		gap: 8px;
		padding: 32px 24px;
		color: var(--text-3);
	}
	.tool-uses-empty :global(.empty-icon) {
		opacity: 0.4;
	}
	.tool-uses-empty__text {
		font-size: 12px;
	}
	.tool-uses-empty__sub {
		font-size: var(--text-caption);
		color: var(--text-faint);
		text-align: center;
	}
	.tool-list {
		list-style: none;
		margin: 0;
		padding: 0;
		display: flex;
		flex-direction: column;
		gap: 6px;
	}
	.tool-row {
		min-width: 0;
	}
	.tool-row__ord {
		font-size: var(--text-label);
		font-weight: 500;
		color: var(--text-3);
		text-decoration: underline;
		text-decoration-color: var(--border-default);
		text-underline-offset: 2px;
		white-space: nowrap;
		font-variant-numeric: tabular-nums;
		font-family: var(--font-mono);
	}
	.tool-row__meta {
		display: flex;
		align-items: center;
		gap: 8px;
		flex-wrap: wrap;
	}
	.tool-row__ts,
	.tool-row__req,
	.tool-row__tool-id {
		font-size: var(--text-caption);
		color: var(--text-3);
		white-space: nowrap;
	}
	.tool-row__tool-id {
		font-family: var(--font-mono);
	}
	.tool-uses-footer {
		display: flex;
		flex-direction: column;
		align-items: stretch;
		gap: 8px;
		padding: 12px 0 0;
		border-top: 1px solid var(--border-subtle);
	}
	.tool-uses-footer__count {
		font-size: var(--text-caption);
		color: var(--text-3);
		text-align: center;
	}
	.tool-uses-footer__load-more-backing {
		display: flex;
		align-self: stretch;
		background: var(--surface-card);
	}
	.tool-uses-footer__load-more {
		--edge: 5px;
		--aug-tr: var(--edge);
		--aug-bl: var(--edge);
		--aug-border-all: 1px;
		--aug-border-bg: var(--border-default);
		display: inline-flex;
		align-items: center;
		justify-content: center;
		width: 100%;
		padding: 6px 12px;
		background: var(--surface-card);
		color: var(--text-2);
		font-size: var(--text-label);
		text-transform: uppercase;
		letter-spacing: 0.04em;
		cursor: pointer;
		transition: color var(--dur-1) var(--ease-snap);
	}
	.tool-uses-footer__load-more:hover {
		--aug-border-bg: var(--interactive-active-fg);
		color: var(--text-strong);
	}
	.tool-uses-footer__load-more:focus-visible {
		outline: none;
		box-shadow: 0 0 0 2px var(--interactive-focus-ring);
	}
	.tool-uses-footer__load-more:disabled {
		cursor: not-allowed;
		opacity: 0.6;
	}
	.tool-uses-footer__error {
		font-size: var(--text-caption);
		color: var(--status-danger-fg);
		text-align: center;
	}
</style>
