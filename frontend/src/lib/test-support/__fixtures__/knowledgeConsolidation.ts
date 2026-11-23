import type { CapabilityRow } from '$lib/api/capabilities';
import type {
	Carrier,
	DecisionRow,
	MeasureSeriesRow,
	SubjectDetail,
	SubjectSummary
} from '$lib/api/knowledgeConsolidation';
import type { FleetProjectSummary } from '$lib/fleet/types';

export type ConsolidationScenario =
	'off-never' | 'off-history' | 'first-pass' | 'steady' | 'degraded' | 'empty';

export const PROJECTS: FleetProjectSummary[] = [
	{
		project_path: '/mnt/data/projects/temet/temet-proxy',
		project_id: 3,
		custom_name: null,
		session_count: 41,
		request_count: 612,
		first_seen: '2026-05-02T09:00:00Z',
		last_seen: '2026-11-20T16:40:00Z',
		pinned: true
	},
	{
		project_path: '/mnt/data/projects/temet/riski',
		project_id: 7,
		custom_name: 'riski',
		session_count: 6,
		request_count: 58,
		first_seen: '2026-06-11T10:00:00Z',
		last_seen: '2026-11-02T08:00:00Z',
		pinned: false
	}
];

const DAY = 86_400_000;
const NEWEST_RUN = Date.parse('2026-11-20T16:40:00Z');

export function lessonCarrier(id: number, statedAt: string): Carrier {
	return {
		reference: `lesson:018f7b2a-0000-7000-8000-${String(id).padStart(12, '0')}`,
		kind: 'lesson',
		label: `Lesson ${id}`,
		lesson_id: id,
		source_name: null,
		relative_path: null,
		scope: 'project',
		stated_at: statedAt
	};
}

export function fileCarrier(
	source: string,
	path: string,
	statedAt: string,
	scope: Carrier['scope'] = 'project'
): Carrier {
	return {
		reference: `artifact:${source}:${path}`,
		kind: 'file',
		label: `${source} · ${path}`,
		lesson_id: null,
		source_name: source,
		relative_path: path,
		scope,
		stated_at: statedAt
	};
}

export function measures(runs = 30): MeasureSeriesRow[] {
	return Array.from({ length: runs }, (_, i) => {
		const left = runs - 1 - i;
		return {
			id: 100 + i,
			run_id: 500 + i,
			project_id: 3,
			measured_at: new Date(NEWEST_RUN - left * DAY).toISOString(),
			statements_current: 2044 - Math.round((i * 132) / (runs - 1)),
			subjects: 1604 + Math.round((i * 4) / (runs - 1)),
			subjects_shared: 275 - Math.round((i * 61) / (runs - 1)),
			conflicts_open: i < 10 ? 7 : i < 20 ? 4 : 2,
			guidance_chars: 157_730 - Math.round((i * 9410) / (runs - 1)),
			misgrouped_share: 0.048 - (i * 0.006) / (runs - 1),
			split_subject_share: 0.018,
			grouping_distance: 0.7
		};
	});
}

const RANKING_SUBJECT = { id: 41, title: 'Ranking Wright problems' };

export const CONFLICTS: DecisionRow[] = [
	{
		id: 910,
		run_id: 520,
		project_id: 3,
		subject_id: RANKING_SUBJECT.id,
		subject_title: RANKING_SUBJECT.title,
		statement_id: 8101,
		item_id: 2733,
		carrier_reference: 'lesson:2733',
		verdict: 'conflict_open',
		counterpart_statement_id: 8102,
		counterpart_reference: 'lesson:2741',
		text_before: 'Rank open problems by how many sessions they touched.',
		text_after: null,
		reason: 'One orders by reach, the other by recency.',
		evidence: [],
		model: 'sonnet',
		application: 'none',
		applied_at: null,
		error: null,
		created_at: '2026-11-19T14:02:00Z',
		carrier: lessonCarrier(2733, '2026-11-18T09:00:00Z'),
		counterpart: lessonCarrier(2741, '2026-11-18T21:00:00Z'),
		counterpart_text: 'Rank open problems by the newest failure first.'
	},
	{
		id: 911,
		run_id: 520,
		project_id: 3,
		subject_id: 52,
		subject_title: 'Capture database access',
		statement_id: 8201,
		item_id: 61,
		carrier_reference: 'artifact:claude-memory:memory/capture-db.md',
		verdict: 'conflict_open',
		counterpart_statement_id: 8202,
		counterpart_reference: 'artifact:instructions:AGENTS.md',
		text_before:
			'Query the capture database as mike_adhoc with a 5 s statement timeout, and never run a full scan of the requests table.',
		text_after: null,
		reason: 'One says a scan of requests is never allowed, the other allows one when time-bounded.',
		evidence: [],
		model: 'sonnet',
		application: 'none',
		applied_at: null,
		error: null,
		created_at: '2026-11-20T08:15:00Z',
		carrier: fileCarrier('claude-memory', 'memory/capture-db.md', '2026-11-19T10:00:00Z'),
		counterpart: fileCarrier('instructions', 'AGENTS.md', '2026-11-19T18:00:00Z'),
		counterpart_text:
			'Read-only SQL against the capture database is time-bounded on large tables; a bounded scan is allowed.'
	}
];

