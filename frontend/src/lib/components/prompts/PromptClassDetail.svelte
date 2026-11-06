<script lang="ts">
	import { onMount } from 'svelte';
	import { resolve } from '$app/paths';
	import {
		ApiError,
		getCompositionRevisions,
		getPromptClassMain,
		getPromptClassSubagent,
		UNRESOLVED_PERSONA_SEGMENT
	} from '$lib/api';
	import type { CompositionRevision, PromptClassDetailDto } from '$lib/types';
	import { block2Text, hasBlock2Drift } from '$lib/prompts/catalogueRow';
	import ClassChip from './ClassChip.svelte';
	import BlockCard, { type BlockBadge } from './BlockCard.svelte';
	import PromptCompositionDiff from './PromptCompositionDiff.svelte';
	import LoadErrorBanner from '$lib/components/shared/LoadErrorBanner.svelte';
	import Tooltip from '$lib/components/shared/Tooltip.svelte';
	import * as Tabs from '$lib/components/ui/tabs';
	import { formatRelative } from '$lib/time';
	import { ArrowLeft, ArrowUpRight } from '@lucide/svelte';

	interface Props {
		lane: 'main' | 'subagent';
		persona: string | null;
	}

	let { lane, persona }: Props = $props();

	type LoadState = 'loading' | 'ready' | 'not-found' | 'error';
	let loadState = $state<LoadState>('loading');
	let error = $state<string | null>(null);
	let detail = $state<PromptClassDetailDto | null>(null);
	let revisions = $state<CompositionRevision[]>([]);
	let revisionsError = $state<string | null>(null);

	const OBSERVATIONS_PAGE_SIZE = 20;
	let observationsOffset = $state(0);
	let observationsHasMore = $state(false);
	let observationsLoadingMore = $state(false);
	let observationsLoadMoreError = $state<string | null>(null);

	let activeView = $state<'composition' | 'drift'>('composition');

	function fetchDetail(offset: number) {
		const params = { limit: OBSERVATIONS_PAGE_SIZE, offset };
		return lane === 'main'
			? getPromptClassMain(params)
			: getPromptClassSubagent(persona ?? UNRESOLVED_PERSONA_SEGMENT, params);
	}

	async function load(): Promise<void> {
		loadState = 'loading';
		error = null;
		observationsOffset = 0;
		try {
			const result = await fetchDetail(0);
			detail = result;
			observationsOffset = result.recent_observations.length;
			observationsHasMore = result.recent_observations.length < result.observations_total_count;
			loadState = 'ready';

			if (result.registration.composition_id !== null) {
				try {
					revisions = await getCompositionRevisions(result.registration.composition_id);
					revisionsError = null;
				} catch (err) {
					revisions = [];
					revisionsError = err instanceof ApiError ? err.message : String(err);
				}
			} else {
				revisions = [];
				revisionsError = null;
			}
		} catch (err) {
			detail = null;
			if (err instanceof ApiError && err.status === 404) {
				loadState = 'not-found';
			} else {
				loadState = 'error';
				error = err instanceof ApiError ? err.message : String(err);
			}
		}
	}

	async function loadMoreObservations(): Promise<void> {
		if (observationsLoadingMore || !observationsHasMore) return;
		observationsLoadingMore = true;
		observationsLoadMoreError = null;
		try {
			const page = await fetchDetail(observationsOffset);
			if (detail !== null) {
				detail = {
					...detail,
					recent_observations: [...detail.recent_observations, ...page.recent_observations],
					observations_total_count: page.observations_total_count
				};
			}
			observationsOffset += page.recent_observations.length;
			observationsHasMore = observationsOffset < page.observations_total_count;
		} catch (err) {
			observationsLoadMoreError = err instanceof ApiError ? err.message : String(err);
		} finally {
			observationsLoadingMore = false;
		}
	}

	onMount(() => {
		void load();
	});

	const displayName = $derived(
		lane === 'main' ? 'Claude Code (main agent)' : (persona ?? 'Subagent (persona unresolved)')
	);
	const registered = $derived(detail?.registration.registered ?? false);
	const registeredBlocks = $derived(revisions[0]?.blocks ?? null);
	const observedBlocks = $derived(detail?.sample_blocks ?? null);
	const isDrifted = $derived(
		registered &&
			registeredBlocks !== null &&
			observedBlocks !== null &&
			observedBlocks.length > 0 &&
			hasBlock2Drift(registeredBlocks, observedBlocks)
	);
	const primaryBlocks = $derived(registered ? registeredBlocks : observedBlocks);

	const observationCount = $derived(detail?.observation_count ?? null);
	const observationsTotalCount = $derived(detail?.observations_total_count ?? null);
	const lastSeenLabel = $derived(detail?.last_seen ? formatRelative(detail.last_seen) : null);
	const firstSeenLabel = $derived(detail?.first_seen ? formatRelative(detail.first_seen) : null);

	const classIdentityQuery = $derived(
		lane === 'main'
			? 'lane=main'
			: `persona=${encodeURIComponent(persona ?? UNRESOLVED_PERSONA_SEGMENT)}`
	);
	const registerHref = $derived.by(() => {
		const newestObservation = detail?.recent_observations[0];
		return newestObservation !== undefined
			? (`/prompts/new?from=request:${newestObservation.request_id}&${classIdentityQuery}` as const)
			: (`/prompts/new?${classIdentityQuery}` as const);
	});

	function labelFor(idx: number): string {
		if (idx === 0) return 'block[0] · billing-header';
		if (idx === 1) return 'block[1] · identity preamble';
		if (idx === 2) return 'block[2] · skeleton + role overlay';
		return `block[${idx}]`;
	}
	function railRoleFor(idx: number): 'volatile' | 'preamble' | 'skeleton' {
		if (idx === 0) return 'volatile';
		if (idx === 2) return 'skeleton';
		return 'preamble';
	}
	function badgeFor(idx: number): BlockBadge {
		return idx <= 1 ? 'volatile' : 'authored';
	}

	function requestHref(sessionId: string, requestId: string) {
		return `/sessions/${encodeURIComponent(sessionId)}/requests/${requestId}` as const;
	}
