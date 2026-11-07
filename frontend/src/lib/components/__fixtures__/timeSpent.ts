import type {
	TimeSpentBuildDto,
	TimeSpentCommandDto,
	TimeSpentDto,
	TimeSpentKindDto
} from '$lib/types';

const H = 3600;

export const TIME_SPENT_NOTES: string[] = [
	"An answer's wait runs from the model finishing the answer to the first tool result reaching the next request. It includes permission prompts and every tool the answer called.",
	'An answer that called several kinds of tool is counted under the slowest kind.',
	'Waits over an hour are left out of the sums and counted separately.',
	'Builds running at once counts cargo, nextest, make and gate commands from other sessions in any project that overlapped each build, on every machine that writes to the capture database.',
	'Model time runs from a request being sent to its answer finishing.'
];

/** Totals and build rows are the real figures (temet-proxy, last 14 days, main sessions). Per-kind
 *  answer counts, typical times and every command row are made up for the story. */
export const TIME_SPENT_BY_KIND: TimeSpentKindDto[] = [
	{
		kind: 'other shell',
		answers: 14_205,
		answers_over_an_hour: 0,
		wait_seconds: 30.6 * H,
		p50_wait_seconds: 0.4,
		p90_wait_seconds: 9.3,
		model_seconds: 31 * H,
		sessions: 61
	},
	{
		kind: 'rust tests',
		answers: 931,
		answers_over_an_hour: 1,
		wait_seconds: 24.8 * H,
		p50_wait_seconds: 2.9,
		p90_wait_seconds: 308,
		model_seconds: 3.1 * H,
		sessions: 38
	},
	{
		kind: 'mcp tool',
		answers: 6_420,
		answers_over_an_hour: 0,
		wait_seconds: 14.7 * H,
		p50_wait_seconds: 1.1,
		p90_wait_seconds: 12,
		model_seconds: 17 * H,
		sessions: 70
	},
	{
		kind: 'rust build',
		answers: 300,
		answers_over_an_hour: 0,
		wait_seconds: 10.5 * H,
		p50_wait_seconds: 68,
		p90_wait_seconds: 328,
		model_seconds: 1.2 * H,
		sessions: 29
	},
	{
		kind: 'waiting on background',
		answers: 412,
		answers_over_an_hour: 3,
		wait_seconds: 10.4 * H,
		p50_wait_seconds: 31,
		p90_wait_seconds: 240,
		model_seconds: 1.4 * H,
		sessions: 22
	},
	{
		kind: 'read, edit, search',
		answers: 21_880,
		answers_over_an_hour: 0,
		wait_seconds: 8.6 * H,
		p50_wait_seconds: 0.3,
		p90_wait_seconds: 2.1,
		model_seconds: 51 * H,
		sessions: 74
	},
	{
		kind: 'frontend tests',
		answers: 118,
		answers_over_an_hour: 0,
		wait_seconds: 5.4 * H,
		p50_wait_seconds: 96,
		p90_wait_seconds: 410,
		model_seconds: 0.5 * H,
		sessions: 12
	},
	{
		kind: 'lint',
		answers: 205,
		answers_over_an_hour: 0,
		wait_seconds: 5.1 * H,
		p50_wait_seconds: 41,
		p90_wait_seconds: 190,
		model_seconds: 0.7 * H,
		sessions: 25
	},
	{
		kind: 'gate',
		answers: 22,
		answers_over_an_hour: 0,
		wait_seconds: 2.3 * H,
		p50_wait_seconds: 340,
		p90_wait_seconds: 720,
		model_seconds: 0.1 * H,
		sessions: 9
	},
	{
		kind: 'database query',
		answers: 610,
		answers_over_an_hour: 0,
		wait_seconds: 2.1 * H,
		p50_wait_seconds: 3.2,
		p90_wait_seconds: 19,
		model_seconds: 1.6 * H,
		sessions: 19
	},
	{
		kind: 'deploy',
		answers: 31,
		answers_over_an_hour: 0,
		wait_seconds: 1.7 * H,
		p50_wait_seconds: 140,
		p90_wait_seconds: 420,
		model_seconds: 0.1 * H,
		sessions: 8
	}
];

export const TIME_SPENT_BUILDS: TimeSpentBuildDto[] = [
	{ running_at_once: 'none', sort_key: 0, calls: 656, seconds: 656 * 62, p50_seconds: 49 },
	{ running_at_once: '1 to 2', sort_key: 1, calls: 323, seconds: 323 * 70, p50_seconds: 54 },
	{ running_at_once: '3 to 5', sort_key: 2, calls: 127, seconds: 127 * 150, p50_seconds: 117 },
	{ running_at_once: '6 or more', sort_key: 3, calls: 86, seconds: 86 * 610, p50_seconds: 544 }
];

function row(
	command: string,
	kind: string,
	calls: number,
	seconds: number,
	p50_seconds: number,
	sessions: number
): TimeSpentCommandDto {
	return { command, kind, calls, seconds, p50_seconds, sessions };
}

