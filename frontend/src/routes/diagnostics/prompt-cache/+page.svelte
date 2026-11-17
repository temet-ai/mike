<script lang="ts">
	import { onMount } from 'svelte';
	import { page } from '$app/state';
	import { goto } from '$app/navigation';
	import { resolve } from '$app/paths';
	import { DatabaseZap, RefreshCw, ScanSearch } from '@lucide/svelte';
	import { ApiError } from '$lib/api';
	import {
		getCacheBreaks,
		rescanCacheBreaks,
		type CacheBreak,
		type CacheBreakGroup
	} from '$lib/api/cacheBreaks';
	import CacheBreakGroups from '$lib/components/cache-breaks/CacheBreakGroups.svelte';
	import CacheBreakTable from '$lib/components/cache-breaks/CacheBreakTable.svelte';
	import { formatTokens, plural, totals } from '$lib/components/cache-breaks/cacheBreakFormat';
	import SectionCard from '$lib/components/detail/SectionCard.svelte';
	import LoadErrorBanner from '$lib/components/shared/LoadErrorBanner.svelte';
	import PageIntro from '$lib/components/shared/PageIntro.svelte';
	import { Button } from '$lib/components/ui/button';
	import { Select } from '$lib/components/ui/select';

	const WINDOWS = [
		{ value: '24', label: '24 hours' },
		{ value: '168', label: '7 days' }
	];
	const DEFAULT_HOURS = '24';
	const PAGE_SIZE = 100;
	const RESCAN_DAYS = 7;

	function initialWindow(): string {
		const raw = page.url.searchParams.get('hours');
		return raw !== null && WINDOWS.some((w) => w.value === raw) ? raw : DEFAULT_HOURS;
	}

	let windowHours = $state(initialWindow());
	let groups = $state<CacheBreakGroup[]>([]);
	let breaks = $state<CacheBreak[]>([]);
	let hasMore = $state(false);
	let loaded = $state(false);
	let loading = $state(true);
	let loadingMore = $state(false);
	let error = $state<string | null>(null);
	let moreError = $state<string | null>(null);
	let rescanning = $state(false);
	let rescanMessage = $state<string | null>(null);
	let rescanError = $state<string | null>(null);
	let loadedAt = $state(Date.now());
	let statusAnnouncement = $state('');

	const sum = $derived(totals(groups));

	async function load(): Promise<void> {
		loading = true;
		error = null;
		moreError = null;
		try {
			const result = await getCacheBreaks({
				hours: Number(windowHours),
				limit: PAGE_SIZE,
				offset: 0
			});
			groups = result.groups;
			breaks = result.breaks;
			hasMore = result.has_more;
			loaded = true;
			loadedAt = Date.now();
			statusAnnouncement = `Loaded ${result.breaks.length} cache breaks.`;
		} catch (err) {
			error = err instanceof ApiError ? err.message : String(err);
			statusAnnouncement = "Couldn't load cache breaks.";
		} finally {
			loading = false;
		}
	}

	async function loadMore(): Promise<void> {
		loadingMore = true;
		moreError = null;
		try {
			const result = await getCacheBreaks({
				hours: Number(windowHours),
				limit: PAGE_SIZE,
				offset: breaks.length
			});
			breaks = [...breaks, ...result.breaks];
			hasMore = result.has_more;
		} catch (err) {
			moreError = err instanceof ApiError ? err.message : String(err);
		} finally {
			loadingMore = false;
		}
	}

	async function rescan(): Promise<void> {
		rescanning = true;
		rescanMessage = null;
		rescanError = null;
		try {
			await rescanCacheBreaks(RESCAN_DAYS);
			rescanMessage = `Rescanning the last ${RESCAN_DAYS} days. The scan restarts within a minute; reload to see new breaks.`;
		} catch (err) {
			rescanError = err instanceof ApiError ? err.message : String(err);
		} finally {
			rescanning = false;
		}
	}

	function onWindowChange(value: string): void {
		windowHours = value;
		void goto(`${resolve('/diagnostics/prompt-cache')}?hours=${windowHours}`, {
			replace: true,
			reset: false
		});
		void load();
	}

	onMount(() => {
		void load();
	});
</script>

<svelte:head>
	<title>Prompt cache — mike</title>
</svelte:head>

