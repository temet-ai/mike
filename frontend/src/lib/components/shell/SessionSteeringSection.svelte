<script lang="ts">
	import { untrack, getContext } from 'svelte';
	import { page } from '$app/state';
	import { resolve } from '$app/paths';
	import {
		getSessionSteeringPage,
		getSessionEnrolments,
		listSteerers,
		enrolSession,
		disenrolSession,
		getSteeringVocabulary
	} from '$lib/api/steering';
	import type {
		SessionSteeringRow,
		SessionEnrolmentRow,
		SteeringVocabulary
	} from '$lib/api/steering';
	import { ApiError } from '$lib/api';
	import type { SteererDto, EnrolSessionBody } from '$lib/types';
	import { AUTHORITY_LABELS, STEERING_MODE_LABELS } from '$lib/steering';
	import type { Authority, SteeringMode } from '$lib/steering';
	import { proxyEvents } from '$lib/ws.svelte';
	import LoadErrorBanner from '$lib/components/shared/LoadErrorBanner.svelte';
	import InterventionRow from './InterventionRow.svelte';
	import { UserPlus, UserMinus, ChevronDown, ChevronUp } from '@lucide/svelte';
	import { COLLAPSE_STORE_KEY } from '$lib/context-keys';
	import type { CollapseStore } from '$lib/stores/collapseState.svelte';
	import DisclosureMarker from '$lib/components/shared/DisclosureMarker.svelte';

	interface Props {
		sessionId: string;
		now: number;
		embedded?: boolean;
	}

	let { sessionId, now: _now, embedded = false }: Props = $props();

	let rows = $state<SessionSteeringRow[]>([]);
	let totalCount = $state(0);
	let loadingMore = $state(false);
	let loadMoreError = $state<string | null>(null);
	let enrolments = $state<SessionEnrolmentRow[]>([]);
	let enrolLoaded = $state(false);
	let loading = $state(false);
	let error = $state<string | null>(null);
	let loaded = $state(false);
	let activityExpanded = $state(false);
	let reloadKey = $state(0);
	let ivLoadedForKey = -1;

	let vocabulary = $state<SteeringVocabulary | null>(null);
	$effect(() => {
		getSteeringVocabulary()
			.then((v) => {
				vocabulary = v;
			})
			.catch(() => {});
	});

	let enrolPanelOpen = $state(false);
	let availableSteerers = $state<SteererDto[]>([]);
	let steerersLoading = $state(false);
	let steerersError = $state<string | null>(null);
	let selectedSteererId = $state<number | null>(null);
	let enrolAuthority = $state<Authority>('supply_context');
	let enrolMode = $state<SteeringMode>('augment');
	let enrolling = $state(false);
	let enrolError = $state<string | null>(null);
	let disenrollingId = $state<number | null>(null);
	let disenrolError = $state<string | null>(null);

	async function openEnrolPanel(): Promise<void> {
		enrolPanelOpen = !enrolPanelOpen;
		if (!enrolPanelOpen) return;
		if (availableSteerers.length > 0) return;
		steerersLoading = true;
		steerersError = null;
		try {
			const all = await listSteerers();
			availableSteerers = all.filter((s) => s.enabled);
			if (availableSteerers.length > 0 && selectedSteererId === null) {
				selectedSteererId = availableSteerers[0]!.id;
			}
		} catch (err) {
			steerersError = err instanceof ApiError ? err.message : String(err);
		} finally {
			steerersLoading = false;
		}
	}

	async function handleEnrol(): Promise<void> {
		if (enrolling || selectedSteererId === null) return;
		enrolling = true;
		enrolError = null;
		const body: EnrolSessionBody = {
			steerer_id: selectedSteererId,
			authority: enrolAuthority,
			mode: enrolMode,
			enrolled_by: 'ui'
		};
		try {
			await enrolSession(sessionId, body);
			enrolPanelOpen = false;
			reloadKey += 1;
		} catch (err) {
			enrolError = err instanceof ApiError ? err.message : String(err);
		} finally {
			enrolling = false;
		}
	}

	async function handleDisenrol(enrolmentId: number): Promise<void> {
		if (disenrollingId !== null) return;
		disenrollingId = enrolmentId;
		disenrolError = null;
		try {
			await disenrolSession(sessionId, enrolmentId);
			reloadKey += 1;
		} catch (err) {
			disenrolError = err instanceof ApiError ? err.message : String(err);
		} finally {
			disenrollingId = null;
		}
	}

	function retry(): void {
		reloadKey += 1;
	}

	let prevSid = '';
	$effect(() => {
		const sid = sessionId;
		untrack(() => {
			if (sid === prevSid) return;
			prevSid = sid;
			rows = [];
			totalCount = 0;
			loadingMore = false;
			loadMoreError = null;
			loaded = false;
			error = null;
			activityExpanded = false;
			ivLoadedForKey = -1;
		});
	});

	$effect(() => {
		const sid = sessionId;
		void reloadKey;
		untrack(() => {
			if (!sid) return;
			getSessionEnrolments(sid)
				.then((enrolRows) => {
					if (sessionId !== sid) return;
					enrolments = enrolRows;
					enrolLoaded = true;
				})
				.catch(() => {
					if (sessionId !== sid) return;
					enrolments = [];
					enrolLoaded = true;
				});
		});
	});

	const STEERING_PAGE_SIZE = 500;

	$effect(() => {
		const sid = sessionId;
		const expanded = activityExpanded;
		const key = reloadKey;
		untrack(() => {
			if (!sid || !expanded) return;
			if (ivLoadedForKey === key && loaded) return;
			ivLoadedForKey = key;
			loading = true;
			error = null;
			loadMoreError = null;
			getSessionSteeringPage(sid, { limit: STEERING_PAGE_SIZE })
				.then((envelope) => {
					if (sessionId !== sid) return;
					rows = envelope.rows;
					totalCount = envelope.total_count;
					loaded = true;
				})
				.catch((err) => {
					if (sessionId !== sid) return;
					error = err instanceof ApiError ? err.message : String(err);
					rows = [];
					totalCount = 0;
					loaded = true;
				})
				.finally(() => {
					if (sessionId !== sid) return;
					loading = false;
				});
		});
	});

	async function loadMoreInterventions(): Promise<void> {
		const sid = sessionId;
		if (!sid || loadingMore) return;
		loadingMore = true;
		loadMoreError = null;
		try {
			const envelope = await getSessionSteeringPage(sid, {
				offset: rows.length,
				limit: STEERING_PAGE_SIZE
			});
			if (sessionId !== sid) return;
			rows = [...rows, ...envelope.rows];
			totalCount = envelope.total_count;
		} catch (err) {
			if (sessionId !== sid) return;
			loadMoreError = err instanceof ApiError ? err.message : String(err);
		} finally {
			if (sessionId === sid) loadingMore = false;
		}
	}

	let refetchTimer: ReturnType<typeof setTimeout> | null = null;
	let processedEvents = 0;

	$effect(() => {
		const total = proxyEvents.totalEvents;
		untrack(() => {
			if (total === processedEvents) return;
			const start = Math.max(processedEvents, total - 200);
			for (let i = start; i < total; i++) {
				const ev = proxyEvents.at(i);
				if (!ev) continue;
				if (
					ev.type === 'RowChanged' &&
					ev.table === 'interventions' &&
					(ev.session_id === null || ev.session_id === sessionId)
				) {
					if (refetchTimer) clearTimeout(refetchTimer);
					refetchTimer = setTimeout(() => {
						refetchTimer = null;
						reloadKey += 1;
					}, 800);
					break;
				}
			}
			processedEvents = total;
		});
	});

	const activeEnrolments = $derived(enrolments.filter((e) => e.disenrolled_at === null));
	const firedCount = $derived(rows.filter((r) => r.verdict === 'fired').length);
	const otherCount = $derived(rows.length - firedCount);
	const remaining = $derived(Math.max(0, totalCount - rows.length));

	const countLabel = $derived.by(() => {
		if (rows.length === 0) return '0 interventions';
		const parts: string[] = [];
		if (firedCount > 0) parts.push(`${firedCount} fired`);
		if (otherCount > 0) parts.push(`${otherCount} other evaluation${otherCount === 1 ? '' : 's'}`);
		let label = parts.join(' · ');
		if (remaining > 0) label += ` (${remaining} more not loaded)`;
		return label;
	});

	const suppress = $derived(enrolLoaded && enrolments.length === 0);

	const sessionBasePath = $derived(
		`/sessions/${encodeURIComponent(page.params.sessionId ?? sessionId)}`
	);

	const collapseStore = getContext<CollapseStore | undefined>(COLLAPSE_STORE_KEY);
	let enrolExpandedLocal = $state<Record<number, boolean>>({});

	function enrolOpenKey(enrolmentId: number): string {
		return `session:${sessionId}:enrol:${enrolmentId}-open`;
	}

	function isEnrolExpanded(enrolmentId: number): boolean {
		if (collapseStore) return collapseStore.isExpanded(enrolOpenKey(enrolmentId), true);
		return enrolExpandedLocal[enrolmentId] ?? true;
	}

	function toggleEnrol(enrolmentId: number): void {
		if (collapseStore) {
			collapseStore.toggle(enrolOpenKey(enrolmentId));
			return;
		}
		enrolExpandedLocal = { ...enrolExpandedLocal, [enrolmentId]: !isEnrolExpanded(enrolmentId) };
	}

	const INITIAL_INTERVENTIONS = 20;
	let showAllInterventions = $state(false);
	const visibleRows = $derived(
		showAllInterventions || rows.length <= INITIAL_INTERVENTIONS
			? rows
			: rows.slice(0, INITIAL_INTERVENTIONS)
	);
	const hiddenInterventionCount = $derived(Math.max(0, rows.length - INITIAL_INTERVENTIONS));