function change(partial: Partial<DecisionRow> & Pick<DecisionRow, 'id' | 'verdict'>): DecisionRow {
	return {
		run_id: 530,
		project_id: 3,
		subject_id: 12,
		subject_title: 'Use ask before git for past work',
		statement_id: 9000 + partial.id,
		item_id: partial.id,
		carrier_reference: 'lesson:2690',
		counterpart_statement_id: null,
		counterpart_reference: null,
		text_before: 'Call ask first for anything about past sessions.',
		text_after: null,
		reason: 'AGENTS.md line 4 gives the same instruction.',
		evidence: [
			{
				label: 'instructions · AGENTS.md',
				note: 'Call ask first for past work, then git.',
				at: '2026-11-19T10:00:00Z',
				operator: false
			},
			{
				label: 'correction',
				note: 'use ask before running git log',
				at: '2026-11-18T12:00:00Z',
				operator: true
			}
		],
		model: 'sonnet',
		application: 'applied',
		applied_at: '2026-11-20T16:31:00Z',
		error: null,
		created_at: '2026-11-20T16:31:00Z',
		carrier: lessonCarrier(2690, '2026-11-17T09:00:00Z'),
		counterpart: null,
		counterpart_text: null,
		...partial
	};
}

export const CHANGES: DecisionRow[] = [
	change({
		id: 960,
		verdict: 'covered',
		counterpart_reference: 'artifact:instructions:AGENTS.md',
		counterpart: fileCarrier('instructions', 'AGENTS.md', '2026-11-19T10:00:00Z'),
		counterpart_text: 'Call ask first for past work, then git.'
	}),
	change({
		id: 959,
		verdict: 'tightened',
		subject_id: 52,
		subject_title: 'Capture database access',
		carrier_reference: 'artifact:claude-memory:memory/capture-db.md',
		carrier: fileCarrier('claude-memory', 'memory/capture-db.md', '2026-11-12T10:00:00Z'),
		text_before:
			'When you query the capture database, always use the read-only role, and set a statement timeout so a slow query cannot hold the connection for long.',
		text_after: 'Query the capture DB as mike_adhoc with a 5 s statement timeout.',
		reason: 'The same two instructions, in a third of the words.',
		application: 'proposed',
		applied_at: null
	}),
	change({
		id: 958,
		verdict: 'superseded',
		subject_id: 61,
		subject_title: 'Deploy order',
		carrier_reference: 'lesson:2512',
		carrier: lessonCarrier(2512, '2026-10-30T09:00:00Z'),
		counterpart_reference: 'lesson:2688',
		counterpart: lessonCarrier(2688, '2026-11-16T09:00:00Z'),
		text_before: 'Deploy the SPA before the proxy.',
		reason: 'Newer lesson 2688 reverses the order and you confirmed it.',
		application: 'failed',
		applied_at: null,
		error: 'permission denied writing to memory/deploy.md'
	}),
	change({
		id: 957,
		verdict: 'merged',
		subject_id: 70,
		subject_title: 'Release checks',
		carrier_reference: 'lesson:2401',
		carrier: lessonCarrier(2401, '2026-10-20T09:00:00Z'),
		counterpart_reference: 'lesson:2402',
		counterpart: lessonCarrier(2402, '2026-10-20T11:00:00Z'),
		text_before: 'Run the release acceptance script before tagging.',
		reason: 'Its detail was added to lesson 2402.',
		application: 'undone',
		applied_at: '2026-11-19T11:00:00Z',
		created_at: '2026-11-19T10:30:00Z'
	}),
	change({
		id: 956,
		verdict: 'misgrouped',
		subject_id: 80,
		subject_title: 'Storybook testing',
		carrier_reference: 'lesson:2300',
		carrier: lessonCarrier(2300, '2026-10-10T09:00:00Z'),
		text_before: 'Wait for computed pointer-events before clicking after a Sheet closes.',
		reason: 'This is about overlays, not Storybook runs.',
		application: 'none',
		applied_at: null,
		created_at: '2026-11-18T10:30:00Z'
	}),
	change({
		id: 955,
		verdict: 'kept',
		subject_id: 12,
		carrier_reference: 'lesson:2200',
		carrier: lessonCarrier(2200, '2026-10-01T09:00:00Z'),
		text_before: 'Read the whole spec before building.',
		reason: 'No other statement says this.',
		application: 'none',
		applied_at: null,
		created_at: '2026-11-18T09:30:00Z'
	})
];

