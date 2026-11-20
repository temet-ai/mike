<script lang="ts">
	import { DatabaseZap } from '@lucide/svelte';
	import { resolve } from '$app/paths';
	import SectionCard from '$lib/components/detail/SectionCard.svelte';
	import { Button } from '$lib/components/ui/button';
	import {
		getSessionCacheBreaks,
		type CacheBreak,
		type CacheBreakGroup
	} from '$lib/api/cacheBreaks';
	import CacheBreakGroups from './CacheBreakGroups.svelte';
	import CacheBreakTable from './CacheBreakTable.svelte';
	import { formatTokens, plural, totals } from './cacheBreakFormat';

	interface Props {
		sessionId: string;
		now: number;
		grouped?: boolean;
	}

	const PAGE_SIZE = 50;

	let { sessionId, now, grouped = false }: Props = $props();

	let groups = $state<CacheBreakGroup[]>([]);
	let breaks = $state<CacheBreak[]>([]);
	let hasMore = $state(false);
	let loadingMore = $state(false);
	let moreError = $state(false);
	let attemptedForSession = $state<string | null>(null);

	const sum = $derived(totals(groups));
	const present = $derived(groups.length > 0 || breaks.length > 0);

	// The panel exists only for sessions with a break, so a failed or empty load draws nothing.
	async function loadFirst(id: string): Promise<void> {
		try {
			const result = await getSessionCacheBreaks(id, { limit: PAGE_SIZE, offset: 0 });
			if (id !== sessionId) return;
			groups = result.groups ?? [];
			breaks = result.breaks ?? [];
			hasMore = result.has_more ?? false;
		} catch {
			if (id !== sessionId) return;
			groups = [];
			breaks = [];
			hasMore = false;
		}
	}

	async function loadMore(): Promise<void> {
		const id = sessionId;
		loadingMore = true;
		moreError = false;
		try {
			const result = await getSessionCacheBreaks(id, { limit: PAGE_SIZE, offset: breaks.length });
			if (id !== sessionId) return;
			breaks = [...breaks, ...result.breaks];
			hasMore = result.has_more;
		} catch {
			if (id === sessionId) moreError = true;
		} finally {
			loadingMore = false;
		}
	}

	$effect(() => {
		const id = sessionId;
		if (attemptedForSession === id) return;
		attemptedForSession = id;
		groups = [];
		breaks = [];
		hasMore = false;
		moreError = false;
		void loadFirst(id);
	});
</script>

{#if present}
	<SectionCard
		title="Prompt cache"
		titleIcon={DatabaseZap}
		collapsible
		bordered={!grouped}
		sectionKey={`session:${sessionId}:cache-breaks-open`}
		titleTooltip="Requests that rewrote prompt cache they should have read, or resumed after it expired, and what changed the prompt."
	>
		{#snippet headerActions()}
			<span class="scb-meta" data-slot="session-cache-breaks-summary"
				>{plural(sum.breaks, 'break')} · {formatTokens(sum.tokens)} tokens rewritten</span
			>
		{/snippet}
		<div class="scb-body" data-slot="session-cache-breaks-panel">
			<CacheBreakGroups {groups} />
			<CacheBreakTable {breaks} {now} label="Cache breaks in this session" />
			<div class="scb-foot">
				{#if hasMore}
					<Button
						variant="outline"
						size="sm"
						onclick={() => void loadMore()}
						disabled={loadingMore}
					>
						More
					</Button>
				{/if}
				{#if moreError}
					<span class="scb-note" role="alert">Couldn't load more breaks.</span>
				{/if}
				<a class="scb-link" href={resolve('/diagnostics/prompt-cache')}>All sessions</a>
			</div>
		</div>
	</SectionCard>
{/if}

<style>
	.scb-meta {
		font-family: var(--font-mono, monospace);
		font-size: var(--text-caption, 11px);
		color: var(--text-3);
		white-space: nowrap;
	}
	.scb-body {
		display: flex;
		flex-direction: column;
		gap: var(--space-4);
	}
	.scb-foot {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: var(--space-3);
	}
	.scb-note {
		font-size: var(--text-sm);
		color: var(--status-danger-fg);
	}
	.scb-link {
		margin-inline-start: auto;
		color: var(--interactive-active-fg);
		font-size: var(--text-sm);
		text-decoration: none;
	}
	.scb-link:hover {
		text-decoration: underline;
		text-underline-offset: 2px;
	}
	.scb-link:focus-visible {
		outline: 2px solid var(--interactive-focus-ring);
		outline-offset: 2px;
	}
</style>
