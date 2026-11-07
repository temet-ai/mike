<script lang="ts">
	import { onMount, untrack } from 'svelte';
	import { goto } from '$app/navigation';
	import { page } from '$app/state';
	import { resolve } from '$app/paths';
	import { onApiCacheUpdate } from '$lib/stores/cache/apiCache.svelte';
	import { ApiError } from '$lib/api';
	import { listSessions } from '$lib/api';
	import { getSettings } from '$lib/api/settings';
	import { getFleetProjects } from '$lib/fleet/api';
	import type { FleetProjectSummary } from '$lib/fleet/types';
	import {
		getSteerer,
		patchSteerer,
		createSteerer,
		deleteSteerer,
		enrolSession,
		assistSteerer
	} from '$lib/api/steering';
	import type {
		SteererDto,
		SteererPatch,
		SteererCreate,
		SessionSummary,
		AssistSuggestion
	} from '$lib/types';
	import { AUTHORITY_LABELS, STEERING_MODE_LABELS } from '$lib/steering';
	import type { Authority, SteeringMode } from '$lib/steering';
	import LoadErrorBanner from '$lib/components/shared/LoadErrorBanner.svelte';
	import Tooltip from '$lib/components/shared/Tooltip.svelte';
	import HelpHint from '$lib/components/shared/HelpHint.svelte';
	import ModelPickerPopover from '$lib/components/settings/ModelPickerPopover.svelte';
	import { ArrowLeft, ExternalLink, Save, Trash2, HelpCircle, X } from '@lucide/svelte';

	const steererId = $derived(Number(page.params.id));
	const isNew = $derived(page.params.id === 'new');
	const isGuided = $derived(isNew && (page.url.searchParams.get('mode') ?? 'guided') === 'guided');

	const presetSessionId = $derived(
		isGuided ? (page.url.searchParams.get('session') ?? '').trim() : ''
	);

	const SESSION_UUID_RE = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i;

	let steerer = $state<SteererDto | null>(null);
	let error = $state<string | null>(null);
	let loading = $state(true);

	let displayName = $state('');
	let slug = $state('');
	let slugError = $state<string | null>(null);
	let description = $state('');
	let authority = $state<Authority>('supply_context');
	let mode = $state<SteeringMode>('augment');
	let model = $state('');
	let configuredDefaultModel = $state('');
	let prompt = $state('');
	let priority = $state(100);
	let enabled = $state(false);
	let confidenceThreshold = $state(0.7);
	let debounceTurns = $state(2);
	let triggerPoint = $state('response');
	let invocation = $state('every_turn');
	let pattern = $state('');
	let projectPath = $state('');

	let projects = $state<FleetProjectSummary[]>([]);
	let projectsError = $state<string | null>(null);

	async function loadProjects(): Promise<void> {
		try {
			projects = await getFleetProjects();
		} catch (err) {
			projectsError = err instanceof Error ? err.message : String(err);
		}
	}

	let saving = $state(false);
	let saveError = $state<string | null>(null);
	let saveWarnings = $state<string[]>([]);
	let saved = $state(false);
	let deleting = $state(false);
	let deleteError = $state<string | null>(null);
	let confirmDelete = $state(false);

	const PRESET_MODEL_IDS = [
		'openai/gpt-5.6-luna',
		'google/gemini-3.5-flash-lite',
		'google/gemini-3.8-flash'
	];

	let guidedStep = $state<1 | 2 | 3 | 4>(1);

	const GUIDED_STEPS: { n: 1 | 2 | 3 | 4; label: string }[] = [
		{ n: 1, label: 'Session' },
		{ n: 2, label: 'Context' },
		{ n: 3, label: 'Define' },
		{ n: 4, label: 'Confirm' }
	];

	function goToGuidedStep(n: 1 | 2 | 3 | 4): void {
		if (n < guidedStep) guidedStep = n;
	}

	let sessions = $state<SessionSummary[]>([]);
	let sessionsLoading = $state(false);
	let sessionsError = $state<string | null>(null);
	let sessionSearch = $state('');
	let selectedSessionId = $state<string | null>(null);
	let rawSessionId = $state('');

	const SESSION_SEARCH_MIN_QUERY = 2;
	const SESSION_SEARCH_DEBOUNCE_MS = 200;
	const SESSION_SEARCH_MAX_HITS = 50;

	let searchHits = $state<SessionSummary[]>([]);
	let searchLoading = $state(false);
	let searchError = $state<string | null>(null);
	let searchTimer: ReturnType<typeof setTimeout> | null = null;
	let searchSeq = 0;

	const searchQuery = $derived(sessionSearch.trim());
	const searchActive = $derived(searchQuery.length >= SESSION_SEARCH_MIN_QUERY);

	function clearSearchTimer(): void {
		if (searchTimer !== null) {
			clearTimeout(searchTimer);
			searchTimer = null;
		}
	}

	function resetSessionSearch(): void {
		clearSearchTimer();
		searchHits = [];
		searchLoading = false;
		searchError = null;
		searchSeq += 1;
	}

	async function runSessionSearch(query: string, seq: number): Promise<void> {
		try {
			const hits = await listSessions({ search: query, limit: SESSION_SEARCH_MAX_HITS });
			if (seq !== searchSeq) return;
			searchHits = hits.slice(0, SESSION_SEARCH_MAX_HITS);
			searchError = null;
		} catch (err) {
			if (seq !== searchSeq) return;
			searchError = err instanceof ApiError ? err.message : String(err);
			searchHits = [];
		} finally {
			if (seq === searchSeq) searchLoading = false;
		}
	}

	$effect(() => {
		const q = searchQuery;
		if (q.length < SESSION_SEARCH_MIN_QUERY) {
			untrack(() => resetSessionSearch());
			return;
		}
		untrack(() => {
			clearSearchTimer();
			searchTimer = setTimeout(() => {
				searchTimer = null;
				const seq = ++searchSeq;
				searchLoading = true;
				void runSessionSearch(q, seq);
			}, SESSION_SEARCH_DEBOUNCE_MS);
		});
	});

	interface SessionOverviewTurn {
		user_prompt?: string | null;
		stop_response?: string | null;
		created_at?: string;
	}
	interface SessionOverviewResponse {
		total_turns?: number;
		turns?: SessionOverviewTurn[];
	}

	interface TurnDigest {
		prompt: string;
		response: string;
		turn_number: number;
		created_at?: string;
	}

	const OVERVIEW_TURNS = 5;

	let overviewTurns = $state<TurnDigest[]>([]);
	let overviewTotal = $state(0);
	let overviewLoading = $state(false);
	let overviewError = $state<string | null>(null);

	let intentText = $state('');
	let assistState = $state<'idle' | 'thinking' | 'suggestion' | 'error'>('idle');
	let assistError = $state<string | null>(null);
	let assistSuggestion = $state<AssistSuggestion | null>(null);
	let assistModelUsed = $state<string | null>(null);
	let assistRationale = $state<Record<string, string>>({});
	let userChangedFields = $state(new Set<string>());

	const canAssist = $derived(intentText.trim().length > 0 || overviewTurns.length >= 2);

	let createError = $state<string | null>(null);
	let creating = $state(false);

	const autoSlug = $derived(
		displayName
			.toLowerCase()
			.replace(/[^a-z0-9]+/g, '-')
			.replace(/^-+|-+$/g, '')
			.slice(0, 48)
	);

	$effect(() => {
		if (slug === '' || slug === autoSlug) {
			slug = autoSlug;
		}
	});

	function validateSlug(s: string): string | null {
		if (!s) return 'Slug is required.';
		if (!/^[a-z0-9-]+$/.test(s))
			return 'Slug may only contain lowercase letters, digits, and hyphens.';
		return null;
	}

	async function loadSessions(): Promise<void> {
		if (sessions.length > 0) return;
		sessionsLoading = true;
		sessionsError = null;
		try {
			sessions = await listSessions({ limit: 50, order: 'desc' });
		} catch (err) {
			sessionsError = err instanceof ApiError ? err.message : String(err);
		} finally {
			sessionsLoading = false;
		}
	}

	const filteredSessions = $derived(searchActive ? searchHits : sessions);

	function shortId(id: string): string {
		return id.slice(0, 8);
	}

	function timeAgo(iso: string): string {
		const diff = Math.max(0, Math.floor((Date.now() - new Date(iso).getTime()) / 1000));
		if (diff < 60) return `${diff}s ago`;
		const m = Math.floor(diff / 60);
		if (m < 60) return `${m}m ago`;
		const h = Math.floor(m / 60);
		return `${h}h ago`;
	}

	function selectSession(id: string): void {
		selectedSessionId = id;
		rawSessionId = id;
	}

	async function loadOverview(sessionId: string): Promise<void> {
		overviewLoading = true;
		overviewError = null;
		overviewTurns = [];
		overviewTotal = 0;
		try {
			const res = await fetch(`/api/sessions/${sessionId}/overview?limit=${OVERVIEW_TURNS}`, {
				headers: { accept: 'application/json' }
			});
			if (!res.ok) throw new Error(`${res.status} ${res.statusText}`);
			const data: SessionOverviewResponse = await res.json();
			const total = data.total_turns ?? 0;
			overviewTurns = (data.turns ?? [])
				.map((t, i) => ({
					prompt: t.user_prompt ?? '',
					response: t.stop_response ?? '',
					turn_number: total - i,
					created_at: t.created_at
				}))
				.reverse();
			overviewTotal = total;
		} catch (err) {
			overviewError = err instanceof Error ? err.message : String(err);
		} finally {
			overviewLoading = false;
		}
	}

	async function runAssist(): Promise<void> {
		if (!selectedSessionId) return;
		assistState = 'thinking';
		assistError = null;
		userChangedFields = new Set();
		try {
			const resp = await assistSteerer({
				session_id: selectedSessionId,
				user_intent: intentText.trim()
			});
			if ('error' in resp) {
				assistState = 'error';
				assistError = resp.message;
			} else {
				assistSuggestion = resp.suggestion;
				assistRationale = resp.rationale;
				assistModelUsed = resp.model_used;
				assistState = 'suggestion';
			}
		} catch (err) {
			assistState = 'error';
			assistError = err instanceof ApiError ? err.message : String(err);
		}
	}

	function acceptSuggestion(): void {
		if (!assistSuggestion) return;
		intentText = assistSuggestion.prompt;
		authority = assistSuggestion.authority;
		mode = assistSuggestion.mode;
		if (assistSuggestion.config.confidence_threshold != null)
			confidenceThreshold = Math.round(assistSuggestion.config.confidence_threshold * 100) / 100;
		if (assistSuggestion.config.debounce_turns != null)
			debounceTurns = assistSuggestion.config.debounce_turns;
		assistState = 'idle';
		assistSuggestion = null;
	}

	function dismissSuggestion(): void {
		assistState = 'idle';
		assistSuggestion = null;
	}

	async function guidedNext(): Promise<void> {
		if (guidedStep === 1) {
			const sid = rawSessionId.trim() || selectedSessionId;
			if (!sid) return;
			selectedSessionId = sid;
			guidedStep = 2;
			await loadOverview(sid);
		} else if (guidedStep === 2) {
			guidedStep = 3;
		} else if (guidedStep === 3) {
			if (!displayName) {
				const short = shortId(selectedSessionId ?? '');
				displayName = `Steerer for ${short}`;
			}
			slug = autoSlug || displayName.toLowerCase().replace(/[^a-z0-9]+/g, '-');
			description = intentText;
			prompt = intentText;
			guidedStep = 4;
		}
	}

	async function guidedCreate(): Promise<void> {
		if (creating) return;
		const slugErr = validateSlug(slug);
		if (slugErr) {
			slugError = slugErr;
			return;
		}

		creating = true;
		createError = null;
		try {
			const body: SteererCreate = {
				name: slug,
				display_name: displayName || slug,
				description,
				prompt: prompt || intentText || undefined,
				authority,
				mode,
				model,
				priority,
				enabled: true,
				trigger_point: triggerPoint,
				invocation,
				pattern: invocation === 'pattern' ? pattern || null : null,
				config: { confidence_threshold: confidenceThreshold, debounce_turns: debounceTurns }
			};
			const envelope = await createSteerer(body);
			const newSteerer = envelope.steerer;

			if (selectedSessionId) {
				await enrolSession(selectedSessionId, {
					steerer_id: newSteerer.id,
					mode,
					authority,
					enrolled_by: 'guided-create'
				});
			}
			await goto(resolve('/steering?tab=monitor'));
		} catch (err) {
			createError = err instanceof ApiError ? err.message : String(err);
		} finally {
			creating = false;
		}
	}

	async function handleCreate(): Promise<void> {
		if (saving) return;
		slugError = validateSlug(slug);
		if (slugError) return;

		saving = true;
		saveError = null;
		saveWarnings = [];
		try {
			const body: SteererCreate = {
				name: slug,
				display_name: displayName,
				description: description || undefined,
				prompt: prompt || undefined,
				project_path: projectPath || undefined,
				authority,
				mode,
				model,
				priority,
				enabled,
				trigger_point: triggerPoint,
				invocation,
				pattern: invocation === 'pattern' ? pattern || null : null,
				config: { confidence_threshold: confidenceThreshold, debounce_turns: debounceTurns }
			};
			const envelope = await createSteerer(body);
			saveWarnings = envelope.warnings;
			await goto(resolve(`steering/${envelope.steerer.id}`));
		} catch (err) {
			saveError = err instanceof ApiError ? err.message : String(err);
		} finally {
			saving = false;
		}
	}

	async function handleSave(): Promise<void> {
		if (saving) return;
		saving = true;
		saveError = null;
		saveWarnings = [];
		saved = false;

		const patch: SteererPatch = {
			display_name: displayName,
			description,
			prompt: prompt || null,
			project_path: projectPath.trim() === '' ? null : projectPath,
			authority,
			mode,
			model,
			priority,
			enabled,
			trigger_point: triggerPoint,
			invocation,
			pattern: invocation === 'pattern' ? pattern || null : null,
			config: { confidence_threshold: confidenceThreshold, debounce_turns: debounceTurns }
		};

		try {
			const envelope = await patchSteerer(steererId, patch);
			steerer = envelope.steerer;
			populateForm(envelope.steerer);
			saveWarnings = envelope.warnings;
			saved = true;
			setTimeout(() => {
				saved = false;
			}, 3000);
		} catch (err) {
			saveError = err instanceof ApiError ? err.message : String(err);
		} finally {
			saving = false;
		}
	}

	function handleDelete(): void {
		if (deleting || !steerer) return;
		deleteError = null;
		confirmDelete = true;
	}

	function cancelDelete(): void {
		confirmDelete = false;
		deleteError = null;
	}

	async function performDelete(): Promise<void> {
		if (deleting || !steerer) return;
		deleting = true;
		deleteError = null;
		try {
			await deleteSteerer(steererId);
			history.back();
		} catch (err) {
			deleteError = err instanceof ApiError ? err.message : String(err);
		} finally {
			deleting = false;
		}
	}

	function populateForm(s: SteererDto): void {
		displayName = s.display_name;
		description = s.description;
		authority = s.authority;
		mode = s.mode;
		model = s.model;
		prompt = s.prompt ?? '';
		projectPath = s.project_path ?? '';
		priority = s.priority;
		enabled = s.enabled;
		confidenceThreshold =
			s.config.confidence_threshold != null
				? Math.round(s.config.confidence_threshold * 100) / 100
				: 0.7;
		debounceTurns = s.config.debounce_turns ?? 2;
		triggerPoint = s.trigger_point ?? 'response';
		invocation = s.invocation ?? 'every_turn';
		pattern = s.pattern ?? '';
	}

	async function load(): Promise<void> {
		if (isNew) {
			loading = false;
			if (isGuided) {
				if (SESSION_UUID_RE.test(presetSessionId)) {
					selectSession(presetSessionId);
					guidedStep = 2;
					void loadOverview(presetSessionId);
				}
				await loadSessions();
			}
			return;
		}
		loading = true;
		error = null;
		try {
			const s = await getSteerer(steererId);
			steerer = s;
			populateForm(s);
		} catch (err) {
			error = err instanceof ApiError ? err.message : String(err);
		} finally {
			loading = false;
		}
	}

	async function loadConfiguredDefaultModel(): Promise<void> {
		try {
			const { settings } = await getSettings();
			configuredDefaultModel =
				(settings.find((s) => s.key === 'steerer.model' && typeof s.value === 'string')
					?.value as string) ?? '';
		} catch {}
	}

	onMount(() => {
		void load();
		void loadProjects();
		void loadConfiguredDefaultModel();
		const unsub = onApiCacheUpdate(
			'/api/steerers',
			() => {
				void load();
			},
			{ prefix: true }
		);
		return () => {
			unsub();
			clearSearchTimer();
		};
	});