function subject(id: number, title: string, members: number, extra: Partial<SubjectSummary> = {}) {
	return {
		id,
		project_id: 3,
		title,
		text: `${title}: what sessions are told about it.`,
		members,
		carriers: Math.max(1, members - 1),
		changed_at: null,
		judged_at: '2026-11-20T16:40:00Z',
		newest_decision_at: '2026-11-20T16:31:00Z',
		newest_verdict: 'kept' as const,
		newest_reason: null,
		...extra
	} satisfies SubjectSummary;
}

export const SUBJECTS: SubjectSummary[] = [
	subject(41, 'Ranking Wright problems', 4, {
		newest_verdict: 'conflict_open',
		newest_decision_at: '2026-11-19T14:02:00Z',
		judged_at: '2026-11-19T14:02:00Z'
	}),
	subject(52, 'Capture database access', 3, {
		newest_verdict: 'tightened',
		newest_decision_at: '2026-11-20T16:31:00Z'
	}),
	subject(12, 'Use ask before git for past work', 2, { newest_verdict: 'covered' }),
	subject(61, 'Deploy order', 2, {
		changed_at: '2026-11-20T17:00:00Z',
		newest_verdict: 'superseded',
		newest_decision_at: '2026-11-20T16:31:00Z'
	}),
	subject(80, 'Storybook testing', 6, {
		newest_verdict: null,
		newest_decision_at: null,
		judged_at: null,
		changed_at: '2026-11-20T17:00:00Z'
	})
];

export const SUBJECT_DETAIL: SubjectDetail = {
	...SUBJECTS[0]!,
	text: 'Open Wright problems are ranked before they are investigated.',
	statements: [
		{
			statement_id: 8101,
			text: 'Rank open problems by how many sessions they touched.',
			carrier: lessonCarrier(2733, '2026-11-18T09:00:00Z'),
			state: 'current',
			resolved_by_statement_id: null,
			stated_at: '2026-11-18T09:00:00Z'
		},
		{
			statement_id: 8102,
			text: 'Rank open problems by the newest failure first.',
			carrier: lessonCarrier(2741, '2026-11-18T21:00:00Z'),
			state: 'current',
			resolved_by_statement_id: null,
			stated_at: '2026-11-18T21:00:00Z'
		},
		{
			statement_id: 8103,
			text: 'Order problems by count.',
			carrier: lessonCarrier(2700, '2026-11-10T09:00:00Z'),
			state: 'covered',
			resolved_by_statement_id: 8101,
			stated_at: '2026-11-10T09:00:00Z'
		},
		{
			statement_id: 8104,
			text: 'Investigate the newest problem first.',
			carrier: fileCarrier('claude-memory', 'memory/wright.md', '2026-11-01T09:00:00Z'),
			state: 'superseded',
			resolved_by_statement_id: 8102,
			stated_at: '2026-11-01T09:00:00Z'
		},
		{
			statement_id: 8105,
			text: 'A passage that lived in a section of wright.md before it was split again.',
			carrier: fileCarrier('claude-memory', 'memory/wright.md', '2026-10-20T09:00:00Z'),
			state: 'gone',
			resolved_by_statement_id: null,
			stated_at: '2026-10-20T09:00:00Z'
		}
	]
};

export const SUBJECT_DECISIONS: DecisionRow[] = [
	CONFLICTS[0]!,
	CHANGES[0]!,
	CHANGES[3]!,
	CHANGES[5]!
].map((row, i) => ({
	...row,
	subject_id: 41,
	subject_title: 'Ranking Wright problems',
	id: 970 + i
}));