</script>

<section class="flex flex-col gap-6 overflow-y-auto p-6" data-slot="prompt-class-detail">
	<a
		href={resolve('/prompts')}
		class="flex w-fit items-center gap-1.5 text-label text-[var(--text-3)] transition-colors hover:text-[var(--interactive-active-fg)]"
	>
		<ArrowLeft class="size-3.5" aria-hidden="true" />
		Back to catalogue
	</a>

	{#if loadState === 'loading'}
		<div class="detail-skeleton" data-slot="prompt-class-detail-loading">
			<div class="skeleton-bar" style="width: 40%; height: 20px;"></div>
			<div class="skeleton-bar mt-3" style="width: 100%; height: 200px;"></div>
		</div>
	{:else if loadState === 'error'}
		<LoadErrorBanner
			title="Couldn't load this class"
			detail={error ?? ''}
			onRetry={() => void load()}
			backHref="/prompts"
			backLabel="Back to catalogue"
		/>
	{:else if loadState === 'not-found'}
		<div
			class="flex flex-col items-center justify-center gap-3 p-12 text-center"
			data-slot="prompt-class-detail-unknown"
		>
			<p class="text-sm font-medium text-[var(--text-strong)]">
				No traffic captured for {displayName} yet
			</p>
			<p class="max-w-md text-label text-[var(--text-3)]">
				This class has never been observed and nothing is registered for it — once traffic matches,
				it will appear here.
			</p>
		</div>
	{:else if detail !== null}
		<header class="detail-header">
			<div class="flex flex-wrap items-center gap-2">
				<ClassChip label={displayName} isActive />
				<span
					class="badge-nx"
					class:badge-error={isDrifted}
					class:badge-warning={!registered && !isDrifted}
					class:badge-active={registered && !isDrifted}
				>
					<span class="badge-nx-dot"></span>
					{isDrifted ? 'drifted' : registered ? 'registered' : 'unregistered'}
				</span>
				{#if detail.registration.mode}
					<span class="badge-nx badge-muted">{detail.registration.mode}</span>
				{/if}
			</div>
			<div class="mt-1 flex flex-wrap items-center gap-3 text-label text-[var(--text-3)]">
				<span>
					{#if observationsTotalCount === 0 || observationsTotalCount === null}
						<Tooltip text="No traffic observed for this class, ever.">0 observations, ever</Tooltip>
					{:else}
						{observationCount?.toLocaleString('en-US') ?? 0} in the last 7 days ·
						{observationsTotalCount.toLocaleString('en-US')} captured all time{#if lastSeenLabel}
							&nbsp;· last seen {lastSeenLabel}{/if}
					{/if}
				</span>
				{#if firstSeenLabel}
					<span>first seen {firstSeenLabel}</span>
				{/if}
				{#if detail.registration.agent_definition_path}
					<span class="font-mono">{detail.registration.agent_definition_path}</span>
				{/if}
			</div>
		</header>

		{#snippet compositionBlocks()}
			{#if !registered && observedBlocks !== null && observedBlocks.length > 0}
				<p class="mb-3 text-label text-[var(--text-3)]">
					Showing the most recently observed capture — this class is not registered yet.
				</p>
			{/if}
			{#if revisionsError !== null}
				<div class="mb-3 text-label text-[var(--status-warning-fg)]">
					Couldn't load the registered composition body: {revisionsError}
				</div>
			{/if}
			{#if primaryBlocks !== null && primaryBlocks.length > 0}
				<div class="flex flex-col gap-3">
					{#each primaryBlocks as block, idx (idx)}
						<BlockCard
							label={labelFor(idx)}
							text={block.text}
							badge={badgeFor(idx)}
							railRole={railRoleFor(idx)}
							editable={false}
						/>
					{/each}
				</div>
			{:else}
				<p class="text-label text-[var(--text-3)]">No block content available.</p>
			{/if}
		{/snippet}

		<section
			class="detail-frame"
			data-augmented-ui="tl-clip br-clip border"
			aria-label="Composition and observation history"
			data-slot="prompt-class-frame"
		>
			<div class="detail-sub" data-slot="prompt-class-blocks">
				{#if isDrifted}
					<Tabs.Root bind:value={activeView} class="detail-view-tabs">
						<div class="detail-sub-head">
							<Tabs.List aria-label="View">
								<Tabs.Trigger value="composition">Composition</Tabs.Trigger>
								<Tabs.Trigger value="drift">Drift</Tabs.Trigger>
							</Tabs.List>
							<span class="detail-panel-sub">registered vs. most recently observed</span>
						</div>
						<Tabs.Content value="composition" data-slot="prompt-class-composition">
							{@render compositionBlocks()}
						</Tabs.Content>
						<Tabs.Content value="drift" data-slot="prompt-class-drift">
							<PromptCompositionDiff
								composition={block2Text(registeredBlocks)}
								observed={block2Text(observedBlocks)}
								leftLabel="Registered"
								rightLabel="Observed"
							/>
						</Tabs.Content>
					</Tabs.Root>
				{:else}
					<header class="detail-sub-head">
						<span class="detail-panel-tag-backing">
							<h3 class="detail-panel-tag" data-augmented-ui="tl-clip br-clip border">
								Composition
							</h3>
						</span>
					</header>
					<div data-slot="prompt-class-composition">
						{@render compositionBlocks()}
					</div>
				{/if}
			</div>

			<div class="detail-sub" data-slot="prompt-class-observations">
				<header class="detail-sub-head">
					<span class="detail-panel-tag-backing">
						<h3 class="detail-panel-tag" data-augmented-ui="tl-clip br-clip border">
							Observation history
						</h3>
					</span>
					<span class="detail-panel-count">
						{detail.recent_observations.length} of {detail.observations_total_count.toLocaleString(
							'en-US'
						)}, newest first
					</span>
				</header>
				{#if detail.recent_observations.length === 0}
					<p class="text-label text-[var(--text-3)]">No observations captured yet.</p>
				{:else}
					<div class="flex flex-col gap-1.5" data-slot="prompt-class-observations-list">
						{#each detail.recent_observations as obs (obs.request_id)}
							<a
								href={resolve(requestHref(obs.session_id, obs.request_id))}
								class="observation-row"
								data-slot="prompt-class-observation-row"
							>
								<span class="font-mono text-label text-[var(--text-2)]">
									request #{obs.request_id}
								</span>
								<span class="font-mono text-label text-[var(--text-3)]">
									session {obs.session_id}
								</span>
								<span class="text-label text-[var(--text-3)]">{formatRelative(obs.created_at)}</span
								>
								{#if obs.altered}
									<span class="badge-nx badge-warning">altered</span>
								{/if}
								<span
									class="observation-sha font-mono text-label text-[var(--text-faint)]"
									title={obs.content_sha256}
								>
									{obs.content_sha256}
								</span>
								<ArrowUpRight
									class="ml-auto size-3 shrink-0 text-[var(--text-3)]"
									aria-hidden="true"
								/>
							</a>
						{/each}
					</div>
					{#if observationsLoadMoreError !== null}
						<p class="mt-2 text-label text-[var(--status-warning-fg)]">
							Couldn't load more: {observationsLoadMoreError}
						</p>
					{/if}
					{#if observationsHasMore}
						<button
							type="button"
							class="detail-load-more"
							disabled={observationsLoadingMore}
							onclick={() => void loadMoreObservations()}
						>
							{observationsLoadingMore ? 'loading…' : 'load more'}
						</button>
					{/if}
				{/if}
			</div>
		</section>

		<div class="flex justify-end">
			{#if !registered}
				<span class="detail-cta-backing">
					<a
						href={resolve(registerHref)}
						class="detail-cta"
						data-augmented-ui="tr-clip bl-clip border"
						data-slot="prompt-class-register-cta"
					>
						Register this
					</a>
				</span>
			{/if}
		</div>
	{/if}
</section>

<style>
	.detail-header {
		display: flex;
		flex-direction: column;
		gap: 2px;
	}

	.detail-frame {
		--edge: 10px;
		--aug-tl: var(--edge);
		--aug-br: var(--edge);
		--aug-border-all: 1px;
		--aug-border-bg: linear-gradient(
			135deg,
			var(--data-teal-fg) 0%,
			var(--data-teal-border) 35%,
			var(--border-default) 70%
		);
		display: flex;
		flex-direction: column;
		background: var(--surface-card);
	}
	.detail-sub {
		display: flex;
		flex-direction: column;
		gap: 12px;
		padding: calc(var(--edge) + 4px);
	}
	.detail-sub + .detail-sub {
		border-top: 1px solid var(--border-subtle);
	}
	.detail-sub-head {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: 8px;
	}
	:global(.detail-view-tabs) {
		display: flex;
		flex-direction: column;
		gap: 12px;
	}
	.detail-panel-tag-backing {
		display: inline-flex;
		background: var(--surface-card);
	}
	.detail-panel-tag {
		--aug-tl: 6px;
		--aug-tr: 0;
		--aug-br: 6px;
		--aug-bl: 0;
		--aug-border-all: 1px;
		--aug-border-bg: var(--data-teal-border);
		margin: 0;
		font-family: var(--font-display, 'Syne Variable', sans-serif);
		font-weight: 800;
		font-size: var(--text-label);
		text-transform: uppercase;
		letter-spacing: 0.06em;
		color: var(--data-teal-fg);
		padding: 3px 8px;
		background:
			linear-gradient(0deg, var(--data-teal-bg), var(--data-teal-bg)), var(--surface-card);
	}
	.detail-panel-sub,
	.detail-panel-count {
		font-family: 'JetBrains Mono Variable', ui-monospace, monospace;
		font-size: var(--text-label);
		color: var(--text-3);
	}

	.detail-cta-backing {
		display: inline-flex;
		background: var(--surface-bg);
	}
	.detail-cta {
		--edge: 8px;
		--aug-tl: 0;
		--aug-tr: var(--edge);
		--aug-br: 0;
		--aug-bl: var(--edge);
		--aug-border-all: 1px;
		--aug-border-bg: var(--interactive-active-border);
		display: inline-flex;
		align-items: center;
		background: var(--surface-card);
		color: var(--interactive-active-fg);
		padding: 7px 16px;
		font-size: var(--text-label);
		font-weight: 600;
		text-decoration: none;
		transition: color 0.12s ease;
	}
	.detail-cta:hover,
	.detail-cta:focus-visible {
		background: var(--surface-2);
	}
	.detail-cta:focus-visible {
		outline: 2px solid var(--interactive-focus-ring);
		outline-offset: 2px;
	}

	.detail-skeleton {
		display: flex;
		flex-direction: column;
	}
	.skeleton-bar {
		background: var(--surface-2);
		border-radius: 4px;
	}

	.observation-row {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: 10px;
		border-radius: 6px;
		border: 1px solid var(--border-subtle);
		background: var(--surface-bg);
		padding: 7px 10px;
		text-decoration: none;
		transition: border-color 0.12s ease;
	}
	.observation-row:hover {
		border-color: var(--interactive-active-border);
	}
	.observation-sha {
		overflow-wrap: anywhere;
	}

	.detail-load-more {
		margin-top: 10px;
		border: 1px solid var(--border-default);
		border-radius: 6px;
		background: var(--surface-card);
		color: var(--text-2);
		padding: 5px 12px;
		font-size: var(--text-label);
		cursor: pointer;
	}
	.detail-load-more:disabled {
		opacity: 0.6;
		cursor: not-allowed;
	}
</style>
