<script lang="ts">
	import { setContext } from 'svelte';
	import {
		SESSION_LIST_STORE_KEY,
		COLLAPSE_STORE_KEY,
		STATS_KEY,
		type StatsContext
	} from '$lib/context-keys';
	import type { SessionListStore } from '$lib/stores/sessions.svelte';
	import { looksLikeSessionId } from '$lib/stores/sessions.svelte';
	import type { CollapseStore } from '$lib/stores/collapseState.svelte';
	import type {
		SessionSummary,
		StatsResponse,
		ProxyEvent,
		UnifiedTextMatch,
		ContentSearchSessionGroup
	} from '$lib/types';
	import type { StatusKind } from '$lib/status';
	import Page from './+page.svelte';

	type Scenario =
		| 'loading'
		| 'empty'
		| 'error'
		| 'populated'
		| 'single-project'
		| 'ungrouped'
		| 'load-more'
		| 'load-more-loading'
		| 'load-more-error'
		| 'live'
		| 'collapsed-group'
		| 'single-req'
		| 'browse-active'
		| 'browse-all'
		| 'search-results';

	interface Props {
		scenario: Scenario;
		onToggle?: () => void;
		onLoadMore?: () => void;
	}

	let { scenario, onToggle, onLoadMore }: Props = $props();

	function makeSummary(overrides: Partial<SessionSummary> & { id: string }): SessionSummary {
		return {
			project: '/Users/dev/my-project',
			project_id: 1,
			cwd: '/Users/dev/my-project',
			project_resolution: 'git',
			first_seen: '2026-06-17T10:00:00Z',
			last_seen: '2026-06-17T10:05:00Z',
			request_count: 12,
			models: ['claude-opus-4-5'],
			last_status: 'Complete',
			title: null,
			pinned: false,
			avg_latency_ms: 1200,
			...overrides
		};
	}

	const SESSION_ALPHA_1 = makeSummary({
		id: '11111111-0000-0000-0000-000000000001',
		project: '/Users/dev/alpha',
		title: 'Implement auth middleware',
		last_status: 'Complete',
		request_count: 34,
		first_seen: '2026-06-20T09:00:00Z',
		last_seen: '2026-06-20T09:45:00Z',
		originating_operator_prompt:
			'Implement JWT-based auth middleware for the Express API. It should validate tokens, handle expiry, and attach the decoded user to req.user.'
	});

	const SESSION_ALPHA_2 = makeSummary({
		id: '11111111-0000-0000-0000-000000000002',
		project: '/Users/dev/alpha',
		title: 'Fix rate limit headers',
		last_status: 'Error: upstream timeout',
		request_count: 7,
		first_seen: '2026-06-19T08:00:00Z',
		last_seen: '2026-06-19T08:20:00Z',
		originating_operator_prompt: 'The rate limit headers are not being forwarded correctly. Fix it.'
	});

	const SESSION_BETA_1 = makeSummary({
		id: '22222222-0000-0000-0000-000000000001',
		project: '/Users/dev/beta',
		title: null,
		last_status: 'Streaming',
		request_count: 3,
		first_seen: '2026-06-19T10:30:00Z',
		last_seen: '2026-06-19T10:35:00Z',
		originating_operator_prompt: 'What is the current state of the migration system?'
	});

	const SESSION_BETA_2 = makeSummary({
		id: '22222222-0000-0000-0000-000000000002',
		project: '/Users/dev/beta',
		title: 'Refactor DB migrations',
		last_status: 'Complete',
		request_count: 19,
		first_seen: '2026-06-18T14:00:00Z',
		last_seen: '2026-06-18T16:00:00Z',
		pending_inbox_count: 3,
		originating_operator_prompt: `Refactor the DB migration system to use a proper versioning scheme.

Current problems:
1. Migrations are numbered sequentially but there are gaps (001, 002, 005, 008) which confuses the runner.
2. The migration runner doesn't detect drift between the applied migrations and the files on disk.
3. There is no rollback support — once a migration is applied, there is no way to undo it.
4. The migration lock is per-table, not per-database, so two servers can run different migrations simultaneously.
5. Migration filenames are not descriptive — they're just numbers with no indication of what they do.

What I want:
- Use a timestamp-based naming scheme (YYYYMMDDHHMMSS_description.sql).
- Add a checksum column to the applied_migrations table so we detect if a file changes after being applied.
- Add a dry-run mode that prints what would be run without executing.
- Add a --lock-timeout flag to prevent indefinite blocking.
- Document the rollback convention (down.sql files alongside up.sql).`
	});

	const SESSION_UNGROUPED = makeSummary({
		id: '33333333-0000-0000-0000-000000000001',
		project: null,
		title: null,
		last_status: 'Complete',
		request_count: 5,
		first_seen: '2026-06-18T11:00:00Z',
		last_seen: '2026-06-18T11:30:00Z',
		originating_operator_prompt:
			'Run a quick audit of the codebase and identify any obvious issues.'
	});

	const SESSION_ALPHA_IDONLY = makeSummary({
		id: '11111111-0000-0000-0000-000000000099',
		project: '/Users/dev/alpha',
		title: null,
		last_status: 'Complete',
		request_count: 2,
		first_seen: '2026-06-17T07:00:00Z',
		last_seen: '2026-06-17T07:05:00Z',
		originating_operator_prompt: null
	});

	const SESSION_ALPHA_PINNED = makeSummary({
		id: '11111111-0000-0000-0000-000000000098',
		project: '/Users/dev/alpha',
		title: 'Pinned investigation',
		last_status: 'Complete',
		request_count: 9,
		pinned: true,
		first_seen: '2026-06-17T06:00:00Z',
		last_seen: '2026-06-17T06:20:00Z',
		originating_operator_prompt: 'Investigate the recurring flaky test in the tool-uses suite.'
	});

	const BIG_PROJECT_SESSIONS: SessionSummary[] = Array.from({ length: 12 }, (_, i) =>
		makeSummary({
			id: `66666666-0000-0000-0000-0000000000${String(i + 1).padStart(2, '0')}`,
			project: '/Users/dev/big-project',
			title: `Big project session ${i + 1}`,
			last_status: 'Complete',
			request_count: 4 + i,
			first_seen: `2026-06-21T${String(8 + i).padStart(2, '0')}:00:00Z`,
			last_seen: `2026-06-21T${String(8 + i).padStart(2, '0')}:10:00Z`,
			originating_operator_prompt: `Task number ${i + 1} for the big project.`
		})
	);

	const SESSION_LIVE = makeSummary({
		id: '44444444-0000-0000-0000-000000000001',
		project: '/Users/dev/alpha',
		title: 'Streaming now',
		last_status: 'Streaming',
		request_count: 2,
		first_seen: '2026-06-20T09:55:00Z',
		last_seen: '2026-06-20T09:59:00Z',
		originating_operator_prompt: 'Review the current request flow and suggest improvements.'
	});

	const SESSION_SINGLE_REQ = makeSummary({
		id: '55555555-0000-0000-0000-000000000001',
		project: '/Users/dev/alpha',
		title: 'Single request session',
		last_status: 'Complete',
		request_count: 1,
		first_seen: '2026-06-18T08:00:00Z',
		last_seen: '2026-06-18T08:01:00Z',
		originating_operator_prompt: 'Check the health endpoint and confirm it returns 200.'
	});

	function getSessions(): SessionSummary[] {
		switch (scenario) {
			case 'loading':
			case 'empty':
				return [];
			case 'error':
				return [];
			case 'populated':
				return [
					SESSION_ALPHA_1,
					SESSION_ALPHA_2,
					SESSION_LIVE,
					SESSION_ALPHA_IDONLY,
					SESSION_ALPHA_PINNED,
					SESSION_BETA_1,
					SESSION_BETA_2,
					SESSION_UNGROUPED,
					...BIG_PROJECT_SESSIONS
				];
			case 'single-project':
				return [SESSION_ALPHA_1, SESSION_ALPHA_2];
			case 'ungrouped':
				return [SESSION_UNGROUPED, SESSION_ALPHA_1];
			case 'load-more':
			case 'load-more-loading':
			case 'load-more-error':
				return [SESSION_ALPHA_1, SESSION_BETA_1];
			case 'live':
				return [SESSION_LIVE, SESSION_ALPHA_1];
			case 'collapsed-group':
				return [SESSION_ALPHA_1, SESSION_ALPHA_2, SESSION_BETA_1];
			case 'single-req':
				return [SESSION_SINGLE_REQ];
			case 'browse-active':
				return [SESSION_LIVE, SESSION_ALPHA_1, SESSION_ALPHA_2];
			case 'browse-all':
				return [SESSION_LIVE, SESSION_ALPHA_1, SESSION_ALPHA_2, SESSION_BETA_1, SESSION_BETA_2];
			case 'search-results':
				return [SESSION_ALPHA_1];
			default:
				return [];
		}
	}

	function getLoading(): boolean {
		return scenario === 'loading';
	}

	function getLoadingMore(): boolean {
		return scenario === 'load-more-loading';
	}

	function getHasMore(): boolean {
		return (
			scenario === 'load-more' ||
			scenario === 'load-more-loading' ||
			scenario === 'load-more-error' ||
			scenario === 'browse-all'
		);
	}

	function getError(): string | null {
		return scenario === 'error' ? 'Failed to load sessions: connection refused' : null;
	}

	function getLoadMoreError(): string | null {
		return scenario === 'load-more-error' ? 'Failed to load older sessions: network error' : null;
	}

	// svelte-ignore state_referenced_locally
	const initialCollapsed = new Set<string>(
		scenario === 'collapsed-group' ? ['sidebar:project:/Users/dev/beta'] : []
	);
	let collapsed = $state(new Set<string>(initialCollapsed));

	const collapseStub: CollapseStore = {
		isCollapsed(key: string): boolean {
			return collapsed.has(key);
		},
		isExpanded(key: string, defaultExpanded = false): boolean {
			const isInSet = collapsed.has(key);
			return defaultExpanded ? !isInSet : isInSet ? false : true;
		},
		toggle(key: string): void {
			onToggle?.();
			const next = new Set(collapsed);
			if (next.has(key)) {
				next.delete(key);
			} else {
				next.add(key);
			}
			collapsed = next;
		},
		setDefault(_key: string, _c: boolean): void {},
		setAll(keys: string[], c: boolean): void {
			const next = new Set(collapsed);
			for (const k of keys) {
				if (c) {
					next.add(k);
				} else {
					next.delete(k);
				}
			}
			collapsed = next;
		}
	};

	let sessions = $state<SessionSummary[]>(getSessions());
	const loading = $state(getLoading());
	const loadingMore = $state(getLoadingMore());
	const error = $state<string | null>(getError());
	const loadMoreError = $state<string | null>(getLoadMoreError());
	const hasMore = $state(getHasMore());

	const ALL_SESSIONS: SessionSummary[] = [
		SESSION_ALPHA_1,
		SESSION_ALPHA_2,
		SESSION_BETA_1,
		SESSION_BETA_2,
		SESSION_UNGROUPED,
		SESSION_LIVE,
		SESSION_SINGLE_REQ
	];

	const ALL_BODY_MATCHES: UnifiedTextMatch[] = [
		{
			request_id: 34021,
			session_id: SESSION_ALPHA_1.id,
			source: 'response',
			byte_offset: 482,
			match_length: 4,
			line: 9,
			column: 21,
			before: 'The JWT middleware rejected the request because the ',
			matched: 'token',
			after: ' had expired before the refresh flow could run.'
		},
		{
			request_id: 34021,
			session_id: SESSION_ALPHA_1.id,
			source: 'response',
			byte_offset: 910,
			match_length: 4,
			line: 22,
			column: 5,
			before: 'Added a unit test that asserts an expired ',
			matched: 'token',
			after: ' is rejected with a 401 before hitting the handler.'
		},
		{
			request_id: 34030,
			session_id: SESSION_ALPHA_1.id,
			source: 'response',
			byte_offset: 120,
			match_length: 4,
			line: 3,
			column: 8,
			before: 'Wired the new bearer-',
			matched: 'token',
			after: ' validation into the Express router ahead of the protected routes.'
		},
		{
			request_id: 7,
			session_id: SESSION_ALPHA_2.id,
			source: 'response',
			byte_offset: 1190,
			match_length: 4,
			line: 14,
			column: 3,
			before: 'I checked the rate-limit forwarding and the upstream timed out before the ',
			matched: 'token',
			after: ' refresh could complete.'
		}
	];

	const ALL_CONTENT_GROUPS: ContentSearchSessionGroup[] = [
		{
			session_id: SESSION_ALPHA_1.id,
			project_path: SESSION_ALPHA_1.project ?? '',
			project_display_name: SESSION_ALPHA_1.project ?? '',
			title: SESSION_ALPHA_1.title,
			match_count: 5,
			turn_count: 3,
			last_match_at: '2026-06-20T09:40:00Z',
			matches_truncated: true,
			matches: [
				{
					turn_id: 501,
					request_id: 34021,
					role: 'prompt',
					chunk_idx: 0,
					byte_offset: 12,
					match_length: 4,
					line: 1,
					column: 13,
					before: 'Make sure the auth ',
					matched: 'token',
					after: ' expiry is handled with a clean 401, not a stack trace.',
					created_at: '2026-06-20T09:30:00Z'
				},
				{
					turn_id: 502,
					request_id: 34030,
					role: 'assistant',
					chunk_idx: 2,
					byte_offset: 88,
					match_length: 4,
					line: 3,
					column: 9,
					before: 'Wired the bearer-',
					matched: 'token',
					after: ' validation into the Express router ahead of the protected routes.',
					created_at: '2026-06-20T09:40:00Z'
				}
			]
		},
		{
			session_id: SESSION_ALPHA_2.id,
			project_path: SESSION_ALPHA_2.project ?? '',
			project_display_name: SESSION_ALPHA_2.project ?? '',
			title: SESSION_ALPHA_2.title,
			match_count: 1,
			turn_count: 1,
			last_match_at: '2026-06-19T08:15:00Z',
			matches_truncated: false,
			matches: [
				{
					turn_id: 601,
					request_id: 7,
					role: 'assistant',
					chunk_idx: 1,
					byte_offset: 40,
					match_length: 4,
					line: 2,
					column: 5,
					before: 'The upstream timed out before the refresh ',
					matched: 'token',
					after: ' could complete.',
					created_at: '2026-06-19T08:15:00Z'
				}
			]
		}
	];

	let searchHits = $state<SessionSummary[]>([]);
	let bodyHits = $state<UnifiedTextMatch[]>([]);
	let searchLoading = $state(false);
	let bodySearchLoading = $state(false);
	let searchError = $state<string | null>(null);
	let bodySearchError = $state<string | null>(null);
	let bodyFullMatchCount = $state(0);
	let bodyMatchesTruncated = $state(false);
	let bodyWindowNote = $state<string | null>(null);
	let contentHits = $state<ContentSearchSessionGroup[]>([]);
	let contentSearchLoading = $state(false);
	let contentSearchError = $state<string | null>(null);
	let contentTotalMatches = $state(0);
	let contentTotalSessions = $state(0);
	let contentHasMore = $state(false);
	const HYDRATABLE_SESSIONS: SessionSummary[] = [];
	let hydratedSessions = $state<SessionSummary[]>([]);
	const hydratingSessions = $state(false);
	const hydrationError = $state<string | null>(null);

	function searchSessionById(sessionId: string): SessionSummary | null {
		return (
			searchHits.find((s) => s.id === sessionId) ??
			sessions.find((s) => s.id === sessionId) ??
			hydratedSessions.find((s) => s.id === sessionId) ??
			null
		);
	}

	function setSearch(query: string): void {
		const raw = query.trim();
		const q = raw.toLowerCase();
		if (q.length < 4) {
			searchHits = [];
			bodyHits = [];
			searchLoading = false;
			bodySearchLoading = false;
			searchError = null;
			bodySearchError = null;
			bodyFullMatchCount = 0;
			bodyMatchesTruncated = false;
			bodyWindowNote = null;
			contentHits = [];
			contentSearchLoading = false;
			contentSearchError = null;
			contentTotalMatches = 0;
			contentTotalSessions = 0;
			contentHasMore = false;
			return;
		}
		searchHits = ALL_SESSIONS.filter(
			(s) =>
				s.id.toLowerCase().includes(q) ||
				(s.title?.toLowerCase().includes(q) ?? false) ||
				(s.project?.toLowerCase().includes(q) ?? false)
		);
		searchLoading = false;
		searchError = null;

		if (looksLikeSessionId(raw)) {
			bodyHits = [];
			bodyFullMatchCount = 0;
			bodyMatchesTruncated = false;
			bodyWindowNote = null;
			contentHits = [];
			contentTotalMatches = 0;
			contentTotalSessions = 0;
			contentHasMore = false;
		} else {
			bodyHits = ALL_BODY_MATCHES.filter(
				(m) =>
					m.matched.toLowerCase().includes(q) ||
					m.before.toLowerCase().includes(q) ||
					m.after.toLowerCase().includes(q)
			);
			bodyFullMatchCount = bodyHits.length;
			bodyMatchesTruncated = false;
			bodyWindowNote = 'Searched response bodies from the last 7 days.';

			contentHits = ALL_CONTENT_GROUPS.filter((g) =>
				g.matches.some(
					(m) =>
						m.matched.toLowerCase().includes(q) ||
						m.before.toLowerCase().includes(q) ||
						m.after.toLowerCase().includes(q)
				)
			);
			contentTotalMatches = contentHits.reduce((sum, g) => sum + g.match_count, 0);
			contentTotalSessions = contentHits.length;
			contentHasMore = false;
		}
		bodySearchLoading = false;
		bodySearchError = null;
		contentSearchLoading = false;
		contentSearchError = null;

		const known = new Set([...sessions.map((s) => s.id), ...searchHits.map((s) => s.id)]);
		const needed = new Set<string>();
		for (const g of contentHits) if (!known.has(g.session_id)) needed.add(g.session_id);
		for (const m of bodyHits) if (!known.has(m.session_id)) needed.add(m.session_id);
		hydratedSessions = HYDRATABLE_SESSIONS.filter((s) => needed.has(s.id));
	}

	let projectLoadingMore = $state<Record<string, boolean>>({});
	let projectExhausted = $state<Record<string, boolean>>({});

	async function loadMoreForProject(project: string): Promise<void> {
		if (projectExhausted[project] || projectLoadingMore[project]) return;
		projectLoadingMore = { ...projectLoadingMore, [project]: true };
		await new Promise((resolve) => setTimeout(resolve, 30));
		const alreadyLoaded = sessions.filter((s) => s.project === project).length;
		const olderSession = makeSummary({
			id: `77777777-0000-0000-0000-${String(alreadyLoaded).padStart(12, '0')}`,
			project,
			title: `Older session for ${project}`,
			request_count: 3,
			first_seen: '2026-06-10T08:00:00Z',
			last_seen: '2026-06-10T08:10:00Z',
			originating_operator_prompt: 'Fetched via the per-project "Load older sessions" affordance.'
		});
		sessions = [...sessions, olderSession];
		projectExhausted = { ...projectExhausted, [project]: true };
		projectLoadingMore = { ...projectLoadingMore, [project]: false };
	}

	const sessionStub: SessionListStore = {
		get sessions() {
			return sessions;
		},
		get loading() {
			return loading;
		},
		get loadingMore() {
			return loadingMore;
		},
		get error() {
			return error;
		},
		get loadMoreError() {
			return loadMoreError;
		},
		get hasMore() {
			return hasMore;
		},
		get searchHits() {
			return searchHits;
		},
		get searchLoading() {
			return searchLoading;
		},
		get searchError() {
			return searchError;
		},
		get bodyHits() {
			return bodyHits;
		},
		get bodySearchLoading() {
			return bodySearchLoading;
		},
		get bodySearchError() {
			return bodySearchError;
		},
		get bodyFullMatchCount() {
			return bodyFullMatchCount;
		},
		get bodyMatchesTruncated() {
			return bodyMatchesTruncated;
		},
		get bodyWindowNote() {
			return bodyWindowNote;
		},
		get contentHits() {
			return contentHits;
		},
		get contentSearchLoading() {
			return contentSearchLoading;
		},
		get contentSearchError() {
			return contentSearchError;
		},
		get contentTotalMatches() {
			return contentTotalMatches;
		},
		get contentTotalSessions() {
			return contentTotalSessions;
		},
		get contentHasMore() {
			return contentHasMore;
		},
		get hydratedSessions() {
			return hydratedSessions;
		},
		get hydratingSessions() {
			return hydratingSessions;
		},
		get hydrationError() {
			return hydrationError;
		},
		searchSessionById,
		get __wsTrackingSize() {
			return 0;
		},
		setSearch,
		async refresh(): Promise<void> {},
		async loadMore(): Promise<void> {
			onLoadMore?.();
		},
		loadMoreForProject,
		isProjectLoadingMore(project: string): boolean {
			return projectLoadingMore[project] ?? false;
		},
		projectLoadMoreError(_project: string): string | null {
			return null;
		},
		isProjectExhausted(project: string): boolean {
			return projectExhausted[project] ?? false;
		},
		async togglePin(sessionId: string, next: boolean): Promise<void> {
			const idx = sessions.findIndex((s) => s.id === sessionId);
			if (idx !== -1) sessions[idx] = { ...sessions[idx]!, pinned: next };
		},
		async toggleProjectPin(path: string, next: boolean): Promise<void> {
			sessions.forEach((s, i) => {
				if (s.project === path) sessions[i] = { ...s, pinned: next };
			});
		},
		handleEvent(_event: ProxyEvent): void {},
		latestWsEventForSession(_sessionId: string): ProxyEvent | null {
			if (_sessionId === SESSION_LIVE.id) {
				return {
					type: 'TextDelta',
					id: 1,
					text: ''
				} as ProxyEvent;
			}
			return null;
		},
		observeStatus(_sessionId: string, _kind: StatusKind, _now: number): void {},
		isRecentlyActive(_sessionId: string, _now: number): boolean {
			return _sessionId === SESSION_LIVE.id;
		},
		registerSessionRequests(_sessionId: string, _requestIds: number[]): void {},
		dispose(): void {}
	};

	const FIXTURE_STATS: StatsResponse = {
		request_count: 128_430,
		active_streams: 2,
		uptime_seconds: 4 * 86_400 + 2 * 3_600,
		db_enabled: true
	};
	const statsContext: StatsContext = {
		get value() {
			return FIXTURE_STATS;
		}
	};

	setContext(SESSION_LIST_STORE_KEY, sessionStub);
	setContext(COLLAPSE_STORE_KEY, collapseStub);
	setContext(STATS_KEY, statsContext);
</script>

<Page />