<section class="pc-page" data-slot="prompt-cache-page">
	<PageIntro
		title="Prompt cache"
		level="h1"
		description="Requests that rewrote prompt cache they should have read, or resumed after it expired, and what changed the prompt."
	>
		{#snippet actions()}
			<Select
				label="Window"
				options={WINDOWS}
				bind:value={windowHours}
				onchange={(event) => onWindowChange(event.currentTarget.value)}
				disabled={loading}
			/>
			<Button variant="ghost" size="sm" onclick={() => void load()} disabled={loading}>
				<RefreshCw size={12} aria-hidden="true" />
				Reload
			</Button>
			<Button variant="outline" size="sm" onclick={() => void rescan()} disabled={rescanning}>
				<ScanSearch size={12} aria-hidden="true" />
				Rescan last {RESCAN_DAYS} days
			</Button>
		{/snippet}
	</PageIntro>

	<span class="sr-only" role="status" aria-live="polite">{statusAnnouncement}</span>
	{#if rescanMessage}
		<p class="pc-note" role="status" data-slot="prompt-cache-rescan-status">{rescanMessage}</p>
	{/if}
	{#if rescanError}
		<p class="pc-note pc-note--error" role="alert">Couldn't start the rescan: {rescanError}</p>
	{/if}

	{#if error}
		<LoadErrorBanner
			title="Couldn't load cache breaks"
			detail={error}
			hint="The inspect API at :9989 may not be reachable."
			onRetry={() => void load()}
		/>
	{:else if loading && !loaded}
		<div class="pc-skeleton" aria-hidden="true">
			<div class="pc-skel-row"></div>
			<div class="pc-skel-row pc-skel-row--short"></div>
		</div>
	{:else if loaded && breaks.length === 0 && groups.length === 0}
		<p class="pc-note" data-slot="prompt-cache-empty">No cache breaks in this window.</p>
	{:else if loaded}
		<div class="edge mech-group" data-slot="prompt-cache-bands">
			<SectionCard
				title="By cause"
				titleIcon={DatabaseZap}
				collapsible
				bordered={false}
				sectionKey="prompt-cache:groups"
				titleTooltip="Mike-caused breaks first, then Claude Code, unknown, and expiries by what woke the session."
			>
				{#snippet headerActions()}
					<span class="pc-meta"
						>{plural(sum.breaks, 'break')} · {formatTokens(sum.tokens)} tokens rewritten</span
					>
				{/snippet}
				<CacheBreakGroups {groups} showSessions />
			</SectionCard>

			<SectionCard
				title="Breaks"
				titleIcon={DatabaseZap}
				collapsible
				bordered={false}
				sectionKey="prompt-cache:breaks"
				titleTooltip="Newest first. Each row links to the request that rewrote the cache."
			>
				{#snippet headerActions()}
					<span class="pc-meta">{plural(breaks.length, 'row')} shown</span>
				{/snippet}
				<div class="pc-breaks">
					<CacheBreakTable {breaks} now={loadedAt} showSession />
					{#if hasMore || moreError}
						<div class="pc-foot">
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
								<span class="pc-note pc-note--error" role="alert">{moreError}</span>
							{/if}
						</div>
					{/if}
				</div>
			</SectionCard>
		</div>
	{/if}
</section>

<style>
	.pc-page {
		display: flex;
		flex-direction: column;
		gap: var(--space-4);
		width: 100%;
		max-width: 1200px;
		margin: 0 auto;
		padding: var(--space-4);
		flex: 1 1 auto;
		min-width: 0;
		min-height: 0;
		overflow-y: auto;
	}
	.pc-note {
		margin: 0;
		color: var(--text-2);
	}
	.pc-note--error {
		color: var(--status-danger-fg);
	}
	.pc-meta {
		color: var(--text-3);
		white-space: nowrap;
	}
	.pc-breaks {
		display: flex;
		flex-direction: column;
		gap: var(--space-4);
	}
	.pc-foot {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: var(--space-3);
	}
	.pc-skeleton {
		display: flex;
		flex-direction: column;
		gap: var(--space-2);
	}
	.pc-skel-row {
		height: 14px;
		border-radius: var(--r-tol);
		background: var(--surface-2);
	}
	.pc-skel-row--short {
		width: 60%;
	}
</style>
