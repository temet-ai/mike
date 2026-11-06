<script lang="ts">
	import { setContext } from 'svelte';
	import { SESSION_LIST_STORE_KEY, STATS_KEY, type StatsContext } from '$lib/context-keys';
	import { buildMockSessionListStore } from '$lib/test-support/storyMocks';
	import type { SessionSummary, StatsResponse } from '$lib/types';
	import Page from './+page.svelte';

	type Scenario = 'populated' | 'cold-start' | 'no-live';

	interface Props {
		scenario: Scenario;
	}

	const { scenario }: Props = $props();

	const NOW_MS = Date.now();
	const iso = (offsetMs: number): string => new Date(NOW_MS + offsetMs).toISOString();

	function session(overrides: Partial<SessionSummary> & { id: string }): SessionSummary {
		return {
			project: '/Users/dev/alpha',
			project_id: 1,
			cwd: '/Users/dev/alpha',
			project_resolution: 'git',
			first_seen: iso(-3_600_000),
			last_seen: iso(-30_000),
			request_count: 12,
			models: ['claude-opus-4-5'],
			last_status: 'Complete',
			title: null,
			...overrides
		};
	}

	const LIVE_ALPHA = session({
		id: '11111111-0000-0000-0000-000000000001',
		project: '/Users/dev/alpha',
		title: 'Implement auth middleware',
		last_status: 'Streaming',
		last_seen: iso(-15_000)
	});
	const LIVE_BETA = session({
		id: '22222222-0000-0000-0000-000000000001',
		project: '/Users/dev/beta',
		title: 'Refactor DB migrations',
		last_status: 'Pending',
		last_seen: iso(-45_000)
	});
	const HISTORY_ONLY = session({
		id: '33333333-0000-0000-0000-000000000001',
		project: '/Users/dev/alpha',
		title: 'Old finished session',
		last_status: 'Complete',
		last_seen: iso(-6 * 3_600_000)
	});

	function getSessions(): SessionSummary[] {
		switch (scenario) {
			case 'cold-start':
				return [];
			case 'no-live':
				return [HISTORY_ONLY];
			case 'populated':
			default:
				return [LIVE_ALPHA, LIVE_BETA, HISTORY_ONLY];
		}
	}

	// svelte-ignore state_referenced_locally
	const sessionStub = buildMockSessionListStore({
		sessions: getSessions(),
		hasMore: scenario === 'populated'
	});

	// svelte-ignore state_referenced_locally
	const FIXTURE_STATS: StatsResponse = {
		request_count: scenario === 'cold-start' ? 0 : 128_430,
		active_streams: scenario === 'cold-start' ? 0 : 2,
		uptime_seconds: 4 * 86_400 + 2 * 3_600,
		db_enabled: true
	};
	const statsContext: StatsContext = {
		get value() {
			return FIXTURE_STATS;
		}
	};

	setContext(SESSION_LIST_STORE_KEY, sessionStub);
	setContext(STATS_KEY, statsContext);
</script>

<Page />
