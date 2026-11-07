<script lang="ts">
	import { getContext, onMount, untrack, type Component } from 'svelte';
	import { page } from '$app/state';
	import { resolve } from '$app/paths';
	import { goto } from '$app/navigation';
	import { shortSessionId } from '$lib/format';
	import SessionOverview from '$lib/components/shell/SessionOverview.svelte';
	import SessionTimeline from '$lib/components/shell/SessionTimeline.svelte';
	import RequestDrawer from '$lib/components/shell/RequestDrawer.svelte';
	import type { ValidationState } from '$lib/components/shell/ValidationStatusIndicator.svelte';
	import PinToggle from '$lib/components/shared/PinToggle.svelte';
	import CopyIconButton from '$lib/components/shared/CopyIconButton.svelte';
	import Tooltip from '$lib/components/shared/Tooltip.svelte';
	import { ArrowLeft, MessagesSquare, X } from '@lucide/svelte';
	import {
		SESSION_DETAIL_STORE_KEY,
		SESSION_FLOW_STORE_KEY,
		SESSION_LIST_STORE_KEY
	} from '$lib/context-keys';
	import type { SessionDetailStore } from '$lib/stores/cache/sessionDetailStore.svelte';
	import type { SessionListStore } from '$lib/stores/sessions.svelte';
	import { getSessionHead, getSessionValidationStatus } from '$lib/api';
	import type { SessionValidationStatusDto } from '$lib/api';
	import { createSessionFlowStore } from '$lib/session-flow/sessionFlowStore.svelte';
	import type { SessionFlowStore } from '$lib/session-flow/sessionFlowStore.svelte';
	import { createFlowPlayback } from '$lib/session-flow/playback.svelte';
	import { deriveSessionActivity } from '$lib/sessionActivity';
	import { sceneAt } from '$lib/session-flow/projection';
	import {
		activityIdForRequest,
		buildRequestActivityIndex
	} from '$lib/session-flow/requestActivation';
	import { buildFlowRequestFilter } from '$lib/session-flow/requestFilter';
	import type {
		FlowBounds,
		FlowPlayback,
		FlowSeekOptions,
		FlowSelection
	} from '$lib/session-flow/types';
	import type { SessionFlowProps } from '$lib/components/session-flow/SessionFlow.svelte';
	import { openDrawer } from '$lib/stores/shellState.svelte';

	const detailStore = getContext<SessionDetailStore>(SESSION_DETAIL_STORE_KEY);
	const sessionListStore = getContext<SessionListStore>(SESSION_LIST_STORE_KEY);
	const contextFlowStore = getContext<SessionFlowStore | undefined>(SESSION_FLOW_STORE_KEY);
	const flowStore: SessionFlowStore = contextFlowStore ?? createSessionFlowStore();
	const ownsFlowStore = contextFlowStore === undefined;
	let now = $state(Date.now());

	let pageScrollEl = $state<HTMLElement | undefined>(undefined);

	const sessionTitle = $derived(
		detailStore.detail?.summary?.title || `Session ${shortSessionId(page.params.sessionId ?? '')}`
	);

	const sessionId = $derived(page.params.sessionId ?? '');
	const summary = $derived(detailStore.detail?.summary ?? null);
	let flowLiveDecided = $state(false);
	$effect(() => {
		if (flowLiveDecided || !summary || !flowPlayback) return;
		flowLiveDecided = true;
		if (deriveSessionActivity(summary, null, Date.now()).isActive) {
			flowPlayback.setSpeed(1);
			flowPlayback.returnToLive();
		}
	});
	const listEntry = $derived(
		sessionId && sessionListStore
			? (sessionListStore.sessions.find((s) => s.id === sessionId) ?? null)
			: null
	);
	const pinned = $derived(listEntry?.pinned === true || summary?.pinned === true);
	const project = $derived(listEntry?.project ?? summary?.project ?? null);
	const commsHref = $derived(
		resolve(`sessions/${encodeURIComponent(sessionId)}/comms` as `sessions/${string}/comms`)
	);

	function handleTogglePin(): void {
		if (!sessionId || !sessionListStore) return;
		void sessionListStore.togglePin(sessionId, !pinned);
	}

	let promptKindCounts = $state<Record<string, number>>({});

	$effect(() => {
		const sid = sessionId;
		untrack(() => {
			promptKindCounts = {};
			if (!sid) return;
			getSessionHead(sid)
				.then((head) => {
					if (sessionId !== sid) return;
					promptKindCounts = head.prompt_kind_counts ?? {};
				})
				.catch(() => {
					if (sessionId !== sid) return;
					promptKindCounts = {};
				});
		});
	});

	$effect(() => {
		const id = page.params.sessionId;
		untrack(() => {
			if (!id) return;
			void detailStore.load(id);
		});
	});

	let validationStatusDto = $state<SessionValidationStatusDto | null>(null);
	const validationState = $derived<ValidationState>(
		validationStatusDto === null
			? null
			: validationStatusDto.status === 'dispatched'
				? 'pending'
				: validationStatusDto.status === 'resolved_pass'
					? 'pass'
					: 'correction'
	);

	async function loadValidationStatus(sid: string): Promise<void> {
		try {
			const dto = await getSessionValidationStatus(sid);
			if (sessionId !== sid) return;
			validationStatusDto = dto;
		} catch {
			if (sessionId !== sid) return;
			validationStatusDto = null;
		}
	}

	$effect(() => {
		const sid = sessionId;
		untrack(() => {
			validationStatusDto = null;
			if (!sid) return;
			void loadValidationStatus(sid);
		});
	});

	onMount(() => {
		let tick = 0;
		const timer = setInterval(() => {
			now = Date.now();
			tick += 1;
			if (validationStatusDto?.status === 'dispatched' && tick % 5 === 0 && sessionId) {
				void loadValidationStatus(sessionId);
			}
		}, 1_000);
		return () => {
			clearInterval(timer);
			if (ownsFlowStore) flowStore.dispose();
		};
	});

	let flowLoadTriggered = $state(false);
	let flowPlayback = $state<FlowPlayback | null>(null);
	let flowSelection = $state<FlowSelection>(null);
	let flowFollowId = $state<string | null>(null);
	let flowCollapsedIds = $state<ReadonlySet<string>>(new Set());
	let flowBrush = $state<FlowBounds | null>(null);
	let flowRestorePending = $state(false);
	let pendingFlowAt: number | null = null;
	let pendingFlowActorId: string | null = null;
	let pendingFlowActivityId: string | null = null;
	let FlowGraphComponent = $state<Component<SessionFlowProps> | null>(null);
	let flowSectionEl = $state<HTMLElement | undefined>(undefined);
	let pendingFlowRequestId = $state<number | null>(null);

	const requestActivityIndex = $derived(buildRequestActivityIndex(flowStore.dataset));

	const flowScene = $derived(
		flowPlayback
			? sceneAt(
					flowStore.dataset,
					flowPlayback.atMs ??
						flowStore.dataset.observationFrontierMs ??
						flowStore.dataset.bounds?.startMs ??
						0
				)
			: null
	);

	function resetFlowRouteState(): void {
		flowPlayback?.dispose();
		flowPlayback = null;
		flowSelection = null;
		flowFollowId = null;
		flowCollapsedIds = new Set();
		flowBrush = null;
		flowRestorePending = false;
		pendingFlowAt = null;
		pendingFlowActorId = null;
		pendingFlowActivityId = null;
	}

	function captureFlowUrlRestoreTargets(): void {
		const params = page.url.searchParams;
		const rawAt = params.get('flowAt');
		const parsedAt = rawAt === null ? Number.NaN : Number(rawAt);
		pendingFlowAt = Number.isFinite(parsedAt) && Number.isInteger(parsedAt) ? parsedAt : null;
		pendingFlowActorId = params.get('flowActor');
		pendingFlowActivityId = params.get('flowActivity');
		flowRestorePending =
			pendingFlowAt !== null || pendingFlowActorId !== null || pendingFlowActivityId !== null;
	}

	function openFlowSession(sid: string): void {
		resetFlowRouteState();
		captureFlowUrlRestoreTargets();
		void flowStore.load(sid);
		flowPlayback = createFlowPlayback({
			dataset: flowStore.dataset,
			initialMode: 'paused',
			initialAt: 'end'
		});
		flowPlayback.setSpeed(30);
		flowLiveDecided = pendingFlowAt !== null;
	}

	function ensureFlowLoaded(): void {
		flowLoadTriggered = true;
	}

	$effect(() => {
		const params = page.url.searchParams;
		const wantsFlow =
			params.get('view') === 'flow' ||
			params.has('flowActor') ||
			params.has('flowActivity') ||
			params.has('flowAt');
		if (wantsFlow) untrack(ensureFlowLoaded);
	});

	$effect(() => {
		const el = flowSectionEl;
		if (!el || flowLoadTriggered) return;
		if (typeof IntersectionObserver === 'undefined') {
			ensureFlowLoaded();
			return;
		}
		const observer = new IntersectionObserver(
			(entries) => {
				if (entries.some((entry) => entry.isIntersecting)) ensureFlowLoaded();
			},
			{ rootMargin: '100% 0px' }
		);
		observer.observe(el);
		return () => observer.disconnect();
	});

	$effect(() => {
		const triggered = flowLoadTriggered;
		const sid = sessionId;
		untrack(() => {
			if (!triggered || !sid) {
				if (flowPlayback !== null || flowStore.activeSessionId !== null) {
					resetFlowRouteState();
					flowStore.deactivate();
				}
				return;
			}
			if (FlowGraphComponent === null) {
				void import('$lib/components/session-flow/SessionFlow.svelte').then((mod) => {
					if (flowLoadTriggered && sessionId === sid) FlowGraphComponent = mod.default;
				});
			}
			if (flowStore.activeSessionId !== sid) openFlowSession(sid);
		});
	});

	$effect(() => {
		const playback = flowPlayback;
		const dataset = flowStore.dataset;
		const loading = flowStore.loading;
		if (playback === null || dataset.sessionId !== sessionId) return;
		untrack(() => {
			playback.setDataset(dataset);
			if (!flowRestorePending) return;
			let resolved = false;
			if (pendingFlowActivityId !== null && dataset.activities.has(pendingFlowActivityId)) {
				flowSelection = { kind: 'activity', id: pendingFlowActivityId };
				resolved = true;
			} else if (pendingFlowActorId !== null && dataset.actors.has(pendingFlowActorId)) {
				flowSelection = { kind: 'actor', id: pendingFlowActorId };
				resolved = true;
			}
			if (pendingFlowAt !== null && dataset.bounds !== null) {
				if (playback.seek(pendingFlowAt).status === 'positioned') resolved = true;
			}
			if (resolved || !loading) flowRestorePending = false;
		});
	});

	$effect(() => {
		const requestId = pendingFlowRequestId;
		const index = requestActivityIndex;
		const loading = flowStore.loading;
		if (requestId === null) return;
		untrack(() => {
			const activityId = activityIdForRequest(index, requestId);
			if (activityId !== null) {
				pendingFlowRequestId = null;
				handleFlowSelect({ kind: 'activity', id: activityId });
				scrollFlowIntoView();
			} else if (!loading) {
				pendingFlowRequestId = null;
			}
		});
	});

	function flowUrlWith(
		overrides: Partial<Record<'flowActor' | 'flowActivity' | 'flowAt', string | null>>
	): string {
		const params = new URLSearchParams(page.url.search);
		for (const [key, value] of Object.entries(overrides)) {
			if (value === null) params.delete(key);
			else if (value !== undefined) params.set(key, value);
		}
		const query = params.toString();
		return query ? `${page.url.pathname}?${query}` : page.url.pathname;
	}

	function syncFlowUrl(
		overrides: Partial<Record<'flowActor' | 'flowActivity' | 'flowAt', string | null>>
	): void {
		const target = flowUrlWith(overrides);
		if (target === page.url.pathname + page.url.search) return;
		// eslint-disable-next-line svelte/no-navigation-without-resolve -- rewriting the current page's own already-resolved pathname + a modified query string, matching RequestDrawer's identical `?drawer=` sync
		void goto(target, { replace: true, reset: false });
	}

	function scrollFlowIntoView(): void {
		if (!flowSectionEl) return;
		flowSectionEl.scrollIntoView({ behavior: 'smooth', block: 'nearest' });
	}

	function handleRequestActivate(requestId: number): void {
		ensureFlowLoaded();
		const activityId = activityIdForRequest(requestActivityIndex, requestId);
		if (activityId !== null) {
			handleFlowSelect({ kind: 'activity', id: activityId });
			scrollFlowIntoView();
		} else {
			pendingFlowRequestId = requestId;
		}
	}

	function handleFlowSelect(next: FlowSelection): void {
		flowSelection = next;
		if (next !== null && next.kind === 'actor')
			syncFlowUrl({ flowActor: next.id, flowActivity: null });
		else if (next !== null && next.kind === 'activity')
			syncFlowUrl({ flowActivity: next.id, flowActor: null });
		else syncFlowUrl({ flowActor: null, flowActivity: null });
	}

	function handleFlowSeek(atMs: number, options?: FlowSeekOptions): void {
		const result = flowPlayback?.seek(atMs, options);
		if (result?.status === 'positioned') syncFlowUrl({ flowAt: String(Math.round(result.atMs)) });
	}

	function handleFlowFollow(id: string | null): void {
		flowFollowId = id;
	}

	function handleFlowCollapse(id: string): void {
		const next = new Set(flowCollapsedIds);
		if (next.has(id)) next.delete(id);
		else next.add(id);
		flowCollapsedIds = next;
	}

	function handleFlowBrush(range: FlowBounds | null): void {
		flowBrush = range;
	}

	function handleFlowOpenDrawer(requestId: string, kind: 'request' | 'response'): void {
		const id = Number(requestId);
		if (Number.isFinite(id)) openDrawer(id, kind);
	}

	function handleFlowRetry(): void {
		void flowStore.retry();
	}

	const flowRequestFilter = $derived(buildFlowRequestFilter(flowStore.dataset, flowSelection));
	const totalRequestCount = $derived(detailStore.detail?.total_requests ?? detailStore.detail?.requests.length ?? 0);

	function clearRequestFilter(): void {
		handleFlowSelect(null);
	}