</script>

<svelte:head>
	<title>
		{steerer ? steerer.display_name : isNew ? 'New Steerer' : 'Steerer'} — Steering — mike
	</title>
</svelte:head>

<section class="se-page" data-slot="steerer-editor">
	{#if isNew && isGuided}
		<header class="se-header">
			<a
				href={resolve('/steering?tab=fleet')}
				class="se-back"
				data-augmented-ui="tl-clip br-clip border"
				aria-label="Back to fleet"
			>
				<ArrowLeft size={14} aria-hidden="true" />
			</a>
			<div class="se-header__titles">
				<h2 class="se-title">New steerer<HelpHint slug="steerer" /></h2>
				<span class="se-slug-backing">
					<span class="se-slug" data-augmented-ui="tl-clip br-clip border">
						{#if guidedStep === 1}Step 1 of 4 — Pick a session{/if}
						{#if guidedStep === 2}Step 2 of 4 — Session context{/if}
						{#if guidedStep === 3}Step 3 of 4 — Define the steer{/if}
						{#if guidedStep === 4}Step 4 of 4 — Confirm + create{/if}
					</span>
				</span>
			</div>
		</header>

		<div class="se-body">
			<nav class="gf-progress" aria-label="Guided steerer setup progress">
				{#each GUIDED_STEPS as { n, label } (n)}
					<button
						type="button"
						class="gf-progress__step"
						class:gf-progress__step--current={guidedStep === n}
						class:gf-progress__step--done={guidedStep > n}
						disabled={n >= guidedStep}
						aria-current={guidedStep === n ? 'step' : undefined}
						aria-label={n < guidedStep ? `Back to step ${n} — ${label}` : `Step ${n} — ${label}`}
						title={n < guidedStep ? `Back to step ${n} — ${label}` : `Step ${n} — ${label}`}
						onclick={() => goToGuidedStep(n)}
					>
						<span class="gf-progress__num" aria-hidden="true">{n}</span>
						<span class="gf-progress__label" aria-hidden="true">{label}</span>
					</button>
					{#if n < 4}<span class="gf-progress__rule" aria-hidden="true"></span>{/if}
				{/each}
			</nav>

			{#if guidedStep === 1}
				<div class="gf-step">
					<h3 class="gf-step__heading">Which session do you want to steer?</h3>
					<p class="gf-step__sub">
						Pick a live session. The steerer will watch this session's turns for drift and can
						inject corrections when it wanders off track.
					</p>
					<a href={resolve('/help/guides/steering-concepts')} class="se-guide-link">
						How does steering work? →
					</a>

					{#if sessionsError}
						<p class="se-banner se-banner--error" data-augmented-ui="tl-clip br-clip border">
							{sessionsError}
						</p>
					{:else if sessionsLoading}
						<div
							class="gf-sessions-loading"
							role="status"
							aria-label="Loading sessions"
							aria-busy="true"
						>
							{#each { length: 4 } as _, i (i)}
								<div class="gf-session-skel" style="animation-delay: {i * 0.05}s"></div>
							{/each}
						</div>
					{:else}
						<div class="se-field">
							<input
								class="se-input"
								type="search"
								placeholder="Search all sessions… (2+ characters)"
								aria-label="Search all sessions"
								bind:value={sessionSearch}
							/>
						</div>

						{#if searchActive && searchError !== null}
							<p class="gf-search-status gf-search-status--error" role="alert">
								Search failed: {searchError}
							</p>
						{:else if searchActive && searchLoading}
							<p class="gf-search-status" aria-live="polite">Searching all sessions…</p>
						{:else if !searchActive && searchQuery.length > 0}
							<p class="gf-search-status">
								Type at least {SESSION_SEARCH_MIN_QUERY} characters to search all sessions.
							</p>
						{/if}

						<div
							class="gf-session-list"
							data-augmented-ui="tl-clip br-clip border"
							role="listbox"
							aria-label={searchActive ? 'Matching sessions' : 'Recent sessions'}
						>
							{#each filteredSessions as s (s.id)}
								<button
									type="button"
									role="option"
									class="gf-session-row"
									class:gf-session-row--selected={selectedSessionId === s.id}
									aria-selected={selectedSessionId === s.id}
									onclick={() => selectSession(s.id)}
								>
									<span class="gf-session-id">{shortId(s.id)}</span>
									<span class="gf-session-meta">
										{s.project ?? s.title ?? 'Session'}
										<span class="gf-session-stat">· {s.request_count} requests</span>
										<span class="gf-session-time">· {timeAgo(s.last_seen)}</span>
									</span>
								</button>
							{/each}
							{#if filteredSessions.length === 0 && !(searchActive && searchLoading)}
								{#if searchActive}
									<p class="gf-empty-note">
										No sessions match "{searchQuery}" anywhere in the store.
									</p>
								{:else}
									<p class="gf-empty-note">No sessions yet.</p>
								{/if}
							{/if}
						</div>

						<div class="se-field">
							<label class="se-label" for="gf-raw-session">Or enter a session UUID directly</label>
							<input
								id="gf-raw-session"
								class="se-input"
								type="text"
								placeholder="e.g. 64b50581-7065-4013-8f77-dd7d8adeddb4"
								bind:value={rawSessionId}
								oninput={() => {
									selectedSessionId = rawSessionId.trim() || null;
								}}
							/>
						</div>
					{/if}

					<p class="gf-escape">
						Already know what you want?
						<a href={resolve('steering/new?mode=blank')} class="se-guide-link">
							Create a blank steerer instead →
						</a>
					</p>

					<div class="gf-footer">
						<button
							type="button"
							class="se-btn se-btn--save"
							data-augmented-ui="tl-clip br-clip border"
							disabled={!selectedSessionId && !rawSessionId.trim()}
							onclick={guidedNext}
						>
							Next: See what this session is doing →
						</button>
					</div>
				</div>
			{:else if guidedStep === 2}
				<div class="gf-step">
					<h3 class="gf-step__heading">
						What is session <code class="gf-id">{shortId(selectedSessionId ?? '')}</code> doing?
					</h3>
					<p class="gf-step__sub">
						Here's a summary of what this session has been working on. Use this to decide what you
						want your steerer to watch for.
					</p>

					{#if overviewError}
						<p class="se-banner se-banner--error" data-augmented-ui="tl-clip br-clip border">
							{overviewError}
						</p>
					{:else if overviewLoading}
						<div
							class="gf-sessions-loading"
							role="status"
							aria-busy="true"
							aria-label="Loading session overview"
						>
							{#each { length: 3 } as _, i (i)}
								<div
									class="gf-session-skel gf-session-skel--tall"
									style="animation-delay: {i * 0.05}s"
								></div>
							{/each}
						</div>
					{:else if overviewTurns.length === 0}
						<div class="gf-overview-empty" data-augmented-ui="tl-clip br-clip border">
							<p>
								This session is very new — no turns yet. You can still create a steerer; describe
								what you want the session to work on in Step 3.
							</p>
						</div>
					{:else}
						{#if overviewTotal > overviewTurns.length}
							<p class="gf-overview-count">
								Showing the last {overviewTurns.length} of {overviewTotal} turns.
							</p>
						{/if}
						<!-- svelte-ignore a11y_no_noninteractive_tabindex -->
						<div
							class="gf-overview-turns"
							data-augmented-ui="tl-clip br-clip border"
							role="group"
							aria-label="Recent turns in this session"
							tabindex="0"
						>
							{#each overviewTurns as turn (turn.turn_number)}
								<div class="gf-turn">
									<div class="gf-turn__meta">
										Turn {turn.turn_number}
										{#if turn.created_at}&nbsp;· {timeAgo(turn.created_at)}{/if}
									</div>
									<div class="gf-turn__prompt"><strong>User:</strong> {turn.prompt}</div>
									{#if turn.response}
										<div class="gf-turn__response"><strong>Claude:</strong> {turn.response}</div>
									{/if}
								</div>
							{/each}
						</div>
					{/if}

					<div class="gf-footer">
						<button
							type="button"
							class="se-btn"
							data-augmented-ui="tl-clip br-clip border"
							onclick={() => {
								guidedStep = 1;
							}}
						>
							← Back
						</button>
						<button
							type="button"
							class="se-btn se-btn--save"
							data-augmented-ui="tl-clip br-clip border"
							onclick={guidedNext}
						>
							Next: Define the steer →
						</button>
					</div>
				</div>
			{:else if guidedStep === 3}
				<div class="gf-step">
					<h3 class="gf-step__heading">What should the steerer watch for?</h3>
					<p class="gf-step__sub">
						Describe in plain language what this session should be focused on. The steerer uses this
						as its thread anchor — the reference point against which it evaluates each new turn for
						drift.
					</p>
					<a href={resolve('/help/guides/steering-concepts')} class="se-guide-link">
						How does steering work? →
					</a>

					<div class="se-field">
						<label class="se-label" for="gf-intent">
							What should this session be focused on?
						</label>
						<textarea
							id="gf-intent"
							class="se-textarea gf-intent"
							rows={4}
							placeholder="e.g. &quot;Implement the steering backend. Do not refactor unrelated files.&quot;"
							bind:value={intentText}></textarea>
						<p class="gf-intent-hint">
							This will be used as the steerer's prompt anchor. When the session's turns diverge
							from this description, the steerer fires.
						</p>
					</div>

					{#if assistState === 'idle'}
						{#if canAssist}
							<button
								type="button"
								class="se-btn gf-btn--assist"
								data-augmented-ui="tl-clip br-clip border"
								onclick={runAssist}
							>
								Help me write this →
							</button>
						{:else}
							<p class="gf-assist-note">Add a description above to get a suggestion.</p>
						{/if}
					{:else if assistState === 'thinking'}
						<button
							type="button"
							class="se-btn gf-btn--assist"
							data-augmented-ui="tl-clip br-clip border"
							disabled
						>
							<span class="gf-spinner" aria-hidden="true"></span>
							Drafting suggestion…
						</button>
					{:else if assistState === 'error'}
						<div class="gf-assist-error-row">
							<button
								type="button"
								class="se-btn gf-btn--assist"
								data-augmented-ui="tl-clip br-clip border"
								onclick={runAssist}
							>
								Help me write this →
							</button>
							<span class="gf-assist-err-msg"
								>{assistError ?? 'Could not reach the model. Fill in the fields below.'}</span
							>
						</div>
					{:else if assistState === 'suggestion' && assistSuggestion}
						<div class="gf-suggestion" data-augmented-ui="tl-clip br-clip border">
							<div class="gf-suggestion__head">
								<span>Suggestion from {assistModelUsed}</span>
								<button
									type="button"
									class="gf-suggestion__dismiss"
									aria-label="Dismiss suggestion"
									onclick={dismissSuggestion}
								>
									<X size={12} aria-hidden="true" />
								</button>
							</div>

							<div class="gf-suggestion__body">
								<div class="gf-rationale-field">
									<span class="gf-rationale-label">Prompt anchor:</span>
									<p class="gf-rationale-value">"{assistSuggestion.prompt}"</p>
									{#if assistRationale.purpose || assistRationale.prompt}
										<p class="gf-rationale-why">
											↳ {assistRationale.purpose ?? assistRationale.prompt}
										</p>
									{/if}
								</div>
								<div class="gf-rationale-field">
									<span class="gf-rationale-label"
										>Authority: {AUTHORITY_LABELS[assistSuggestion.authority]}<HelpHint
											slug="steering-authority"
										/></span
									>
									{#if assistRationale.authority}
										<p class="gf-rationale-why">↳ {assistRationale.authority}</p>
									{/if}
								</div>
								<div class="gf-rationale-field">
									<span class="gf-rationale-label"
										>Mode: {STEERING_MODE_LABELS[assistSuggestion.mode]}<HelpHint
											slug="steering-mode"
										/></span
									>
									{#if assistRationale.mode}
										<p class="gf-rationale-why">↳ {assistRationale.mode}</p>
									{/if}
								</div>
								{#if assistSuggestion.config.confidence_threshold != null}
									<div class="gf-rationale-field">
										<span class="gf-rationale-label"
											>Confidence threshold: {assistSuggestion.config.confidence_threshold}</span
										>
										{#if assistRationale.confidence_threshold}
											<p class="gf-rationale-why">↳ {assistRationale.confidence_threshold}</p>
										{/if}
									</div>
								{/if}
								{#if assistSuggestion.config.debounce_turns != null}
									<div class="gf-rationale-field">
										<span class="gf-rationale-label"
											>Debounce: {assistSuggestion.config.debounce_turns} turns</span
										>
										{#if assistRationale.debounce_turns}
											<p class="gf-rationale-why">↳ {assistRationale.debounce_turns}</p>
										{/if}
									</div>
								{/if}
							</div>

							<div class="gf-suggestion__footer">
								<button
									type="button"
									class="se-btn se-btn--save"
									data-augmented-ui="tl-clip br-clip border"
									onclick={acceptSuggestion}
								>
									Accept suggestion
								</button>
								<button
									type="button"
									class="se-btn"
									data-augmented-ui="tl-clip br-clip border"
									onclick={dismissSuggestion}
								>
									Edit manually
								</button>
							</div>
						</div>
					{/if}

					<div
						class="gf-radio-group"
						role="group"
						aria-label="How forceful should corrections be?"
						data-augmented-ui="tl-clip br-clip border"
					>
						<span class="gf-radio-legend-backing">
							<span class="gf-radio-legend" data-augmented-ui="tr-clip bl-clip border"
								>How forceful should corrections be?</span
							>
						</span>
						{#each ['supply_context', 'redirect', 'command'] as const as a (a)}
							<label class="gf-radio-row" class:gf-radio-row--selected={authority === a}>
								<input
									type="radio"
									name="authority"
									value={a}
									bind:group={authority}
									onchange={() => {
										userChangedFields = new Set([...userChangedFields, 'authority']);
									}}
								/>
								<div class="gf-radio-content">
									<span class="gf-radio-label">{AUTHORITY_LABELS[a]}</span>
									{#if a === 'supply_context'}
										<span class="gf-radio-desc"
											>Adds background info. Claude may use it or ignore it.</span
										>
										<span class="gf-radio-hint"
											>Best for: gentle nudges, supplying forgotten context.</span
										>
									{:else if a === 'redirect'}
										<span class="gf-radio-desc"
											>Suggests a change of direction. Claude can push back.</span
										>
										<span class="gf-radio-hint"
											>Best for: catching mild drift, reorienting without interrupting.</span
										>
									{:else if a === 'command'}
										<span class="gf-radio-desc"
											>Binding directive. Treated as operator instruction.</span
										>
										<span class="gf-radio-hint"
											>Best for: well-tested steerers, critical tasks, hard stops.</span
										>
										<span class="gf-radio-warn"
											>Use sparingly — commands override the session's own judgment.</span
										>
									{/if}
								</div>
							</label>
						{/each}
						<a href={resolve('/help/guides/steering-concepts#authority')} class="se-guide-link">
							Learn more about authority →
						</a>
					</div>

					<div
						class="gf-radio-group"
						role="group"
						aria-label="How should the correction arrive?"
						data-augmented-ui="tl-clip br-clip border"
					>
						<span class="gf-radio-legend-backing">
							<span class="gf-radio-legend" data-augmented-ui="tr-clip bl-clip border"
								>How should the correction arrive?</span
							>
						</span>
						{#each ['augment', 'drive'] as const as m (m)}
							<label class="gf-radio-row" class:gf-radio-row--selected={mode === m}>
								<input
									type="radio"
									name="mode"
									value={m}
									bind:group={mode}
									onchange={() => {
										userChangedFields = new Set([...userChangedFields, 'mode']);
									}}
								/>
								<div class="gf-radio-content">
									<span class="gf-radio-label">{STEERING_MODE_LABELS[m]}</span>
									{#if m === 'augment'}
										<span class="gf-radio-desc"
											>Adds the correction alongside the session's existing context.</span
										>
										<span class="gf-radio-hint"
											>Claude generates the next turn normally, with your correction in context.
											Recommended for most steerers.</span
										>
									{:else if m === 'drive'}
										<span class="gf-radio-desc"
											>The steerer's message replaces the session's next turn entirely.</span
										>
										<span class="gf-radio-hint"
											>Use only when augment has repeatedly failed to recapture the session.</span
										>
									{/if}
								</div>
							</label>
						{/each}
					</div>

					<div class="gf-footer">
						<button
							type="button"
							class="se-btn"
							data-augmented-ui="tl-clip br-clip border"
							onclick={() => {
								guidedStep = 2;
							}}
						>
							← Back
						</button>
						<button
							type="button"
							class="se-btn se-btn--save"
							data-augmented-ui="tl-clip br-clip border"
							onclick={guidedNext}
						>
							Review + create →
						</button>
					</div>
				</div>
			{:else if guidedStep === 4}
				<div class="gf-step">
					<h3 class="gf-step__heading">
						Create steerer and enrol session
						<code class="gf-id">{shortId(selectedSessionId ?? '')}</code>
					</h3>
					<p class="gf-step__sub">
						Review the configuration below. When you click Create, the steerer is registered and
						immediately enrolled in session {shortId(selectedSessionId ?? '')}.
					</p>

					<div class="gf-summary-card" data-augmented-ui="tl-clip br-clip border">
						<div class="gf-summary-row"><span>Name</span><span>{displayName || slug}</span></div>
						<div class="gf-summary-row"><span>Slug</span><span class="gf-mono">{slug}</span></div>
						{#if description}
							<div class="gf-summary-row"><span>Description</span><span>{description}</span></div>
						{/if}
						<div class="gf-summary-row">
							<span>Authority</span><span>{AUTHORITY_LABELS[authority]}</span>
						</div>
						<div class="gf-summary-row">
							<span>Mode</span><span>{STEERING_MODE_LABELS[mode]}</span>
						</div>
						<div class="gf-summary-row">
							<span>Model</span><span class="gf-mono"
								>{model ||
									`(inherit default${configuredDefaultModel ? `: ${configuredDefaultModel}` : ''})`}</span
							>
						</div>
						<div class="gf-summary-row">
							<span>Confidence</span><span>{confidenceThreshold}</span>
						</div>
						<div class="gf-summary-row">
							<span>Debounce</span><span>{debounceTurns} turns</span>
						</div>
						<div class="gf-summary-row"><span>Enabled</span><span>yes</span></div>
					</div>

					{#if createError}
						<div
							class="se-banner se-banner--error"
							data-augmented-ui="tl-clip br-clip border"
							role="alert"
						>
							{createError}
						</div>
					{/if}

					<button
						type="button"
						class="se-btn se-btn--save gf-create-btn"
						data-augmented-ui="tl-clip br-clip border"
						disabled={creating}
						onclick={guidedCreate}
					>
						{creating ? 'Creating…' : 'Create + enrol session'}
					</button>

					<div class="gf-what-next" data-augmented-ui="tl-clip br-clip border">
						<p class="gf-what-next__heading">After you click Create:</p>
						<ol class="gf-what-next__list">
							<li>The steerer is registered in the fleet<HelpHint slug="fleet" />.</li>
							<li>Session {shortId(selectedSessionId ?? '')} is enrolled in this steerer.</li>
							<li>The steerer starts evaluating the session's next turn.</li>
							<li>Events appear in the Monitor tab as they fire.</li>
						</ol>
						<p class="gf-what-next__note">
							You can disenrol the session or disable the steerer at any time from the Fleet or
							Session page.
						</p>
					</div>

					<div class="gf-footer">
						<button
							type="button"
							class="se-btn"
							data-augmented-ui="tl-clip br-clip border"
							onclick={() => {
								guidedStep = 3;
							}}
						>
							← Back to edit
						</button>
					</div>
				</div>
			{/if}
		</div>
	{:else}
		<header class="se-header">
			<a
				href={resolve('/steering?tab=fleet')}
				class="se-back"
				data-augmented-ui="tl-clip br-clip border"
				aria-label="Back to steerer fleet"
			>
				<ArrowLeft size={14} aria-hidden="true" />
			</a>
			<div class="se-header__titles">
				<h2 class="se-title">
					{#if loading}
						Loading…
					{:else if isNew}
						New steerer
					{:else}
						{steerer?.display_name ?? 'Steerer'}
					{/if}
				</h2>
				{#if steerer && !isNew}
					<div class="se-header__meta">
						<span class="se-slug-backing">
							<span class="se-slug" data-augmented-ui="tl-clip br-clip border">{steerer.name}</span>
						</span>
						<span class="se-state-backing">
							<span
								class="se-state-chip"
								class:se-state-chip--on={steerer.enabled}
								data-augmented-ui="tl-clip br-clip border"
							>
								{steerer.enabled ? 'ENABLED' : 'DISABLED'}
							</span>
						</span>
					</div>
				{/if}
			</div>
			{#if steerer && !isNew}
				<div class="se-header__actions">
					<a
						href={resolve(`agents/${steerer.agent_id}`)}
						class="se-btn"
						data-augmented-ui="tl-clip br-clip border"
						target="_blank"
						rel="noopener"
						title="Open agent prompt editor"
					>
						<ExternalLink size={13} aria-hidden="true" />
						Edit prompt
					</a>
					<button
						type="button"
						class="se-btn se-btn--danger"
						data-augmented-ui="tl-clip br-clip border"
						disabled={deleting || confirmDelete}
						onclick={handleDelete}
					>
						<Trash2 size={13} aria-hidden="true" />
						Delete
					</button>
				</div>
				{#if confirmDelete}
					<div
						class="se-delete-confirm"
						data-augmented-ui="tl-clip br-clip border"
						role="alertdialog"
						aria-label="Confirm delete steerer"
					>
						<p class="se-delete-confirm__msg">
							Delete steerer <strong>{steerer.display_name}</strong>? This cannot be undone.
						</p>
						{#if deleteError}
							<p class="se-delete-confirm__err" role="alert">{deleteError}</p>
						{/if}
						<div class="se-delete-confirm__actions">
							<button
								type="button"
								class="se-btn se-btn--danger"
								data-augmented-ui="tl-clip br-clip border"
								disabled={deleting}
								onclick={() => void performDelete()}
							>
								<Trash2 size={13} aria-hidden="true" />
								{deleting ? 'Deleting…' : 'Confirm delete'}
							</button>
							<button
								type="button"
								class="se-btn"
								data-augmented-ui="tl-clip br-clip border"
								disabled={deleting}
								onclick={cancelDelete}
							>
								Cancel
							</button>
						</div>
					</div>
				{/if}
			{/if}
		</header>

		{#if error}
			<div class="se-body">
				<LoadErrorBanner title="Couldn't load steerer" detail={error} onRetry={load} />
			</div>
		{/if}

		{#if loading && !error}
			<div class="se-body se-skeleton" role="status" aria-busy="true" aria-label="Loading steerer">
				{#each { length: 5 } as _, i (i)}
					<div class="se-skel-row" style="animation-delay: {i * 0.05}s"></div>
				{/each}
			</div>
		{/if}

		{#if !loading && !error && (steerer || isNew)}
			<div class="se-body">
				{#if isNew}
					<div class="se-onboarding" data-augmented-ui="tl-clip br-clip border">
						You are creating a steerer — a named agent configuration that monitors enrolled sessions
						for drift and injects steering messages when triggered. Give it a name and slug now (the
						slug cannot be changed later). You will be able to edit all other settings after
						creation.
					</div>
				{/if}

				{#if saveError}
					<div
						class="se-banner se-banner--error"
						data-augmented-ui="tl-clip br-clip border"
						role="alert"
					>
						{saveError}
					</div>
				{/if}
				{#if saved && saveWarnings.length === 0}
					<div
						class="se-banner se-banner--ok"
						data-augmented-ui="tl-clip br-clip border"
						role="status"
					>
						Saved.
					</div>
				{/if}
				{#if saveWarnings.length > 0}
					<div
						class="se-banner se-banner--warn"
						data-augmented-ui="tl-clip br-clip border"
						role="status"
					>
						<strong>Saved with warnings:</strong>
						<ul class="se-warning-list">
							{#each saveWarnings as w (w)}
								<li>{w}</li>
							{/each}
						</ul>
					</div>
				{/if}

				<div
					class="se-fieldset"
					role="group"
					aria-label="Basic information"
					data-augmented-ui="tl-clip br-clip border"
				>
					<span class="se-legend-backing">
						<span class="se-legend" data-augmented-ui="tr-clip bl-clip border"
							>Basic information</span
						>
					</span>

					<div class="se-field">
						<label class="se-label" for="se-display-name">Display name</label>
						<input
							id="se-display-name"
							type="text"
							class="se-input"
							bind:value={displayName}
							autocomplete="off"
							placeholder="e.g. Refocus"
						/>
					</div>

					{#if isNew}
						<div class="se-field">
							<label class="se-label" for="se-slug">Slug</label>
							<input
								id="se-slug"
								type="text"
								class="se-input"
								class:se-input--error={!!slugError}
								bind:value={slug}
								autocomplete="off"
								placeholder="e.g. refocus"
								oninput={() => {
									slugError = null;
								}}
							/>
							{#if slugError}
								<p class="se-field-error">{slugError}</p>
							{/if}
							<p class="se-field-hint">
								Used internally to identify this steerer. Cannot be changed after creation.
							</p>
						</div>
					{/if}

					<div class="se-field">
						<label class="se-label" for="se-project-scope">
							<Tooltip
								text="Global steerers evaluate every enrolled session across every project. A project-scoped steerer only evaluates sessions whose repo matches the selected project."
							>
								<HelpCircle size={12} aria-hidden="true" class="se-label__help" />
							</Tooltip>
							Project scope
						</label>
						<select id="se-project-scope" class="se-select" bind:value={projectPath}>
							<option value="">Global (all projects)</option>
							{#each projects as p (p.project_path)}
								<option value={p.project_path}>{p.custom_name ?? p.project_path}</option>
							{/each}
						</select>
						{#if projectsError}
							<p class="se-field-error">Couldn't load projects: {projectsError}</p>
						{/if}
					</div>

					<div class="se-field">
						<label class="se-label" for="se-description">Description</label>
						<textarea
							id="se-description"
							class="se-textarea"
							bind:value={description}
							rows={3}
							placeholder="What does this steerer do?"></textarea>
					</div>

					<div class="se-field se-field--row">
						<label class="se-label" for="se-enabled">
							<Tooltip
								text="Whether this steerer evaluates enrolled sessions. Disabled steerers are ignored at evaluation time even if sessions are enrolled."
							>
								<HelpCircle size={12} aria-hidden="true" class="se-label__help" />
							</Tooltip>
							Enabled
						</label>
						<button
							id="se-enabled"
							type="button"
							class="se-toggle"
							class:se-toggle--on={enabled}
							aria-pressed={enabled}
							aria-label="{enabled ? 'Disable' : 'Enable'} steerer"
							onclick={() => {
								enabled = !enabled;
							}}
						>
							<span class="se-toggle__track" aria-hidden="true">
								<span class="se-toggle__thumb"></span>
							</span>
							<span class="se-toggle__label">{enabled ? 'Enabled' : 'Disabled'}</span>
						</button>
					</div>
				</div>

				<div
					class="se-fieldset"
					role="group"
					aria-label="Steering prompt"
					data-augmented-ui="tl-clip br-clip border"
				>
					<span class="se-legend-backing">
						<span class="se-legend" data-augmented-ui="tr-clip bl-clip border">Steering prompt</span
						>
					</span>
					<div class="se-field">
						<label class="se-label" for="se-prompt"> Prompt anchor </label>
						<textarea
							id="se-prompt"
							class="se-textarea"
							bind:value={prompt}
							rows={4}
							placeholder="Describe what the session should stay focused on. The steerer uses this to detect drift."
						></textarea>
						<p class="se-field-hint">
							When the session's turns diverge from this description, the steerer fires.
						</p>
					</div>
				</div>

				<div
					class="se-fieldset"
					role="group"
					aria-label="Trigger"
					data-augmented-ui="tl-clip br-clip border"
				>
					<span class="se-legend-backing">
						<span class="se-legend" data-augmented-ui="tr-clip bl-clip border">Trigger</span>
					</span>

					<div class="se-field">
						<label class="se-label" for="se-trigger-point">
							<Tooltip
								text="When to evaluate: after the assistant responds (response), after the user sends a message (user_prompt), or at the end of a turn (turn_end)."
								placement="right"
							>
								<HelpCircle size={12} aria-hidden="true" class="se-label__help" />
							</Tooltip>
							Trigger point
						</label>
						<select id="se-trigger-point" class="se-select" bind:value={triggerPoint}>
							<option value="response">On assistant response</option>
							<option value="user_prompt">On user prompt</option>
							<option value="turn_end">On turn end</option>
						</select>
					</div>

					<div class="se-field">
						<label class="se-label" for="se-invocation">
							<Tooltip
								text="Every turn: the LLM judge always runs. Pattern: run a cheap keyword/regex pre-filter first — the judge only runs when the text matches."
								placement="right"
							>
								<HelpCircle size={12} aria-hidden="true" class="se-label__help" />
							</Tooltip>
							Invocation
						</label>
						<select id="se-invocation" class="se-select" bind:value={invocation}>
							<option value="every_turn">Every turn</option>
							<option value="pattern">Pattern pre-filter</option>
						</select>
					</div>

					{#if invocation === 'pattern'}
						<div class="se-field">
							<label class="se-label" for="se-pattern">
								Pattern
								<span class="se-label__note">one keyword or regex per line</span>
							</label>
							<textarea
								id="se-pattern"
								class="se-textarea"
								bind:value={pattern}
								rows={4}
								placeholder="e.g.
done
completed
task is finished"></textarea>
							<p class="se-field-hint">
								The pre-filter before the LLM judge runs. The judge is only called when the
								evaluated text matches at least one entry.
							</p>
						</div>
					{/if}
				</div>

				<div
					class="se-fieldset"
					role="group"
					aria-label="Abilities"
					data-augmented-ui="tl-clip br-clip border"
				>
					<span class="se-legend-backing">
						<span class="se-legend" data-augmented-ui="tr-clip bl-clip border">Abilities</span>
					</span>

					<div class="se-field">
						<label class="se-label" for="se-authority">
							<Tooltip
								text="How forceful the steerer's messages are. Ranges from context-only (weakest) to binding command (strongest)."
								placement="right"
							>
								<HelpCircle size={12} aria-hidden="true" class="se-label__help" />
							</Tooltip>
							Authority
						</label>
						<select id="se-authority" class="se-select" bind:value={authority}>
							<option value="supply_context">{AUTHORITY_LABELS.supply_context}</option>
							<option value="redirect">{AUTHORITY_LABELS.redirect}</option>
							<option value="command">{AUTHORITY_LABELS.command} — binding directive</option>
						</select>
					</div>

					<div class="se-field">
						<label class="se-label" for="se-mode">
							<Tooltip
								text="Whether the steerer nudges (augment) or takes the wheel (drive). Most steerers should stay on augment."
								placement="right"
							>
								<HelpCircle size={12} aria-hidden="true" class="se-label__help" />
							</Tooltip>
							Mode
						</label>
						<select id="se-mode" class="se-select" bind:value={mode}>
							<option value="augment">{STEERING_MODE_LABELS.augment}</option>
							<option value="drive">{STEERING_MODE_LABELS.drive} — stronger intervention</option>
						</select>
					</div>
				</div>

				<div
					class="se-fieldset"
					role="group"
					aria-label="Evaluation config"
					data-augmented-ui="tl-clip br-clip border"
				>
					<span class="se-legend-backing">
						<span class="se-legend" data-augmented-ui="tr-clip bl-clip border"
							>Evaluation config</span
						>
					</span>

					<div class="se-field">
						<label class="se-label" for="se-model">
							<Tooltip
								text="OpenRouter model used to run the drift-detection prompt. Smaller, cheaper models (gpt-5.6-luna, gemini-3.5-flash-lite) are sufficient for most steering tasks."
								placement="right"
							>
								<HelpCircle size={12} aria-hidden="true" class="se-label__help" />
							</Tooltip>
							Model
						</label>
						<p class="se-field-hint">
							Leave blank to inherit the fleet's configured default{configuredDefaultModel
								? ` (currently ${configuredDefaultModel})`
								: ''}.
						</p>
						<div class="se-model-row">
							<input
								id="se-model"
								type="text"
								class="se-input"
								bind:value={model}
								autocomplete="off"
								placeholder={configuredDefaultModel
									? `(inherit default: ${configuredDefaultModel})`
									: 'e.g. openai/gpt-5.6-luna'}
							/>
							<ModelPickerPopover
								currentValue={model}
								allowInherit
								inheritLabel={configuredDefaultModel
									? `(inherit default: ${configuredDefaultModel})`
									: '(inherit default model)'}
								presetModelIds={PRESET_MODEL_IDS}
								triggerLabel="Select"
								onSelect={(id) => {
									model = id;
								}}
							/>
						</div>
					</div>

					<div class="se-field">
						<label class="se-label" for="se-confidence">
							<Tooltip
								text="Score (0–1) the steerer's drift detector must reach before firing. Higher = less sensitive, fewer false positives. Default 0.7 is a reasonable starting point."
								placement="right"
							>
								<HelpCircle size={12} aria-hidden="true" class="se-label__help" />
							</Tooltip>
							Confidence threshold
							<span class="se-label__note">0.0 – 1.0 · fires when score ≥ threshold</span>
						</label>
						<input
							id="se-confidence"
							type="number"
							class="se-input se-input--narrow"
							bind:value={confidenceThreshold}
							min={0}
							max={1}
							step={0.01}
						/>
					</div>

					<div class="se-field">
						<label class="se-label" for="se-debounce">
							<Tooltip
								text="Minimum number of turns that must pass after a fire before the steerer can fire again. Prevents it from injecting on every turn during a rapid exchange."
								placement="right"
							>
								<HelpCircle size={12} aria-hidden="true" class="se-label__help" />
							</Tooltip>
							Debounce turns
							<span class="se-label__note">turns · minimum gap between fires</span>
						</label>
						<input
							id="se-debounce"
							type="number"
							class="se-input se-input--narrow"
							bind:value={debounceTurns}
							min={0}
							step={1}
						/>
					</div>

					<div class="se-field">
						<label class="se-label" for="se-priority">
							<Tooltip
								text="Evaluation order when multiple steerers are watching the same session. Lower numbers run first. If two steerers both want to fire in the same turn, the higher-priority one wins via arbitration."
								placement="right"
							>
								<HelpCircle size={12} aria-hidden="true" class="se-label__help" />
							</Tooltip>
							Priority
							<span class="se-label__note">lower = evaluated first</span>
						</label>
						<input
							id="se-priority"
							type="number"
							class="se-input se-input--narrow"
							bind:value={priority}
							min={1}
							step={1}
						/>
					</div>
				</div>

				<div class="se-footer">
					{#if isNew}
						<button
							type="button"
							class="se-btn se-btn--save"
							data-augmented-ui="tl-clip br-clip border"
							disabled={saving}
							onclick={handleCreate}
						>
							{saving ? 'Creating…' : 'Create steerer'}
						</button>
						<a href={resolve('steering/new?mode=guided')} class="se-guide-link">
							← Use guided flow instead
						</a>
					{:else}
						<button
							type="button"
							class="se-btn se-btn--save"
							data-augmented-ui="tl-clip br-clip border"
							disabled={saving}
							onclick={handleSave}
						>
							<Save size={13} aria-hidden="true" />
							{saving ? 'Saving…' : 'Save changes'}
						</button>
						<span class="se-footer__note">
							Changes to authority / mode take effect on the next evaluation.
						</span>
					{/if}
				</div>
			</div>
		{/if}
	{/if}
</section>

<style>
	.se-page {
		display: flex;
		flex-direction: column;
		height: 100%;
		overflow: hidden;
	}

	.se-header {
		display: flex;
		align-items: center;
		flex-wrap: wrap;
		gap: 12px;
		padding: 16px 24px;
		border-bottom: 1px solid var(--border-default);
		flex-shrink: 0;
	}

	.se-back {
		--edge: 4px;
		--aug-tl: var(--edge);
		--aug-br: var(--edge);
		--aug-border-all: 1px;
		--aug-border-bg: var(--border-subtle);
		display: flex;
		align-items: center;
		justify-content: center;
		width: 28px;
		height: 28px;
		color: var(--text-2);
		text-decoration: none;
		flex-shrink: 0;
	}

	.se-back:hover {
		--aug-border-bg: var(--status-danger-border);
		color: var(--status-danger-fg);
	}

	.se-header__titles {
		display: flex;
		flex-direction: column;
		gap: 4px;
		flex: 1;
		min-width: 0;
	}

	.se-header__meta {
		display: flex;
		align-items: center;
		gap: 6px;
	}

	.se-header__actions {
		display: flex;
		align-items: center;
		gap: 8px;
		flex-shrink: 0;
	}

	.se-delete-confirm {
		--edge: 5px;
		--aug-tl: var(--edge);
		--aug-br: var(--edge);
		--aug-border-all: 1px;
		--aug-border-bg: var(--status-danger-border);
		flex-basis: 100%;
		display: flex;
		flex-direction: column;
		gap: 8px;
		padding: calc(var(--edge) + 6px) calc(var(--edge) + 9px);
		background: var(--status-danger-bg, var(--surface-2));
	}

	.se-delete-confirm__msg {
		margin: 0;
		font-size: 12px;
		font-family: var(--font-mono, monospace);
		color: var(--text-1);
		line-height: 1.5;
	}

	.se-delete-confirm__err {
		margin: 0;
		font-size: 12px;
		font-family: var(--font-mono, monospace);
		color: var(--status-danger-fg);
	}

	.se-delete-confirm__actions {
		display: flex;
		align-items: center;
		gap: 8px;
	}

	.se-title {
		margin: 0;
		font-family: var(--font-display, 'Syne Variable', sans-serif);
		font-size: 1.05rem;
		font-weight: 700;
		letter-spacing: -0.01em;
		color: var(--text-strong);
	}

	.se-slug-backing,
	.se-state-backing {
		display: inline-flex;
		background: var(--surface-bg);
	}

	.se-slug {
		--aug-tl: 4px;
		--aug-tr: 0;
		--aug-br: 4px;
		--aug-bl: 0;
		--aug-border-all: 1px;
		--aug-border-bg: var(--border-default);
		font-family: var(--font-mono, monospace);
		font-size: 10px;
		font-weight: 600;
		letter-spacing: 0.04em;
		color: var(--text-3);
		padding: 2px 7px;
		background: linear-gradient(0deg, var(--surface-2), var(--surface-2)), var(--surface-bg);
	}

	.se-state-chip {
		--aug-tl: 4px;
		--aug-tr: 0;
		--aug-br: 4px;
		--aug-bl: 0;
		--aug-border-all: 1px;
		--aug-border-bg: var(--status-muted-border);
		font-family: var(--font-display, 'Syne Variable', sans-serif);
		font-weight: 800;
		font-size: 10px;
		text-transform: uppercase;
		letter-spacing: 0.06em;
		color: var(--status-muted-fg);
		padding: 2px 7px;
		background:
			linear-gradient(0deg, var(--status-muted-bg), var(--status-muted-bg)), var(--surface-bg);
	}

	.se-state-chip--on {
		--aug-border-bg: var(--status-success-border);
		color: var(--status-success-fg);
		background:
			linear-gradient(0deg, var(--status-success-bg), var(--status-success-bg)), var(--surface-bg);
	}

	.se-btn {
		--edge: 4px;
		--aug-tl: var(--edge);
		--aug-br: var(--edge);
		--aug-border-all: 1px;
		--aug-border-bg: var(--border-default);
		display: inline-flex;
		align-items: center;
		gap: 6px;
		padding: calc(var(--edge) + 2px) calc(var(--edge) + 6px);
		font-size: 12px;
		font-family: var(--font-mono, monospace);
		font-weight: 500;
		text-decoration: none;
		cursor: pointer;
		color: var(--text-1);
		background: var(--surface-2);
	}

	.se-btn:hover {
		--aug-border-bg: var(--status-danger-border);
		color: var(--status-danger-fg);
	}

	.se-btn:disabled {
		opacity: 0.5;
		cursor: not-allowed;
	}

	.se-btn--danger {
		color: var(--status-danger-fg);
		--aug-border-bg: var(--status-danger-border, var(--border-default));
	}

	.se-btn--danger:hover {
		background: var(--status-danger-bg, var(--surface-2));
		--aug-border-bg: var(--status-danger-border, var(--border-default));
		color: var(--status-danger-fg);
	}

	.se-btn--save {
		color: var(--status-danger-fg);
		--aug-border-bg: var(--status-danger-border);
		background: var(--status-danger-bg);
	}

	.se-btn--save:hover {
		background: color-mix(in srgb, var(--status-danger-bg) 70%, var(--status-danger-fg) 30%);
	}

	.se-guide-link {
		font-size: 12px;
		font-family: var(--font-mono, monospace);
		color: var(--text-3);
		text-decoration: none;
		display: inline-block;
	}

	.se-guide-link:hover {
		color: var(--text-1);
		text-decoration: underline;
	}

	.se-body {
		flex: 1;
		overflow-y: auto;
		padding: 20px 24px;
		display: flex;
		flex-direction: column;
		gap: 20px;
	}

	.se-skeleton {
		gap: 10px;
	}

	.se-skel-row {
		height: 40px;
		border-radius: 4px;
		background: var(--surface-2);
		animation: se-rise 0.4s ease both;
	}

	@keyframes se-rise {
		from {
			opacity: 0;
			transform: translateY(6px);
		}
		to {
			opacity: 1;
			transform: translateY(0);
		}
	}

	.se-onboarding {
		--edge: 5px;
		--aug-tl: var(--edge);
		--aug-br: var(--edge);
		--aug-border-all: 1px;
		--aug-border-bg: var(--border-subtle);
		padding: calc(var(--edge) + 7px) calc(var(--edge) + 9px);
		background: var(--surface-2);
		font-family: var(--font-mono, monospace);
		font-size: 12px;
		color: var(--text-2);
		line-height: 1.6;
	}

	.se-banner {
		--edge: 4px;
		--aug-tl: var(--edge);
		--aug-br: var(--edge);
		--aug-border-all: 1px;
		padding: calc(var(--edge) + 6px) calc(var(--edge) + 10px);
		font-size: 12px;
		font-family: var(--font-mono, monospace);
	}

	.se-banner--ok {
		--aug-border-bg: var(--status-success-border, var(--border-subtle));
		background: var(--status-success-bg, var(--surface-2));
		color: var(--status-success-fg);
	}

	.se-banner--error {
		--aug-border-bg: var(--status-danger-border, var(--border-subtle));
		background: var(--status-danger-bg, var(--surface-2));
		color: var(--status-danger-fg);
	}

	.se-banner--warn {
		--aug-border-bg: var(--status-warning-border);
		background: var(--status-warning-bg);
		color: var(--text-1);
	}

	.se-warning-list {
		margin: 6px 0 0 12px;
		padding: 0;
		display: flex;
		flex-direction: column;
		gap: 4px;
	}

	.se-fieldset {
		--edge: 6px;
		--aug-tl: var(--edge);
		--aug-br: var(--edge);
		--aug-border-all: 1px;
		--aug-border-bg: linear-gradient(
			135deg,
			var(--status-danger-fg) 0%,
			var(--status-danger-border) 35%,
			var(--border-default) 70%
		);
		padding: calc(var(--edge) + 8px) calc(var(--edge) + 10px);
		display: flex;
		flex-direction: column;
		gap: 14px;
		min-inline-size: 0;
	}

	.se-legend-backing {
		align-self: flex-start;
		display: inline-flex;
		background: var(--surface-card);
	}

	.se-legend {
		--aug-tl: 0;
		--aug-tr: 6px;
		--aug-br: 0;
		--aug-bl: 6px;
		--aug-border-all: 1px;
		--aug-border-bg: var(--status-danger-border);
		font-family: var(--font-display, 'Syne Variable', sans-serif);
		font-weight: 800;
		font-size: 10px;
		text-transform: uppercase;
		letter-spacing: 0.06em;
		color: var(--status-danger-fg);
		padding: 3px 9px;
		background:
			linear-gradient(0deg, var(--status-danger-bg), var(--status-danger-bg)), var(--surface-card);
	}

	.se-field {
		display: flex;
		flex-direction: column;
		gap: 4px;
	}

	.se-field--row {
		flex-direction: row;
		align-items: center;
		gap: 10px;
	}

	.se-label {
		font-family: var(--font-mono, monospace);
		font-size: 11px;
		font-weight: 600;
		color: var(--text-2);
		display: flex;
		align-items: center;
		gap: 6px;
	}

	.se-label__note {
		font-weight: 400;
		color: var(--text-3);
	}

	:global(.se-label__help) {
		color: var(--text-3);
		cursor: help;
		vertical-align: middle;
	}

	.se-field-hint {
		margin: 0;
		font-size: 11px;
		font-family: var(--font-mono, monospace);
		color: var(--text-3);
	}

	.se-field-error {
		margin: 0;
		font-size: 11px;
		font-family: var(--font-mono, monospace);
		color: var(--status-danger-fg);
	}

	.se-input {
		padding: 6px 8px;
		border: 1px solid var(--border-subtle);
		border-radius: 0;
		background: var(--surface-2);
		color: var(--text-1);
		font-family: var(--font-mono, monospace);
		font-size: 12px;
		width: 100%;
		box-sizing: border-box;
	}

	.se-input:focus {
		outline: 1px solid var(--status-danger-border);
		border-color: var(--status-danger-border);
	}

	.se-input:disabled {
		opacity: 0.6;
		cursor: not-allowed;
	}

	.se-input--narrow {
		max-width: 140px;
	}

	.se-input--error {
		border-color: var(--status-danger-border, var(--border-subtle));
	}

	.se-textarea {
		padding: 6px 8px;
		border: 1px solid var(--border-subtle);
		border-radius: 0;
		background: var(--surface-2);
		color: var(--text-1);
		font-family: var(--font-mono, monospace);
		font-size: 12px;
		width: 100%;
		box-sizing: border-box;
		resize: vertical;
		min-height: 64px;
	}

	.se-textarea:focus {
		outline: 1px solid var(--status-danger-border);
		border-color: var(--status-danger-border);
	}

	.se-select {
		padding: 6px 8px;
		border: 1px solid var(--border-subtle);
		border-radius: 0;
		background: var(--surface-2);
		color: var(--text-1);
		font-family: var(--font-mono, monospace);
		font-size: 12px;
		width: 100%;
		cursor: pointer;
	}

	.se-select:focus {
		outline: 1px solid var(--status-danger-border);
		border-color: var(--status-danger-border);
	}

	.se-toggle {
		background: none;
		border: none;
		cursor: pointer;
		padding: 2px;
		display: flex;
		align-items: center;
		gap: 8px;
	}

	.se-toggle__track {
		display: block;
		position: relative;
		width: 32px;
		height: 18px;
		border-radius: 9px;
		background: var(--surface-2);
		border: 1px solid var(--border-subtle);
		transition:
			background 0.15s,
			border-color 0.15s;
		flex-shrink: 0;
	}

	.se-toggle--on .se-toggle__track {
		background: var(--status-success-bg);
		border-color: var(--status-success-fg);
		box-shadow: 0 0 0 1px var(--status-success-border);
	}

	.se-toggle__thumb {
		display: block;
		position: absolute;
		top: 1px;
		left: 1px;
		width: 14px;
		height: 14px;
		border-radius: 50%;
		background: var(--text-3);
		transition:
			transform 0.15s,
			background 0.15s;
	}

	.se-toggle--on .se-toggle__thumb {
		transform: translateX(14px);
		background: var(--status-success-fg);
		box-shadow: 0 0 5px var(--status-success-border);
	}

	.se-toggle__label {
		font-family: var(--font-mono, monospace);
		font-size: 12px;
		color: var(--text-2);
	}

	.se-toggle--on .se-toggle__label {
		color: var(--status-success-fg);
	}

	.se-model-row {
		display: flex;
		gap: 8px;
		align-items: center;
	}

	.se-footer {
		display: flex;
		align-items: center;
		gap: 12px;
		position: sticky;
		bottom: -20px;
		margin: 8px -24px -20px;
		padding: 10px 24px 20px;
		background: var(--surface-bg);
		border-top: 1px solid var(--border-subtle);
		flex-shrink: 0;
	}

	.se-footer__note {
		font-family: var(--font-mono, monospace);
		font-size: 11px;
		color: var(--text-3);
	}

	.gf-progress {
		display: flex;
		align-items: center;
		padding-bottom: 6px;
		border-bottom: 1px solid var(--border-subtle);
	}

	.gf-progress__step {
		display: inline-flex;
		align-items: center;
		gap: 6px;
		padding: 4px 8px;
		border: none;
		border-radius: 3px;
		background: none;
		font-family: var(--font-mono, monospace);
		font-size: 11px;
		color: var(--text-3);
		cursor: default;
	}

	.gf-progress__step:not(:disabled) {
		cursor: pointer;
		color: var(--text-2);
	}

	.gf-progress__step:not(:disabled):hover {
		background: var(--surface-2);
		color: var(--text-1);
	}

	.gf-progress__step:focus-visible {
		outline: 2px solid var(--interactive-focus-ring);
		outline-offset: 1px;
	}

	.gf-progress__num {
		display: inline-flex;
		align-items: center;
		justify-content: center;
		width: 16px;
		height: 16px;
		flex-shrink: 0;
		border: 1px solid var(--border-subtle);
		border-radius: 50%;
		font-size: 10px;
		font-weight: 600;
	}

	.gf-progress__label {
		white-space: nowrap;
	}

	.gf-progress__step--done .gf-progress__num {
		border-color: var(--interactive-active-border);
		color: var(--interactive-active-fg);
	}

	.gf-progress__step--current {
		cursor: default;
	}

	.gf-progress__step--current .gf-progress__num {
		background: var(--interactive-active-bg);
		border-color: var(--interactive-active-border);
		color: var(--interactive-active-fg);
	}

	.gf-progress__step--current .gf-progress__label {
		color: var(--interactive-active-fg);
	}

	.gf-progress__rule {
		width: 14px;
		height: 1px;
		flex-shrink: 0;
		background: var(--border-subtle);
	}

	@media (max-width: 480px) {
		.gf-progress__step {
			gap: 0;
			padding: 4px 6px;
		}
		.gf-progress__label {
			display: none;
		}
		.gf-progress__rule {
			width: 8px;
		}
	}

	.gf-step {
		display: flex;
		flex-direction: column;
		gap: 16px;
	}

	.gf-step__heading {
		margin: 0;
		font-family: var(--font-display, 'Syne Variable', sans-serif);
		font-size: 1.1rem;
		font-weight: 700;
		letter-spacing: -0.01em;
		color: var(--text-strong);
	}

	.gf-step__sub {
		margin: 0;
		font-size: 13px;
		color: var(--text-2);
		line-height: 1.5;
	}

	.gf-id {
		font-family: var(--font-mono, monospace);
		font-size: 0.9em;
		color: var(--status-danger-fg);
	}

	.gf-sessions-loading {
		display: flex;
		flex-direction: column;
		gap: 6px;
	}

	.gf-session-skel {
		height: 44px;
		border-radius: 4px;
		background: var(--surface-2);
		animation: se-rise 0.4s ease both;
	}

	.gf-session-skel--tall {
		height: 80px;
	}

	.gf-session-list {
		--edge: 5px;
		--aug-tl: var(--edge);
		--aug-br: var(--edge);
		--aug-border-all: 1px;
		--aug-border-bg: var(--border-subtle);
		display: flex;
		flex-direction: column;
		gap: 4px;
		max-height: 280px;
		overflow-y: auto;
		padding: calc(var(--edge) + 2px);
	}

	.gf-session-row {
		display: flex;
		align-items: center;
		gap: 12px;
		padding: 8px 10px;
		border-radius: 0;
		border: 1px solid transparent;
		background: none;
		cursor: pointer;
		text-align: left;
		width: 100%;
		font-family: var(--font-mono, monospace);
		font-size: 12px;
	}

	.gf-session-row:hover {
		background: var(--surface-2);
		border-color: var(--border-subtle);
	}

	.gf-session-row--selected {
		background: var(--interactive-active-bg);
		border-color: var(--interactive-active-border);
	}

	.gf-session-id {
		font-weight: 600;
		color: var(--status-danger-fg);
		flex-shrink: 0;
		min-width: 72px;
	}

	.gf-session-meta {
		color: var(--text-2);
		flex: 1;
		min-width: 0;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.gf-session-stat {
		color: var(--text-3);
	}
	.gf-session-time {
		color: var(--text-3);
	}

	.gf-empty-note {
		margin: 0;
		padding: 12px;
		font-family: var(--font-mono, monospace);
		font-size: 12px;
		color: var(--text-3);
	}

	.gf-search-status {
		margin: 0;
		font-family: var(--font-mono, monospace);
		font-size: 11px;
		font-style: italic;
		color: var(--text-3);
	}

	.gf-search-status--error {
		font-style: normal;
		color: var(--status-danger-fg);
	}

	.gf-overview-turns {
		--edge: 5px;
		--aug-tl: var(--edge);
		--aug-br: var(--edge);
		--aug-border-all: 1px;
		--aug-border-bg: var(--border-subtle);
		display: flex;
		flex-direction: column;
		gap: 12px;
		max-height: 360px;
		overflow-y: auto;
		padding: calc(var(--edge) + 7px);
	}

	.gf-overview-turns:focus-visible {
		outline: none;
		box-shadow: inset 0 0 0 2px var(--interactive-focus-ring);
	}

	.gf-overview-empty {
		--edge: 5px;
		--aug-tl: var(--edge);
		--aug-br: var(--edge);
		--aug-border-all: 1px;
		--aug-border-bg: var(--border-subtle);
		padding: calc(var(--edge) + 11px);
		font-size: 13px;
		color: var(--text-2);
		background: var(--surface-2);
	}

	.gf-overview-count {
		margin: 0 0 8px;
		font-size: 12px;
		color: var(--text-3);
	}

	.gf-turn {
		display: flex;
		flex-direction: column;
		gap: 4px;
		padding-bottom: 12px;
		border-bottom: 1px solid var(--border-subtle);
	}

	.gf-turn:last-child {
		border-bottom: none;
		padding-bottom: 0;
	}

	.gf-turn__meta {
		font-family: var(--font-mono, monospace);
		font-size: 10px;
		font-weight: 600;
		text-transform: uppercase;
		color: var(--text-3);
		letter-spacing: 0.04em;
	}

	.gf-turn__prompt,
	.gf-turn__response {
		font-size: 12px;
		color: var(--text-1);
		line-height: 1.5;
		white-space: pre-wrap;
		word-break: break-word;
	}

	.gf-intent {
		min-height: 100px;
	}

	.gf-intent-hint {
		margin: 0;
		font-size: 11px;
		font-family: var(--font-mono, monospace);
		color: var(--text-3);
		line-height: 1.5;
	}

	.gf-btn--assist {
		border-color: var(--border-subtle);
		color: var(--text-2);
		align-self: flex-start;
	}

	.gf-btn--assist:hover {
		color: var(--text-1);
		border-color: var(--border-default);
	}

	.gf-assist-note {
		margin: 0;
		font-size: 11px;
		font-family: var(--font-mono, monospace);
		color: var(--text-3);
	}

	.gf-spinner {
		display: inline-block;
		width: 10px;
		height: 10px;
		border: 1.5px solid var(--text-3);
		border-top-color: var(--text-1);
		border-radius: 50%;
		animation: gf-spin 0.7s linear infinite;
	}

	@keyframes gf-spin {
		to {
			transform: rotate(360deg);
		}
	}

	.gf-assist-error-row {
		display: flex;
		align-items: center;
		gap: 12px;
	}

	.gf-assist-err-msg {
		font-size: 11px;
		font-family: var(--font-mono, monospace);
		color: var(--text-3);
	}

	.gf-suggestion {
		--edge: 6px;
		--aug-tl: var(--edge);
		--aug-br: var(--edge);
		--aug-border-all: 1px;
		--aug-border-bg: linear-gradient(
			135deg,
			var(--status-danger-fg) 0%,
			var(--status-danger-border) 35%,
			var(--border-default) 70%
		);
		background: var(--surface-2);
		display: flex;
		flex-direction: column;
		overflow: hidden;
	}

	.gf-suggestion__head {
		display: flex;
		align-items: center;
		justify-content: space-between;
		padding: 8px 12px;
		background: var(--surface-card);
		border-bottom: 1px solid var(--border-subtle);
		font-family: var(--font-mono, monospace);
		font-size: 11px;
		font-weight: 600;
		color: var(--text-2);
	}

	.gf-suggestion__dismiss {
		background: none;
		border: none;
		cursor: pointer;
		color: var(--text-3);
		font-size: 14px;
		padding: 0 2px;
	}

	.gf-suggestion__dismiss:hover {
		color: var(--text-1);
	}

	.gf-suggestion__body {
		padding: 12px;
		display: flex;
		flex-direction: column;
		gap: 10px;
	}

	.gf-suggestion__footer {
		padding: 8px 12px;
		border-top: 1px solid var(--border-subtle);
		display: flex;
		gap: 8px;
	}

	.gf-rationale-field {
		display: flex;
		flex-direction: column;
		gap: 2px;
	}

	.gf-rationale-label {
		font-family: var(--font-mono, monospace);
		font-size: 12px;
		font-weight: 600;
		color: var(--text-1);
	}

	.gf-rationale-value {
		margin: 0;
		font-family: var(--font-mono, monospace);
		font-size: 12px;
		color: var(--text-1);
		white-space: pre-wrap;
	}

	.gf-rationale-why {
		margin: 0;
		font-family: var(--font-mono, monospace);
		font-size: 11px;
		color: var(--text-3);
		line-height: 1.5;
	}

	.gf-radio-group {
		--edge: 6px;
		--aug-tl: var(--edge);
		--aug-br: var(--edge);
		--aug-border-all: 1px;
		--aug-border-bg: var(--border-subtle);
		padding: calc(var(--edge) + 4px);
		display: flex;
		flex-direction: column;
		gap: 8px;
		min-inline-size: 0;
	}

	.gf-radio-legend-backing {
		align-self: flex-start;
		display: inline-flex;
		background: var(--surface-bg);
	}

	.gf-radio-legend {
		--aug-tl: 0;
		--aug-tr: 4px;
		--aug-br: 0;
		--aug-bl: 4px;
		--aug-border-all: 1px;
		--aug-border-bg: var(--border-default);
		font-family: var(--font-mono, monospace);
		font-size: 11px;
		font-weight: 600;
		color: var(--text-2);
		padding: 3px 8px;
		background: linear-gradient(0deg, var(--surface-2), var(--surface-2)), var(--surface-bg);
	}

	.gf-radio-row {
		display: flex;
		align-items: flex-start;
		gap: 10px;
		padding: 8px 10px;
		border-radius: 0;
		border: 1px solid transparent;
		cursor: pointer;
	}

	.gf-radio-row:hover {
		background: var(--surface-2);
	}

	.gf-radio-row--selected {
		background: var(--interactive-active-bg);
		border-color: var(--interactive-active-border);
	}

	.gf-radio-row input[type='radio'] {
		margin-top: 2px;
		flex-shrink: 0;
		accent-color: var(--interactive-active-fg);
	}

	.gf-radio-content {
		display: flex;
		flex-direction: column;
		gap: 2px;
	}

	.gf-radio-label {
		font-family: var(--font-mono, monospace);
		font-size: 12px;
		font-weight: 600;
		color: var(--text-1);
	}

	.gf-radio-desc {
		font-size: 12px;
		color: var(--text-2);
	}

	.gf-radio-hint {
		font-size: 11px;
		color: var(--text-3);
	}

	.gf-radio-warn {
		font-size: 11px;
		color: var(--status-warning-fg);
	}

	.gf-footer {
		display: flex;
		align-items: center;
		gap: 10px;
		position: sticky;
		bottom: -20px;
		margin: 8px -24px -20px;
		padding: 10px 24px 20px;
		background: var(--surface-bg);
	}

	.gf-escape {
		margin: 0;
		font-family: var(--font-mono, monospace);
		font-size: 11px;
		color: var(--text-3);
	}

	.gf-summary-card {
		--edge: 6px;
		--aug-tl: var(--edge);
		--aug-br: var(--edge);
		--aug-border-all: 1px;
		--aug-border-bg: linear-gradient(
			135deg,
			var(--status-danger-fg) 0%,
			var(--status-danger-border) 35%,
			var(--border-default) 70%
		);
		overflow: hidden;
	}

	.gf-summary-row {
		display: flex;
		gap: 16px;
		padding: 8px 12px;
		border-bottom: 1px solid var(--border-subtle);
		font-family: var(--font-mono, monospace);
		font-size: 12px;
	}

	.gf-summary-row:last-child {
		border-bottom: none;
	}

	.gf-summary-row > span:first-child {
		color: var(--text-3);
		min-width: 100px;
		flex-shrink: 0;
	}

	.gf-summary-row > span:last-child {
		color: var(--text-1);
	}

	.gf-mono {
		font-family: var(--font-mono, monospace);
	}

	.gf-create-btn {
		width: 100%;
		justify-content: center;
		padding: 10px;
	}

	.gf-what-next {
		--edge: 5px;
		--aug-tl: var(--edge);
		--aug-br: var(--edge);
		--aug-border-all: 1px;
		--aug-border-bg: var(--border-subtle);
		background: var(--surface-2);
		padding: calc(var(--edge) + 9px) calc(var(--edge) + 11px);
		font-family: var(--font-mono, monospace);
		font-size: 12px;
		color: var(--text-2);
	}

	.gf-what-next__heading {
		margin: 0 0 8px;
		font-weight: 600;
		color: var(--text-1);
	}

	.gf-what-next__list {
		margin: 0 0 10px;
		padding-left: 20px;
		display: flex;
		flex-direction: column;
		gap: 4px;
	}

	.gf-what-next__note {
		margin: 0;
		color: var(--text-3);
		font-size: 11px;
	}
</style>