export const TIME_SPENT_COMMANDS: TimeSpentCommandDto[] = [
	row('cargo nextest run --workspace', 'rust tests', 212, 9.8 * H, 161, 14),
	row('scripts/cx nextest run -p mike', 'rust tests', 388, 7.1 * H, 38, 21),
	row(
		'scripts/task-wait <tmp>/tasks/b7.output --grade',
		'waiting on background',
		290,
		6.2 * H,
		41,
		17
	),
	row('cargo build -p mike', 'rust build', 140, 4.6 * H, 70, 18),
	row('scripts/sb-test', 'frontend tests', 96, 4.3 * H, 109, 11),
	row('make test', 'rust tests', 120, 4.0 * H, 72, 13),
	row('make lint', 'lint', 160, 3.9 * H, 52, 22),
	row('scripts/gate', 'gate', 22, 2.3 * H, 340, 9),
	row('./scripts/start-proxy.sh', 'deploy', 31, 1.7 * H, 140, 8),
	row(
		'docker exec armitage-pg psql -U devuser -d claude_proxy -X -At -c "select count(*) from requests.requests where created_at > now() - interval \'1 day\'"',
		'database query',
		410,
		1.6 * H,
		4.1,
		16
	),
	row('scripts/cx clippy --workspace --all-targets', 'lint', 88, 1.5 * H, 58, 12),
	row('git push origin main', 'git', 74, 1.4 * H, 6.2, 19),
	row('scripts/cx build --release -p mike', 'rust build', 41, 1.3 * H, 96, 9),
	row('cargo test -p mike-mcp', 'rust tests', 77, 1.2 * H, 33, 10),
	row('scripts/sb-test routes/sessions', 'frontend tests', 38, 1.1 * H, 84, 7),
	row('git fetch --all --prune', 'git', 120, 1.0 * H, 2.4, 15),
	row('scripts/deploy-proxy.sh --force', 'deploy', 14, 0.9 * H, 210, 5),
	row('sleep 30 && tail -n 40 .deploy/proxy.log', 'waiting on background', 133, 0.8 * H, 30, 11),
	row('rg -n "time_spent" crates frontend/src', 'other shell', 902, 0.7 * H, 0.9, 40),
	row('make fmt', 'lint', 66, 0.6 * H, 21, 12),
	row('find . -name "*.sql" -newer Cargo.lock', 'other shell', 310, 0.5 * H, 1.2, 22),
	row(
		'psql -U devuser -d claude_proxy -c "\\d requests.requests"',
		'database query',
		52,
		0.4 * H,
		2.1,
		8
	),
	row('scripts/test-commit-gates', 'gate', 12, 0.3 * H, 80, 5),
	row('git log --oneline -20', 'git', 411, 0.2 * H, 0.5, 31)
];

export const TIME_SPENT_PAGE_SIZE = 20;

export function timeSpentTotals(byKind: TimeSpentKindDto[]): TimeSpentDto['totals'] {
	return {
		wait_seconds: byKind.reduce((sum, k) => sum + k.wait_seconds, 0),
		model_seconds: 108 * H,
		answers: 45_134
	};
}

export function buildTimeSpent(
	overrides: Partial<TimeSpentDto> & { kind?: string | null; offset?: number; limit?: number } = {}
): TimeSpentDto {
	const { kind = null, offset = 0, limit = TIME_SPENT_PAGE_SIZE, ...rest } = overrides;
	const matching = TIME_SPENT_COMMANDS.filter((c) => kind === null || c.kind === kind);
	return {
		from: '2026-09-28T09:00:00Z',
		to: '2026-10-12T09:00:00Z',
		project: null,
		include_subagents: false,
		session_id: null,
		totals: timeSpentTotals(TIME_SPENT_BY_KIND),
		by_kind: TIME_SPENT_BY_KIND,
		slow_commands: matching.slice(offset, offset + limit),
		builds_running_at_once: TIME_SPENT_BUILDS,
		notes: TIME_SPENT_NOTES,
		...rest
	};
}

export function timeSpentFromUrl(url: string): TimeSpentDto {
	const params = new URL(url, 'http://localhost').searchParams;
	return buildTimeSpent({
		kind: params.get('kind'),
		offset: Number(params.get('offset') ?? '0'),
		limit: Number(params.get('limit') ?? String(TIME_SPENT_PAGE_SIZE))
	});
}

export function stubTimeSpentFetch(): void {
	const inner = globalThis.fetch;
	globalThis.fetch = (async (input: RequestInfo | URL, init?: RequestInit) => {
		const url =
			typeof input === 'string' ? input : input instanceof URL ? input.toString() : input.url;
		if (url.includes('/api/time_spent')) {
			return new Response(JSON.stringify(timeSpentFromUrl(url)), {
				status: 200,
				headers: { 'content-type': 'application/json' }
			});
		}
		return inner(input, init);
	}) as typeof fetch;
}
