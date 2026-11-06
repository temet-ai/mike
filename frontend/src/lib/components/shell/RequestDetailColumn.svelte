<script lang="ts">
	import { onMount, untrack, getContext } from 'svelte';
	import { page } from '$app/state';
	import { resolve } from '$app/paths';
	import { MAX_EVENTS, proxyEvents } from '$lib/ws.svelte';
	import { createRequestDetailStore } from '$lib/stores/cache/requestDetailStore.svelte';
	import type { RequestDetailStore } from '$lib/stores/cache/requestDetailStore.svelte';
	import { SESSION_DETAIL_STORE_KEY, SESSION_LIST_STORE_KEY } from '$lib/context-keys';
	import type { SessionListStore } from '$lib/stores/sessions.svelte';
	import type { SessionDetailStore } from '$lib/stores/cache/sessionDetailStore.svelte';
	import { shellState } from '$lib/stores/shellState.svelte';
	import LoadErrorBanner from '$lib/components/shared/LoadErrorBanner.svelte';
	import SessionTimeline from '$lib/components/shell/SessionTimeline.svelte';
	import RequestDrawer from '$lib/components/shell/RequestDrawer.svelte';
	import { formatRelative, formatDuration } from '$lib/time';
	import PinToggle from '$lib/components/shared/PinToggle.svelte';
	import Tooltip from '$lib/components/shared/Tooltip.svelte';

	let store = $state<RequestDetailStore | null>(null);
	let processedEvents = $state(0);
	let processedLagged = $state(0);
	let now = $state(Date.now());

	let detailBodyEl = $state<HTMLElement | undefined>(undefined);

	const sessionDetailStore = getContext<SessionDetailStore>(SESSION_DETAIL_STORE_KEY);

	const sessionId = $derived(page.params.sessionId ?? '');
	const currentRequestId = $derived.by(() => {
		const raw = page.params.requestId;
		const parsed = raw ? Number(raw) : Number.NaN;
		return Number.isFinite(parsed) ? parsed : null;
	});

	const sessionListStore = getContext<SessionListStore | undefined>(SESSION_LIST_STORE_KEY);
	const sessionSummary = $derived.by(() => {
		if (!sessionListStore) return null;
		const sid = sessionId;
		if (!sid) return null;
		return sessionListStore.sessions.find((sess) => sess.id === sid) ?? null;
	});
	const sessionTitle = $derived(sessionSummary?.title ?? null);
	const sessionPinned = $derived(sessionSummary?.pinned === true);

	const sessionRequestCount = $derived(sessionSummary?.request_count ?? null);
	const sessionStartedRel = $derived.by(() => {
		const fs = sessionSummary?.first_seen ?? null;
		if (!fs) return null;
		return formatRelative(fs, now);
	});
	const sessionSpanLabel = $derived.by(() => {
		const fs = sessionSummary?.first_seen ?? null;
		const ls = sessionSummary?.last_seen ?? null;
		if (!fs || !ls) return null;
		const ms = new Date(ls).getTime() - new Date(fs).getTime();
		if (!Number.isFinite(ms) || ms < 1000) return null;
		return formatDuration(ms);
	});
	const sessionModels = $derived.by(() => {
		const ms = sessionSummary?.models ?? [];
		const distinct = Array.from(new Set(ms.filter((m) => m.length > 0)));
		return distinct.length > 1 ? distinct : null;
	});

	function handleHeaderTogglePin(): void {
		const sid = sessionId;
		if (!sid || !sessionListStore) return;
		void sessionListStore.togglePin(sid, !sessionPinned);
	}

	$effect(() => {
		const raw = page.params.requestId;
		untrack(() => {
			const parsed = raw ? Number(raw) : Number.NaN;
			if (!Number.isFinite(parsed)) {
				store = null;
				return;
			}
			processedEvents = proxyEvents.totalEvents;
			store = createRequestDetailStore(parsed);
		});
	});

	$effect(() => {
		const sid = sessionId;
		untrack(() => {
			if (!sid) return;
			void sessionDetailStore.load(sid);
		});
	});

	onMount(() => {
		const timer = setInterval(() => (now = Date.now()), 1_000);
		return () => clearInterval(timer);
	});

	$effect(() => {
		if (shellState.paused) return;
		const total = proxyEvents.totalEvents;
		const active = store;
		if (!active || total === processedEvents) return;
		const start = total < processedEvents ? 0 : Math.max(processedEvents, total - MAX_EVENTS);
		for (let i = start; i < total; i++) {
			const ev = proxyEvents.at(i);
			if (ev) active.handleEvent(ev);
		}
		processedEvents = total;
	});

	$effect(() => {
		if (shellState.paused) return;
		const lagged = proxyEvents.laggedCount;
		const active = store;
		if (!active || lagged === processedLagged) return;
		const delta = lagged - processedLagged;
		processedLagged = lagged;
		if (delta > 0) active.handleEvent({ type: 'Lagged', skipped: delta });
	});

	const detail = $derived(
		store?.detail ?? sessionDetailStore.details.get(currentRequestId ?? -1) ?? null
	);
	const err = $derived(store?.error ?? null);
	const showErr = $derived(!!err && !detail);
	function retry(): void {
		store?.refresh();
	}

	const hasRequestSelected = $derived(currentRequestId !== null);