</script>

{#if !suppress}
	<section
		class="sss-section"
		class:sss-section--embedded={embedded}
		aria-label="Steering activity"
	>
		<div class="sss-header">
			<span class="sss-title panel-subhd">Steering activity</span>
			{#if rows.length > 0}
				<span class="sss-count">{countLabel}</span>
			{/if}
			{#if activeEnrolments.length > 0}
				<span class="sss-enrol-chip-backing">
					<span class="sss-enrol-chip" data-augmented-ui="tl-clip br-clip border">
						{activeEnrolments.length === 1 ? '1 enrolled' : `${activeEnrolments.length} enrolled`}
					</span>
				</span>
			{/if}
			<span class="sss-enrol-btn-backing">
				<button
					type="button"
					class="sss-enrol-btn"
					data-augmented-ui="tr-clip bl-clip border"
					onclick={openEnrolPanel}
					aria-expanded={enrolPanelOpen}
					aria-label="Enrol session in a steerer"
					title="Enrol in a steerer"
				>
					<UserPlus size={11} aria-hidden="true" />
					{#if enrolPanelOpen}
						<ChevronUp size={10} aria-hidden="true" />
					{:else}
						<ChevronDown size={10} aria-hidden="true" />
					{/if}
				</button>
			</span>
		</div>

		{#if activeEnrolments.length > 0}
			<div class="sss-enrolments">
				{#each activeEnrolments as e (e.enrolment_id)}
					{@const enrolOpen = isEnrolExpanded(e.enrolment_id)}
					{@const enrolName = e.steerer_display_name ?? e.steerer_name ?? 'unknown'}
					<div class="sss-enrol-row" data-augmented-ui="tl-clip br-clip border">
						<button
							type="button"
							class="sss-enrol-toggle"
							onclick={() => toggleEnrol(e.enrolment_id)}
							aria-expanded={enrolOpen}
							aria-label="{enrolOpen ? 'Collapse' : 'Expand'} {enrolName}"
						>
							<DisclosureMarker open={enrolOpen} size={12} />
							<span class="sss-enrol-name entity-name">{enrolName}</span>
						</button>
						{#if enrolOpen}
							<div class="sss-enrol-details">
								{#if e.mode}
									<span class="sss-enrol-tag-backing">
										<span class="sss-enrol-tag" data-augmented-ui="tl-clip br-clip border"
											>{STEERING_MODE_LABELS[e.mode as SteeringMode] ?? e.mode}</span
										>
									</span>
								{/if}
								{#if e.authority}
									<span class="sss-enrol-tag-backing">
										<span class="sss-enrol-tag" data-augmented-ui="tl-clip br-clip border"
											>{AUTHORITY_LABELS[e.authority as Authority] ?? e.authority}</span
										>
									</span>
								{/if}
								<span class="sss-disenrol-btn-backing">
									<button
										type="button"
										class="sss-disenrol-btn"
										data-augmented-ui="tr-clip bl-clip border"
										disabled={disenrollingId === e.enrolment_id}
										onclick={() => handleDisenrol(e.enrolment_id)}
										aria-label="Disenrol from {enrolName}"
										title="Disenrol"
									>
										<UserMinus size={10} aria-hidden="true" />
										{disenrollingId === e.enrolment_id ? '…' : 'Disenrol'}
									</button>
								</span>
							</div>
						{/if}
					</div>
				{/each}
				{#if disenrolError}
					<p class="sss-inline-error" role="alert">{disenrolError}</p>
				{/if}
			</div>
		{/if}

		{#if enrolPanelOpen}
			<div
				class="sss-enrol-panel"
				data-augmented-ui="tl-clip br-clip border"
				role="region"
				aria-label="Enrol in steerer"
			>
				{#if steerersLoading}
					<p class="sss-panel-loading">Loading steerers…</p>
				{:else if steerersError}
					<p class="sss-panel-error" role="alert">{steerersError}</p>
				{:else if availableSteerers.length === 0}
					<p class="sss-panel-empty">
						No enabled steerers available.
						<a
							href={resolve(
								`steering/new?mode=guided&session=${sessionId}` as `steering/new?${string}`
							)}
							class="sss-panel-link"
						>
							Create one for this session
						</a>
					</p>
				{:else}
					<div class="sss-panel-fields">
						<div class="sss-panel-field">
							<label class="sss-panel-label" for="sss-steerer-select">Steerer</label>
							<select
								id="sss-steerer-select"
								class="sss-panel-select"
								bind:value={selectedSteererId}
							>
								{#each availableSteerers as s (s.id)}
									<option value={s.id}>{s.display_name}</option>
								{/each}
							</select>
						</div>
						<div class="sss-panel-field">
							<label class="sss-panel-label" for="sss-authority-select">Authority</label>
							<select
								id="sss-authority-select"
								class="sss-panel-select"
								bind:value={enrolAuthority}
							>
								{#each ['supply_context', 'redirect', 'command'] as const as a (a)}
									<option value={a}>{AUTHORITY_LABELS[a]}</option>
								{/each}
							</select>
						</div>
						<div class="sss-panel-field">
							<label class="sss-panel-label" for="sss-mode-select">Mode</label>
							<select id="sss-mode-select" class="sss-panel-select" bind:value={enrolMode}>
								{#each ['augment', 'drive'] as const as m (m)}
									<option value={m}>{STEERING_MODE_LABELS[m]}</option>
								{/each}
							</select>
						</div>
					</div>
					{#if enrolError}
						<p class="sss-inline-error" role="alert">{enrolError}</p>
					{/if}
					<div class="sss-panel-actions">
						<span class="sss-enrol-submit-backing">
							<button
								type="button"
								class="sss-enrol-submit"
								data-augmented-ui="tr-clip bl-clip border"
								disabled={enrolling || selectedSteererId === null}
								onclick={handleEnrol}
							>
								{enrolling ? 'Enrolling…' : 'Enrol session'}
							</button>
						</span>
						<span class="sss-enrol-cancel-backing">
							<button
								type="button"
								class="sss-enrol-cancel"
								data-augmented-ui="tr-clip bl-clip border"
								onclick={() => {
									enrolPanelOpen = false;
								}}
							>
								Cancel
							</button>
						</span>
						<span class="sss-panel-new-steerer-backing">
							<a
								href={resolve(
									`steering/new?mode=guided&session=${sessionId}` as `steering/new?${string}`
								)}
								class="sss-panel-new-steerer"
								data-augmented-ui="tr-clip bl-clip border"
							>
								New steerer for this session →
							</a>
						</span>
					</div>
				{/if}
			</div>
		{/if}

		{#if loading && !loaded}
			<div class="sss-skeleton" aria-hidden="true">
				<div class="sss-skel-row"></div>
				<div class="sss-skel-row sss-skel-row--short"></div>
				<div class="sss-skel-row"></div>
			</div>
		{/if}

		<button
			type="button"
			class="sss-activity-toggle"
			aria-expanded={activityExpanded}
			onclick={() => (activityExpanded = !activityExpanded)}
		>
			<DisclosureMarker open={activityExpanded} size={12} />
			<span class="sss-activity-label panel-subhd">Intervention timeline</span>
			{#if activityExpanded && loaded && rows.length > 0}
				<span class="sss-activity-hint">{countLabel}</span>
			{:else if !activityExpanded}
				<span class="sss-activity-hint">expand to load</span>
			{/if}
		</button>

		{#if activityExpanded}
			{#if loading}
				<p class="sss-loading" role="status">Loading interventions…</p>
			{:else if error}
				<div class="sss-error-wrap">
					<LoadErrorBanner
						title="Couldn't load steering data"
						detail={error}
						hint="The steering endpoint may not be live yet, or this session has no steering rows."
						onRetry={retry}
					/>
				</div>
			{:else if rows.length === 0}
				<p class="sss-empty">No interventions recorded for this session yet.</p>
			{:else}
				<div class="sss-list" role="list">
					{#each visibleRows as row (row.intervention_id)}
						<div role="listitem">
							<InterventionRow {row} {sessionBasePath} {vocabulary} />
						</div>
					{/each}
				</div>
				{#if !showAllInterventions && hiddenInterventionCount > 0}
					<span class="sss-show-all-btn-backing">
						<button
							type="button"
							class="sss-show-all-btn"
							data-augmented-ui="tr-clip bl-clip border"
							onclick={() => {
								showAllInterventions = true;
							}}
						>
							Show all {rows.length} interventions ({hiddenInterventionCount} more)
						</button>
					</span>
				{/if}
				{#if remaining > 0}
					<span class="sss-show-all-btn-backing">
						<button
							type="button"
							class="sss-show-all-btn"
							data-augmented-ui="tr-clip bl-clip border"
							disabled={loadingMore}
							onclick={loadMoreInterventions}
						>
							{loadingMore
								? 'Loading...'
								: `Load more interventions (${rows.length} of ${totalCount} loaded)`}
						</button>
					</span>
					{#if loadMoreError}
						<p class="sss-inline-error" role="alert">{loadMoreError}</p>
					{/if}
				{/if}
			{/if}
		{/if}
	</section>
{/if}

<style>
	.sss-section {
		display: flex;
		flex-direction: column;
		gap: 8px;
		padding: 16px 0 8px;
	}

	.sss-section--embedded {
		padding: 14px 16px;
	}

	.sss-header {
		display: flex;
		align-items: center;
		gap: 8px;
		flex-wrap: wrap;
	}

	.sss-count {
		font-family: var(--font-mono, monospace);
		font-size: 11px;
		color: var(--text-2);
	}

	.sss-enrol-chip-backing {
		display: inline-flex;
		flex-shrink: 0;
		background: var(--surface-card);
	}
	.sss-enrol-chip {
		--edge: 5px;
		--aug-tl: var(--edge);
		--aug-br: var(--edge);
		--aug-border-all: 1px;
		--aug-border-bg: var(--status-steering-border);
		display: inline-flex;
		align-items: center;
		padding: calc(var(--edge) + 1px) calc(var(--edge) + 3px);
		background: var(--status-steering-bg);
		font-family: var(--font-mono, monospace);
		font-size: 10px;
		font-weight: 600;
		color: var(--status-steering-fg);
		text-transform: uppercase;
		white-space: nowrap;
	}

	.sss-skeleton {
		display: flex;
		flex-direction: column;
		gap: 6px;
	}

	.sss-skel-row {
		height: 44px;
		border-radius: 2px;
		background: linear-gradient(
			90deg,
			var(--surface-2) 0%,
			var(--surface-shine) 50%,
			var(--surface-2) 100%
		);
		background-size: 200% 100%;
	}

	.sss-skel-row--short {
		height: 32px;
		opacity: 0.7;
	}

	.sss-skel-row {
		animation: sss-shimmer 1.6s ease-in-out infinite;
	}

	@keyframes sss-shimmer {
		0% {
			background-position: 200% 0;
		}
		100% {
			background-position: -200% 0;
		}
	}

	.sss-error-wrap {
		min-height: 0;
	}

	.sss-empty {
		margin: 0;
		font-family: var(--font-mono, monospace);
		font-size: 12px;
		color: var(--text-3);
		padding: 8px 0;
	}

	.sss-list {
		display: flex;
		flex-direction: column;
		gap: 4px;
	}

	.sss-show-all-btn-backing {
		align-self: flex-start;
		margin-top: 4px;
		display: inline-flex;
		background: var(--surface-card);
	}
	.sss-show-all-btn {
		--edge: 5px;
		--aug-tr: var(--edge);
		--aug-bl: var(--edge);
		--aug-border-all: 1px;
		--aug-border-bg: var(--border-subtle);
		padding: calc(var(--edge) + 2px) calc(var(--edge) + 5px);
		background: var(--surface-2);
		font-family: var(--font-mono, monospace);
		font-size: 11px;
		color: var(--text-2);
		cursor: pointer;
		transition: color var(--dur-1) var(--ease-snap);
	}

	.sss-show-all-btn:hover {
		color: var(--text-strong);
		--aug-border-bg: var(--border-default);
	}

	.sss-show-all-btn:focus-visible {
		outline: 2px solid var(--interactive-focus-ring);
		outline-offset: 2px;
	}

	.sss-enrol-btn-backing {
		display: inline-flex;
		margin-left: auto;
		flex-shrink: 0;
		background: var(--surface-card);
	}
	.sss-enrol-btn {
		--edge: 5px;
		--aug-tr: var(--edge);
		--aug-bl: var(--edge);
		--aug-border-all: 1px;
		--aug-border-bg: var(--border-subtle);
		display: inline-flex;
		align-items: center;
		gap: 3px;
		padding: calc(var(--edge) + 1px) calc(var(--edge) + 3px);
		background: transparent;
		font-family: var(--font-mono, monospace);
		font-size: 10px;
		color: var(--text-3);
		cursor: pointer;
	}

	.sss-enrol-btn:hover {
		--aug-border-bg: var(--status-steering-border);
		color: var(--status-steering-fg);
	}

	.sss-enrolments {
		display: flex;
		flex-direction: column;
		gap: 4px;
	}

	.sss-enrol-row {
		--edge: 6px;
		--aug-tl: var(--edge);
		--aug-br: var(--edge);
		--aug-border-all: 1px;
		--aug-border-bg: var(--status-steering-border);
		display: flex;
		flex-direction: column;
		background: var(--status-steering-bg);
	}

	.sss-enrol-toggle {
		display: flex;
		align-items: center;
		gap: 6px;
		width: 100%;
		padding: 4px 8px;
		border: none;
		background: transparent;
		text-align: left;
		cursor: pointer;
		font: inherit;
		color: inherit;
	}

	.sss-enrol-toggle:hover {
		background: color-mix(in srgb, var(--status-steering-bg) 60%, var(--surface-2) 40%);
	}

	.sss-enrol-toggle:focus-visible {
		outline: none;
		box-shadow: inset 0 0 0 2px var(--interactive-focus-ring);
	}

	.sss-activity-toggle {
		display: flex;
		align-items: center;
		gap: 6px;
		width: 100%;
		padding: 5px 8px;
		border: none;
		background: transparent;
		text-align: left;
		cursor: pointer;
		font: inherit;
		color: var(--text-2);
	}

	.sss-activity-toggle:hover {
		background: color-mix(in srgb, var(--status-steering-bg) 60%, var(--surface-2) 40%);
		color: var(--text-strong);
	}

	.sss-activity-toggle:focus-visible {
		outline: none;
		box-shadow: inset 0 0 0 2px var(--interactive-focus-ring);
	}

	.sss-activity-hint {
		font-size: 10px;
		color: var(--text-faint);
		font-variant-numeric: tabular-nums;
	}

	.sss-loading {
		margin: 0;
		padding: 8px 12px;
		font-size: 11px;
		color: var(--text-faint);
	}

	.sss-enrol-name {
		color: var(--status-steering-fg);
		flex: 1;
	}

	.sss-enrol-details {
		display: flex;
		align-items: center;
		flex-wrap: wrap;
		gap: 6px;
		padding: 0 8px 6px 26px;
	}

	.sss-enrol-tag-backing {
		display: inline-flex;
		background: var(--status-steering-bg);
	}
	.sss-enrol-tag {
		--edge: 3px;
		--aug-tl: var(--edge);
		--aug-br: var(--edge);
		--aug-border-all: 1px;
		--aug-border-bg: var(--border-subtle);
		font-family: var(--font-mono, monospace);
		font-size: 10px;
		color: var(--text-3);
		padding: calc(var(--edge) + 1px) calc(var(--edge) + 1px);
		background: var(--surface-card);
	}

	.sss-disenrol-btn-backing {
		display: inline-flex;
		background: var(--status-steering-bg);
	}
	.sss-disenrol-btn {
		--edge: 4px;
		--aug-tr: var(--edge);
		--aug-bl: var(--edge);
		--aug-border-all: 1px;
		--aug-border-bg: var(--status-danger-border);
		display: inline-flex;
		align-items: center;
		gap: 3px;
		padding: calc(var(--edge) + 1px) calc(var(--edge) + 3px);
		background: var(--status-danger-bg);
		font-family: var(--font-mono, monospace);
		font-size: 10px;
		font-weight: 600;
		color: var(--status-danger-fg);
		cursor: pointer;
	}

	.sss-disenrol-btn:hover {
		background: var(--status-danger-border);
	}

	.sss-disenrol-btn:focus-visible {
		outline: none;
		box-shadow: inset 0 0 0 2px var(--interactive-focus-ring);
	}

	.sss-disenrol-btn:disabled {
		opacity: 0.5;
		cursor: not-allowed;
	}

	.sss-enrol-panel {
		--edge: 8px;
		--aug-tl: var(--edge);
		--aug-br: var(--edge);
		--aug-border-all: 1px;
		--aug-border-bg: var(--status-steering-border);
		display: flex;
		flex-direction: column;
		gap: 10px;
		padding: calc(var(--edge) + 4px);
		background: var(--status-steering-bg);
	}

	.sss-panel-loading,
	.sss-panel-empty {
		margin: 0;
		font-family: var(--font-mono, monospace);
		font-size: 12px;
		color: var(--text-3);
	}

	.sss-panel-error {
		margin: 0;
		font-family: var(--font-mono, monospace);
		font-size: 12px;
		color: var(--status-danger-fg);
	}

	.sss-panel-link {
		color: var(--status-steering-fg);
		text-decoration: underline;
		text-underline-offset: 2px;
	}

	.sss-panel-new-steerer-backing {
		display: inline-flex;
		margin-left: auto;
		background: var(--status-steering-bg);
	}
	.sss-panel-new-steerer {
		--edge: 4px;
		--aug-tr: var(--edge);
		--aug-bl: var(--edge);
		--aug-border-all: 1px;
		--aug-border-bg: var(--border-subtle);
		display: inline-flex;
		align-items: center;
		padding: calc(var(--edge) + 2px) calc(var(--edge) + 5px);
		background: transparent;
		font-family: var(--font-mono, monospace);
		font-size: 11px;
		color: var(--text-2);
		text-decoration: none;
		white-space: nowrap;
	}

	.sss-panel-new-steerer:hover {
		color: var(--text-1);
	}

	.sss-panel-new-steerer:focus-visible {
		outline: none;
		box-shadow: inset 0 0 0 2px var(--interactive-focus-ring);
	}

	.sss-panel-fields {
		display: flex;
		flex-direction: column;
		gap: 8px;
	}

	.sss-panel-field {
		display: flex;
		flex-direction: column;
		gap: 3px;
	}

	.sss-panel-label {
		font-family: var(--font-mono, monospace);
		font-size: 10px;
		font-weight: 600;
		text-transform: uppercase;
		color: var(--text-3);
	}

	.sss-panel-select {
		padding: 4px 6px;
		border: 1px solid var(--border-subtle);
		border-radius: 2px;
		background: var(--surface-2);
		color: var(--text-1);
		font-family: var(--font-mono, monospace);
		font-size: 11px;
	}

	.sss-panel-actions {
		display: flex;
		align-items: center;
		gap: 8px;
	}

	.sss-enrol-submit-backing {
		display: inline-flex;
		background: var(--status-steering-bg);
	}
	.sss-enrol-submit {
		--edge: 4px;
		--aug-tr: var(--edge);
		--aug-bl: var(--edge);
		--aug-border-all: 1px;
		--aug-border-bg: var(--status-steering-border);
		padding: calc(var(--edge) + 2px) calc(var(--edge) + 5px);
		background: var(--status-steering-bg);
		font-family: var(--font-mono, monospace);
		font-size: 11px;
		font-weight: 600;
		color: var(--status-steering-fg);
		cursor: pointer;
	}

	.sss-enrol-submit:hover {
		background: var(--status-steering-border);
	}

	.sss-enrol-submit:disabled {
		opacity: 0.5;
		cursor: not-allowed;
	}

	.sss-enrol-cancel-backing {
		display: inline-flex;
		background: var(--status-steering-bg);
	}
	.sss-enrol-cancel {
		--edge: 4px;
		--aug-tr: var(--edge);
		--aug-bl: var(--edge);
		--aug-border-all: 1px;
		--aug-border-bg: var(--border-subtle);
		padding: calc(var(--edge) + 2px) calc(var(--edge) + 5px);
		background: transparent;
		font-family: var(--font-mono, monospace);
		font-size: 11px;
		color: var(--text-2);
		cursor: pointer;
	}

	.sss-enrol-cancel:hover {
		color: var(--text-1);
		--aug-border-bg: var(--border-default);
	}

	.sss-inline-error {
		margin: 0;
		font-family: var(--font-mono, monospace);
		font-size: 11px;
		color: var(--status-danger-fg);
	}
</style>