</script>

<svelte:head>
	<title>Session {shortSessionId(page.params.sessionId ?? '')} — mike</title>
</svelte:head>

<section class="col detail session-page" aria-label="Session detail" bind:this={pageScrollEl}>
	<header class="session-header">
		<span class="session-back-backing">
			<a
				href={resolve('/sessions')}
				class="session-back"
				data-augmented-ui="tr-clip bl-clip border"
				aria-label="Back to sessions"
			>
				<ArrowLeft size={14} aria-hidden="true" />
			</a>
		</span>
		<div class="session-header__titles">
			<div class="session-title-row">
				<h2 class="session-title">{sessionTitle}</h2>
			</div>
			<span class="session-subtitle">
				{#if project}<a
						href={resolve(`projects/${encodeURIComponent(project)}`)}
						class="session-subtitle__project">{project}</a
					><span class="session-subtitle__sep" aria-hidden="true">·</span>{/if}
				{shortSessionId(sessionId)}
				<CopyIconButton
					getText={() => sessionId || null}
					label="Copy full session id"
					copiedLabel="Copied"
					size={12}
				/>
			</span>
		</div>
		<Tooltip text="Comms — cross-session message threads for this session">
			{#snippet child({ props })}
				<a {...props} href={commsHref} class="session-comms-link" aria-label="Comms">
					<MessagesSquare size={16} aria-hidden="true" />
				</a>
			{/snippet}
		</Tooltip>
		<PinToggle
			{pinned}
			onToggle={handleTogglePin}
			size={16}
			ariaLabel={pinned ? 'Unpin session' : 'Pin session'}
		/>
	</header>
	<SessionOverview
		{now}
		{promptKindCounts}
		validation={validationState}
		validationSince={validationStatusDto?.dispatched_at ?? null}
	/>
	<div class="flow-section" data-slot="flow-section" bind:this={flowSectionEl}>
		{#if FlowGraphComponent && flowPlayback && flowScene}
			<FlowGraphComponent
				dataset={flowStore.dataset}
				scene={flowScene}
				playback={flowPlayback}
				selection={flowSelection}
				availability={flowStore.availability}
				loading={flowStore.loading}
				error={flowStore.error}
				stale={flowStore.stale}
				staleReasons={flowStore.staleReasons}
				followId={flowFollowId}
				collapsedIds={flowCollapsedIds}
				brush={flowBrush}
				overlay={flowStore.overlay}
				title={summary?.title ?? null}
				onSelect={handleFlowSelect}
				onSeek={handleFlowSeek}
				onFollow={handleFlowFollow}
				onCollapse={handleFlowCollapse}
				onBrush={handleFlowBrush}
				onOpenDrawer={handleFlowOpenDrawer}
				onRetry={handleFlowRetry}
			/>
		{:else}
			<div class="edge flow-placeholder">
				<p class="flow-loading-hint">
					{flowLoadTriggered
						? 'Loading Session Flow...'
						: 'Session Flow loads once this section scrolls into view.'}
				</p>
			</div>
		{/if}
	</div>

	<div class="edge requests-card" data-slot="requests">
		<div class="requests-card__head">
			<h3 class="requests-card__hd">Requests</h3>
			{#if flowRequestFilter}
				<span class="requests-filter-backing">
					<span class="requests-filter" data-augmented-ui="tl-clip br-clip border">
						<span class="requests-filter__label">{flowRequestFilter.label}</span>
						<span class="requests-filter__count"
							>{flowRequestFilter.requestIds.size} of {totalRequestCount}</span
						>
						<button
							type="button"
							class="requests-filter__clear"
							aria-label="Show all requests"
							onclick={clearRequestFilter}
						>
							<X size={10} aria-hidden="true" />
						</button>
					</span>
				</span>
			{/if}
		</div>
		<SessionTimeline
			{detailStore}
			{now}
			scrollContainer={pageScrollEl}
			onRequestActivate={handleRequestActivate}
			requestIdFilter={flowRequestFilter?.requestIds ?? null}
		/>
	</div>

	<RequestDrawer />
</section>

<style>
	.session-page {
		display: flex;
		flex-direction: column;
	}

	.session-header {
		display: flex;
		align-items: center;
		gap: 12px;
		padding: 12px 24px;
		flex-shrink: 0;
	}

	.session-back-backing {
		display: inline-flex;
		flex-shrink: 0;
		background: var(--surface-bg);
	}

	.session-back {
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
		transition: color var(--dur-1) var(--ease-snap);
	}

	.session-back:hover {
		--aug-border-bg: var(--border-default);
		color: var(--text-strong);
	}

	.session-back:focus-visible {
		outline: 2px solid var(--interactive-focus-ring);
		outline-offset: 2px;
	}

	.session-header__titles {
		display: flex;
		flex-direction: column;
		gap: 2px;
		flex: 1;
		min-width: 0;
	}

	.session-title-row {
		display: flex;
		align-items: center;
		flex-wrap: wrap;
		gap: 8px;
		min-width: 0;
	}

	.session-title {
		margin: 0;
		font-family: var(--font-display);
		font-size: 1.05rem;
		font-weight: 700;
		letter-spacing: -0.01em;
		color: var(--text-1);
		min-width: 0;
		overflow-wrap: anywhere;
	}

	.session-subtitle__project {
		color: var(--text-2);
		text-decoration: none;
		min-width: 0;
		overflow-wrap: anywhere;
	}
	.session-subtitle__project:hover {
		color: var(--text-strong);
		text-decoration: underline;
	}
	.session-subtitle__project:focus-visible {
		outline: none;
		box-shadow: 0 0 0 2px var(--interactive-focus-ring);
		border-radius: 2px;
	}

	.session-subtitle__sep {
		color: var(--text-3);
	}

	.session-subtitle {
		display: inline-flex;
		flex-wrap: wrap;
		align-items: center;
		min-width: 0;
		gap: 4px;
		font-family: var(--font-mono, monospace);
		font-size: 11px;
		color: var(--text-3);
	}

	.session-comms-link {
		display: inline-flex;
		flex-shrink: 0;
		align-items: center;
		justify-content: center;
		width: 24px;
		height: 24px;
		border-radius: 2px;
		color: var(--text-3);
		transition:
			color var(--dur-1) ease,
			background-color var(--dur-1) ease;
	}
	.session-comms-link:hover {
		background: var(--surface-card);
		color: var(--text-strong);
	}
	.session-comms-link:focus-visible {
		outline: none;
		box-shadow: 0 0 0 2px var(--ring);
	}

	.session-page :global(.s-overview),
	.session-page :global(.s-scroll) {
		flex: 0 0 auto;
		min-height: unset;
		overflow: visible;
	}

	.requests-card {
		--edge: var(--edge-sm);
		margin-top: var(--space-4, 16px);
		background: var(--surface-card);
	}

	.requests-card__head {
		display: flex;
		align-items: center;
		gap: var(--space-3, 12px);
		margin: 0 0 var(--space-3, 12px);
	}

	.requests-card__hd {
		margin: 0;
		font-family: var(--font-display, 'Syne Variable', sans-serif);
		font-size: 11px;
		font-weight: 800;
		text-transform: uppercase;
		letter-spacing: 0.07em;
		color: var(--text-2);
	}

	.requests-filter-backing {
		display: inline-flex;
		background: var(--surface-card);
	}
	.requests-filter {
		--edge: 4px;
		--aug-tl: var(--edge);
		--aug-br: var(--edge);
		--aug-border-all: 1px;
		--aug-border-bg: var(--data-teal-border);
		display: inline-flex;
		align-items: center;
		gap: 8px;
		height: 22px;
		padding: 0 4px 0 8px;
		background: var(--data-teal-bg);
		font-family: var(--font-mono);
		font-size: 10.5px;
	}
	.requests-filter__label {
		color: var(--data-teal-fg);
		font-weight: 600;
	}
	.requests-filter__count {
		color: var(--text-3);
		font-variant-numeric: tabular-nums;
	}
	.requests-filter__clear {
		display: inline-flex;
		align-items: center;
		justify-content: center;
		width: 18px;
		height: 18px;
		border: 0;
		background: transparent;
		color: var(--text-2);
		cursor: pointer;
	}
	.requests-filter__clear:hover {
		color: var(--text-strong);
	}
	.requests-filter__clear:focus-visible {
		outline: 2px solid var(--interactive-focus-ring);
		outline-offset: 2px;
	}

	.flow-section {
		margin-top: var(--space-4, 16px);
	}

	.flow-placeholder {
		--edge: var(--edge-sm);
		background: var(--surface-card);
	}

	.flow-loading-hint {
		margin: 0;
		font-size: 12px;
		color: var(--text-3);
		font-style: italic;
	}
</style>