</script>

<section class="col detail" aria-label="Request detail">
	{#if !hasRequestSelected}
		<div class="detail-head">
			<div class="path">Select a request</div>
			<div class="sub">
				<span>Pick a row in the Requests column to inspect its messages, body, and findings.</span>
			</div>
		</div>
	{:else}
		<div class="detail-head">
			<div class="path">
				<Tooltip text={sessionId}>
					{#snippet child({ props })}
						<a
							{...props}
							href={resolve(`/sessions/${encodeURIComponent(sessionId)}`)}
							class="text-inherit hover:underline"
						>
							session {sessionId.slice(0, 12)}
						</a>
					{/snippet}
				</Tooltip>
				{#if sessionTitle}
					<span class="dot-sep" aria-hidden="true">—</span>
					<Tooltip text={sessionTitle}>
						<span class="session-title">{sessionTitle}</span>
					</Tooltip>
				{/if}
				{#if sessionSummary}
					<span class="detail-head-pin">
						<PinToggle pinned={sessionPinned} onToggle={handleHeaderTogglePin} />
					</span>
				{/if}
			</div>
			<div class="sub">
				{#if sessionSummary}
					{#if sessionRequestCount !== null}
						<Tooltip text="Total requests captured under this session">
							<span><b>{sessionRequestCount}</b> requests</span>
						</Tooltip>
					{/if}
					{#if sessionStartedRel}
						<span>·</span>
						<span>
							started
							<Tooltip text={sessionSummary.first_seen}>
								<b>{sessionStartedRel}</b>
							</Tooltip>
							{#if sessionSpanLabel}
								<span class="text-[var(--text-faint)]">
									(spans {sessionSpanLabel})
								</span>
							{/if}
						</span>
					{/if}
					{#if sessionModels}
						<span>·</span>
						<Tooltip text="Distinct models used across this session">
							<span class="font-mono">{sessionModels.join(', ')}</span>
						</Tooltip>
					{/if}
				{:else}
					<span class="text-[var(--text-faint)]">loading session</span>
				{/if}
			</div>
		</div>

		{#if showErr}
			<div class="p-6">
				<LoadErrorBanner
					title="Couldn't load this request"
					detail={err ?? ''}
					hint="The request may have been evicted by retention, or the inspect API may be unreachable. Pinned sessions keep their requests indefinitely."
					onRetry={retry}
					backHref={resolve(`/sessions/${encodeURIComponent(sessionId)}`)}
					backLabel="Back to this session"
				/>
			</div>
		{:else}
			<div bind:this={detailBodyEl} class="detail-body flex-1 min-h-0 flex flex-col">
				<SessionTimeline detailStore={sessionDetailStore} {now} scrollContainer={detailBodyEl} />
			</div>

			<RequestDrawer />
		{/if}
	{/if}
</section>

<style>
	:global(.detail-head .path) {
		display: flex;
		flex-wrap: wrap;
		align-items: baseline;
		gap: 8px;
	}
	:global(.detail-head .path .session-title) {
		color: var(--text-2);
		font-weight: 400;
		font-family: var(--font-mono);
		font-size: var(--text-md);
		letter-spacing: 0;
		min-width: 0;
		white-space: normal;
		overflow-wrap: anywhere;
		word-break: break-word;
	}
	:global(.detail-head .path .dot-sep) {
		color: var(--text-faint);
		font-weight: 400;
	}
	:global(.detail-head .path .detail-head-pin) {
		display: inline-flex;
		align-items: center;
	}
</style>