export function capabilityFor(scenario: ConsolidationScenario): CapabilityRow | null {
	const base = { id: 'knowledge_consolidation', label: 'Knowledge consolidation', fix: '' };
	switch (scenario) {
		case 'off-never':
		case 'off-history':
			return {
				...base,
				state: 'off',
				detail: 'The knowledge_consolidation toggle is off.',
				fix: 'Turn on Knowledge consolidation at /settings/workers'
			};
		case 'first-pass':
			return {
				...base,
				state: 'active',
				detail: '14 files waiting to be split · 3 subjects waiting to be judged'
			};
		case 'degraded':
			return {
				...base,
				state: 'degraded',
				detail:
					'The last pass failed at 18:12: the consolidation model returned no answer. 2 subjects waiting to be judged',
				fix: 'Check the model named at /settings/workers; the worker retries on its own.'
			};
		case 'empty':
			return { ...base, state: 'active', detail: 'Nothing waiting.' };
		default:
			return { ...base, state: 'active', detail: 'Nothing waiting.' };
	}
}

export interface ConsolidationFixture {
	capability: CapabilityRow | null;
	measures: MeasureSeriesRow[];
	conflicts: DecisionRow[];
	changes: DecisionRow[];
	subjects: SubjectSummary[];
}

export function fixtureFor(scenario: ConsolidationScenario): ConsolidationFixture {
	const stored = scenario === 'off-never' || scenario === 'first-pass';
	const empty = scenario === 'empty';
	return {
		capability: capabilityFor(scenario),
		measures: empty
			? measures(3).map((m) => ({ ...m, statements_current: 0, subjects: 0 }))
			: stored
				? []
				: measures(),
		conflicts: stored || empty ? [] : structuredClone(CONFLICTS),
		changes: stored || empty ? [] : structuredClone(CHANGES),
		subjects: stored || empty ? [] : structuredClone(SUBJECTS)
	};
}

function json(body: unknown, status = 200): Promise<Response> {
	return Promise.resolve(
		new Response(JSON.stringify(body), {
			status,
			headers: { 'content-type': 'application/json' }
		})
	);
}

/**
 * Installs a fetch that answers the consolidation routes from a fixture, and applies undo,
 * apply and settle to it, so a story can press a button and watch the row change.
 */
export function stubConsolidationFetch(scenario: ConsolidationScenario): ConsolidationFixture {
	const fx = fixtureFor(scenario);
	globalThis.fetch = (async (input: RequestInfo | URL, init?: RequestInit) => {
		const raw =
			typeof input === 'string'
				? input
				: input instanceof URL
					? input.href
					: (input as Request).url;
		const url = new URL(raw, 'http://localhost');
		const path = url.pathname;
		const q = url.searchParams;
		const decision = (id: number): DecisionRow | undefined =>
			[...fx.changes, ...fx.conflicts].find((d) => d.id === id);

		if (path === '/api/capabilities')
			return json({ capabilities: fx.capability ? [fx.capability] : [] });
		if (path.endsWith('/measures')) return json({ project_id: 3, measures: fx.measures });
		if (path.endsWith('/subjects') && !path.match(/subjects\/\d+/))
			return json({ project_id: 3, subjects: fx.subjects });
		const subjectMatch = path.match(/\/subjects\/(\d+)$/);
		if (subjectMatch) {
			return Number(subjectMatch[1]) === SUBJECT_DETAIL.id
				? json(SUBJECT_DETAIL)
				: json({ error: `no subject ${subjectMatch[1]}` }, 404);
		}
		const act = path.match(/\/decisions\/(\d+)\/(undo|apply|settle)$/);
		if (act && init?.method === 'POST') {
			const row = decision(Number(act[1]));
			if (!row) return json({ error: 'no such decision' }, 404);
			if (act[2] === 'undo') row.application = 'undone';
			if (act[2] === 'apply') {
				row.application = 'applied';
				row.applied_at = '2026-11-20T16:50:00Z';
				row.error = null;
			}
			if (act[2] === 'settle') {
				fx.conflicts = fx.conflicts.filter((d) => d.id !== row.id);
				return json({ decision: row, conflict: row });
			}
			return json({ decision: row });
		}
		if (path.endsWith('/decisions')) {
			const verdicts = q.get('verdict')?.split(',');
			let rows =
				q.get('subject') !== null
					? SUBJECT_DECISIONS
					: q.get('open') === 'true'
						? fx.conflicts
						: fx.changes;
			if (verdicts && q.get('open') !== 'true')
				rows = rows.filter((d) => verdicts.includes(d.verdict));
			return json({ project_id: 3, decisions: rows, next_before: null });
		}
		return json({});
	}) as typeof fetch;
	return fx;
}
