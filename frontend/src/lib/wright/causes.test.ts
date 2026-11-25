import { describe, expect, it } from 'vitest';
import {
	normaliseCause,
	normaliseProblem,
	type WireProblem,
	type WrightCause
} from '$lib/api/wright';
import {
	byPriorityDesc,
	canMove,
	causeArea,
	causeLinks,
	causeState,
	dropStep,
	engineerLine,
	isMoved,
	moveStep,
	openFor,
	priorityForIndex,
	queryTables,
	scoreBreakdown,
	sectionsByArea
} from './causes';

const NOW = Date.parse('2026-11-17T12:00:00Z');
const hoursAgo = (h: number): string => new Date(NOW - h * 3_600_000).toISOString();

function cause(over: Partial<WrightCause> = {}): WrightCause {
	return {
		cause_id: 1,
		problem_id: 7,
		check_name: 'slow_queries',
		resource: 'query 1',
		statement: null,
		problems: 1,
		observations: 1,
		opened_at: hoursAgo(1),
		attempts: 0,
		last_attempt_at: null,
		finding_id: null,
		followup_id: null,
		issue_number: null,
		remedied: false,
		priority: 1,
		notified_at: null,
		commented_at: null,
		pinned: false,
		pinned_at: null,
		operator_priority: null,
		rank_priority: over.operator_priority ?? over.priority ?? 1,
		group_id: null,
		group_name: null,
		absorbed: [],
		investigable: true,
		not_investigated_because: null,
		...over
	};
}

const LONG_QUERY =
	'query 6512464039861100498: WITH own_turn AS ( SELECT max(t.id) AS id FROM memory.operator_turns t WHERE t.session_id = $1 ), prior AS ( SELECT p.id FROM memory.operator_turns p JOIN own_turn o ON p.id < o.id JOIN wright.problems w ON w.id = p.id ) SELECT * FROM prior';

describe('causeState', () => {
	it('names the answer a cause has, finding first', () => {
		expect(causeState(cause({ finding_id: 2, followup_id: 3, remedied: true }), 6, NOW)).toBe(
			'found'
		);
		expect(causeState(cause({ followup_id: 3, remedied: true }), 6, NOW)).toBe('filed');
		expect(causeState(cause({ remedied: true, opened_at: hoursAgo(40) }), 6, NOW)).toBe(
			'known_fix'
		);
	});

	it('is waiting only once nothing has answered it for the waited hours', () => {
		expect(causeState(cause({ opened_at: hoursAgo(7) }), 6, NOW)).toBe('waiting');
		expect(causeState(cause({ opened_at: hoursAgo(5) }), 6, NOW)).toBe('queued');
	});
});

describe('causeLinks', () => {
	it('links the finding, the issue on GitHub and the representative problem', () => {
		expect(causeLinks(cause({ finding_id: 5, issue_number: 272 }))).toEqual([
			{ kind: 'finding', text: '5', href: '#finding-5' },
			{
				kind: 'issue',
				text: '#272',
				href: 'https://github.com/temet-ai/mike/issues/272',
				external: true
			},
			{ kind: 'problem', text: '7', href: '#problem-7' }
		]);
	});
});

describe('openFor', () => {
	it('uses the largest whole unit', () => {
		expect(openFor(hoursAgo(0.5), NOW)).toBe('Open 30m');
		expect(openFor(hoursAgo(26), NOW)).toBe('Open 26h');
		expect(openFor(hoursAgo(72), NOW)).toBe('Open 3d');
	});
});

describe('byPriorityDesc', () => {
	it('ranks the higher rank_priority first and breaks ties on the cause id', () => {
		const rows = [
			cause({ cause_id: 3, rank_priority: 1 }),
			cause({ cause_id: 2, rank_priority: 5 }),
			cause({ cause_id: 1, rank_priority: 5 })
		];
		expect([...rows].sort(byPriorityDesc).map((c) => c.cause_id)).toEqual([1, 2, 3]);
	});

	it('puts every pinned cause before the unpinned ones, then orders each by rank_priority', () => {
		const rows = [
			cause({ cause_id: 1, rank_priority: 99 }),
			cause({ cause_id: 2, rank_priority: 3, pinned: true }),
			cause({ cause_id: 3, rank_priority: 8, pinned: true }),
			cause({ cause_id: 4, rank_priority: 50 })
		];
		expect([...rows].sort(byPriorityDesc).map((c) => c.cause_id)).toEqual([3, 2, 1, 4]);
	});

	it('uses the operator value, not the formula, when the cause was moved', () => {
		const moved = cause({ cause_id: 2, priority: 5, operator_priority: 40, rank_priority: 40 });
		const other = cause({ cause_id: 1, priority: 30, rank_priority: 30 });
		expect([other, moved].sort(byPriorityDesc).map((c) => c.cause_id)).toEqual([2, 1]);
		expect(isMoved(moved)).toBe(true);
		expect(isMoved(other)).toBe(false);
	});
});

describe('causeArea', () => {
	it('names a query by the schema of its first table, whichever check found it', () => {
		for (const check of ['slow_queries', 'reads_outpace_writes', 'idle_polls']) {
			const area = causeArea(cause({ check_name: check, resource: LONG_QUERY }));
			expect(area.key).toBe('tables:memory');
			expect(area.label).toBe('memory tables');
		}
	});

	it('lists every schema.table the query names and skips aliases and CTE names', () => {
		expect(causeArea(cause({ resource: LONG_QUERY })).detail).toEqual([
			'memory.operator_turns',
			'wright.problems'
		]);
		expect(queryTables('update wright.causes set x = 1')).toEqual(['wright.causes']);
		expect(queryTables('INSERT INTO "steering"."interventions" (a) VALUES (1)')).toEqual([
			'steering.interventions'
		]);
		expect(queryTables('select t.id from prior t')).toEqual([]);
	});

	it('does not take the FROM of EXTRACT, SUBSTRING, TRIM or OVERLAY for a table', () => {
		expect(
			queryTables('SELECT EXTRACT(epoch FROM t.created_at) AS s FROM wright.problems t')
		).toEqual(['wright.problems']);
		expect(
			queryTables(
				'select substring(p.text from x.y for 3), trim(both $1 from p.name) from memory.turns p'
			)
		).toEqual(['memory.turns']);
		expect(
			queryTables('SELECT TRIM(LEADING FROM t.name), OVERLAY(a.b PLACING c FROM d.e) FROM prior t')
		).toEqual([]);
		expect(queryTables('SELECT extract ( day FROM  t.opened ) FROM sessions.sessions t')).toEqual([
			'sessions.sessions'
		]);
	});

	it('still reads a table named in a subquery inside one of those functions', () => {
		expect(
			queryTables(
				'SELECT EXTRACT(epoch FROM (SELECT max(x.at) FROM memory.events x)) FROM wright.causes c'
			)
		).toEqual(['memory.events', 'wright.causes']);
	});

	it('does not take IS DISTINCT FROM for a table', () => {
		expect(
			queryTables(
				'SELECT 1 FROM wright.causes c WHERE c.a IS DISTINCT FROM p.b AND c.x IS NOT DISTINCT FROM q.y'
			)
		).toEqual(['wright.causes']);
		expect(queryTables('SELECT 1 FROM prior c WHERE c.a IS DISTINCT FROM p.b')).toEqual([]);
	});

	it('does not read a table from a string literal or a comment', () => {
		expect(
			queryTables(
				"SELECT 1 FROM wright.causes WHERE note = 'copied from a.b, it''s join c.d' /* from e.f */ -- from g.h"
			)
		).toEqual(['wright.causes']);
	});

	it('names the area of a long full statement whose first table is far into the text', () => {
		const columns = Array.from({ length: 30 }, (_, i) => `COALESCE(t.col_${i}, 0) AS c${i}`).join(
			', '
		);
		const sql = `query 42: WITH own_turn AS ( SELECT ${columns}, EXTRACT(epoch FROM t.created_at) AS age FROM prior t ) SELECT * FROM own_turn o JOIN mailbox.messages m ON m.id = o.c0 JOIN steering.interventions i ON i.id = m.id`;
		expect(sql.search(/mailbox\.messages/)).toBeGreaterThan(200);
		const area = causeArea(cause({ check_name: 'idle_polls', resource: sql }));
		expect(area).toMatchObject({
			kind: 'tables',
			label: 'mailbox tables',
			detail: ['mailbox.messages', 'steering.interventions']
		});
	});

	it('reads the area from the whole statement when the resource was cut before its table', () => {
		const text = `SELECT ${'a, '.repeat(60)}x FROM mailbox.messages m JOIN steering.interventions i ON i.id = m.id`;
		const cut = `query 42: ${text.slice(0, 120)}`;
		expect(causeArea(cause({ check_name: 'idle_polls', resource: cut }))).toMatchObject({
			kind: 'check',
			label: 'idle_polls'
		});
		const area = causeArea(cause({ check_name: 'idle_polls', resource: cut, statement: text }));
		expect(area).toMatchObject({
			kind: 'tables',
			label: 'mailbox tables',
			detail: ['mailbox.messages', 'steering.interventions']
		});
	});

	it('puts two checks against the same schema in one area', () => {
		const slow = causeArea(
			cause({ check_name: 'slow_queries', resource: 'query 1: SELECT * FROM wright.problems' })
		);
		const idle = causeArea(
			cause({ check_name: 'idle_polls', resource: 'query 2: UPDATE wright.causes SET a = 1' })
		);
		expect(slow.key).toBe(idle.key);
		expect(slow.label).toBe('wright tables');
	});

	it('falls back to the check name when a query names no qualified table', () => {
		const area = causeArea(cause({ check_name: 'slow_queries', resource: 'query 1: SELECT 1' }));
		expect(area).toMatchObject({ kind: 'check', label: 'slow_queries' });
	});

	it('names a log error by the first three segments of its module path', () => {
		const area = causeArea(
			cause({
				check_name: 'log_errors',
				resource: 'mike_features::proxy::responses_capture::record: bad frame'
			})
		);
		expect(area).toMatchObject({
			kind: 'module',
			label: 'mike_features::proxy::responses_capture',
			detail: ['mike_features::proxy::responses_capture::record']
		});
		expect(causeArea(cause({ check_name: 'log_errors', resource: 'plain text error' })).label).toBe(
			'log_errors'
		);
	});

	it('names a failed scheduled job by its name', () => {
		const area = causeArea(
			cause({
				check_name: 'scheduled_job_failures',
				resource: 'job:decisions_loop@3dcd7aa2-3126-4122-84e0-051a00751c02'
			})
		);
		expect(area).toMatchObject({ kind: 'job', label: 'decisions_loop' });
	});

	it('calls every decisions_health cause "decisions" and any other check by its name', () => {
		expect(
			causeArea(cause({ check_name: 'decisions_health', resource: 'mail_reply_link' })).label
		).toBe('decisions');
		expect(causeArea(cause({ check_name: 'undelivered_mail', resource: 'x' })).label).toBe(
			'undelivered_mail'
		);
	});
});

/** Rows as the running mike sent them on 2026-11-24: no operator fields, the resource cut at about 148 characters. */
const REAL_ROWS = [
	{
		attempts: 0,
		cause_id: 19,
		check_name: 'slow_queries',
		commented_at: '2026-11-19T05:06:01.289085Z',
		finding_id: null,
		followup_id: null,
		issue_number: null,
		last_attempt_at: null,
		notified_at: '2026-11-18T01:22:33.379923Z',
		observations: 184,
		opened_at: '2026-11-23T22:08:26.002558Z',
		priority: 7.61,
		problem_id: 78184,
		problems: 2,
		remedied: false,
		resource:
			'query -7049553398043295787: WITH own_turn AS ( SELECT max(t.id) AS id FROM memory.operator_turns t JOIN sessions.sessions s ON s.id = t.session_id W'
	},
	{
		attempts: 0,
		cause_id: 404,
		check_name: 'slow_queries',
		commented_at: null,
		finding_id: null,
		followup_id: null,
		issue_number: null,
		last_attempt_at: null,
		notified_at: null,
		observations: 34,
		opened_at: '2026-11-24T04:38:11.495056Z',
		priority: 4.67,
		problem_id: 79928,
		problems: 1,
		remedied: false,
		resource:
			'query 9062887631660520957: SELECT r.id, r.request_body AS "request_body!", (r.filtered_request_body IS NOT NULL) AS "has_filtered_body!" FROM reque'
	}
];

describe('normaliseCause', () => {
	it('fills the operator fields a running mike that has not deployed them leaves out', () => {
		const row = normaliseCause(REAL_ROWS[0]!);
		expect(row).toMatchObject({
			pinned: false,
			pinned_at: null,
			operator_priority: null,
			rank_priority: 7.61,
			group_id: null,
			group_name: null,
			absorbed: [],
			investigable: null,
			not_investigated_because: null
		});
		// The engineer line stays silent rather than claim the engineer will take it.
		expect(engineerLine(row, null)).toBeNull();
	});

	it('reads the statement as null when absent and keeps it when the server sent one', () => {
		expect(normaliseCause(REAL_ROWS[0]!).statement).toBeNull();
		const whole = 'SELECT 1 FROM wright.problems';
		expect(normaliseCause({ ...REAL_ROWS[0]!, statement: whole }).statement).toBe(whole);
		expect(normaliseCause({ ...REAL_ROWS[0]!, statement: null }).statement).toBeNull();
	});

	it('keeps what the server sent, and ranks by the operator value when there is one', () => {
		const row = normaliseCause({
			...REAL_ROWS[0]!,
			pinned: true,
			operator_priority: 40
		});
		expect(row.pinned).toBe(true);
		expect(row.rank_priority).toBe(40);
	});

	it('names an area when the resource still holds the table, and the check when it was cut short', () => {
		expect(causeArea(normaliseCause(REAL_ROWS[0]!))).toMatchObject({
			label: 'memory tables',
			detail: ['memory.operator_turns', 'sessions.sessions']
		});
		expect(causeArea(normaliseCause(REAL_ROWS[1]!))).toMatchObject({
			kind: 'check',
			label: 'slow_queries'
		});
	});
});

describe('normaliseProblem', () => {
	const wire: WireProblem = {
		id: 4,
		check_name: 'slow_queries',
		scope: 'query',
		source: 'capture_db',
		resource: 'query 1: SELECT 1',
		state: 'open',
		opened_at: '2026-11-24T10:00:00Z',
		last_seen_at: '2026-11-24T11:00:00Z',
		healthy_at: null,
		episodes: 1,
		latest_evidence: {}
	};

	it('reads the statement as null when a running mike does not send it', () => {
		expect(normaliseProblem(wire).statement).toBeNull();
	});

	it('keeps the whole statement the server sent', () => {
		const whole = `SELECT ${'x, '.repeat(9000)}y FROM wright.problems`;
		expect(normaliseProblem({ ...wire, statement: whole }).statement).toBe(whole);
	});
});

describe('sectionsByArea', () => {
	const rows = [
		cause({ cause_id: 1, check_name: 'decisions_health', resource: 'a', rank_priority: 9 }),
		cause({
			cause_id: 2,
			check_name: 'slow_queries',
			resource: 'query 1: SELECT * FROM wright.problems',
			rank_priority: 20
		}),
		cause({
			cause_id: 3,
			check_name: 'idle_polls',
			resource: 'query 2: SELECT * FROM wright.causes',
			rank_priority: 15
		}),
		cause({
			cause_id: 4,
			check_name: 'undelivered_mail',
			resource: 'x',
			rank_priority: 1,
			group_id: 7,
			group_name: 'Mail'
		}),
		cause({ cause_id: 5, check_name: 'decisions_health', resource: 'b', rank_priority: 30 })
	];

	it('lists named groups first, then areas, each section in rank order', () => {
		const sections = sectionsByArea([...rows].sort(byPriorityDesc));
		expect(sections.map((s) => [s.kind, s.label, s.causes.map((c) => c.cause_id)])).toEqual([
			['group', 'Mail', [4]],
			['area', 'decisions', [5, 1]],
			['area', 'wright tables', [2, 3]]
		]);
	});

	it('orders sections by their highest-ranked cause, pinned first', () => {
		const pinned = rows.map((c) => (c.cause_id === 3 ? { ...c, pinned: true } : c));
		expect(
			sectionsByArea([...pinned].sort(byPriorityDesc))
				.filter((s) => s.kind === 'area')
				.map((s) => s.label)
		).toEqual(['wright tables', 'decisions']);
	});

	it('keeps the order it is given, so a refresh in place does not reorder a section', () => {
		const given = [rows[2]!, rows[1]!, rows[0]!, rows[4]!];
		expect(sectionsByArea(given)[0]!.causes.map((c) => c.cause_id)).toEqual([3, 2]);
	});

	it('shows a grouped cause only in its group', () => {
		const all = sectionsByArea(rows).flatMap((s) => s.causes.map((c) => c.cause_id));
		expect(all.sort()).toEqual([1, 2, 3, 4, 5]);
	});
});

describe('scoreBreakdown', () => {
	it('lists the inputs and the formula score, and the operator value when set', () => {
		const plain = scoreBreakdown(cause({ problems: 3, observations: 40, priority: 9.4 }), NOW);
		expect(plain.map((l) => l.label)).toEqual([
			'Problems',
			'Times seen',
			'Days open',
			'Formula score'
		]);
		expect(plain[1]!.value).toBe('40 (adds 3.7)');
		expect(plain[3]!.value).toBe('9.4');
		const moved = scoreBreakdown(cause({ operator_priority: 12, rank_priority: 12 }), NOW);
		expect(moved.at(-1)).toEqual({ label: 'Your value', value: '12, used instead' });
	});

	it('counts days open from when the cause opened', () => {
		const lines = scoreBreakdown(cause({ opened_at: hoursAgo(36) }), NOW);
		expect(lines.find((l) => l.label === 'Days open')!.value).toBe('1.5');
	});
});

describe('engineerLine', () => {
	it('says the engineer takes an investigable cause, and that a pin puts it first', () => {
		expect(engineerLine(cause(), 3)).toBe(
			'The engineer takes it when it reaches the top of the queue.'
		);
		expect(engineerLine(cause({ pinned: true }), 3)).toBe(
			'Pinned. The engineer takes it before the unpinned causes.'
		);
	});

	it('gives the reason the engineer will not take a cause, naming the attempt limit', () => {
		const no = (because: NonNullable<WrightCause['not_investigated_because']>, pinned = false) =>
			engineerLine(cause({ investigable: false, not_investigated_because: because, pinned }), 3);
		expect(no('finding', true)).toBe(
			'Pinned. The engineer will not take it: it already has a finding.'
		);
		expect(no('followup')).toBe(
			'The engineer will not take it: a follow-up is already filed for it.'
		);
		expect(no('known_fix')).toBe('The engineer will not take it: a known fix already answers it.');
		expect(no('attempts')).toBe(
			'The engineer will not take it: it has used all 3 of its attempts.'
		);
	});

	it('says nothing while the server has not said', () => {
		expect(
			engineerLine(cause({ investigable: null, not_investigated_because: null }), null)
		).toBeNull();
	});
});

describe('reordering', () => {
	const rank = (id: number, rank_priority: number, pinned = false) =>
		cause({ cause_id: id, rank_priority, priority: rank_priority, pinned });
	const list = [rank(1, 30), rank(2, 20), rank(3, 10)];

	it('puts a moved cause midway between its new neighbours', () => {
		expect(moveStep(list, 3, 'up')).toBe(25);
		expect(moveStep(list, 1, 'down')).toBe(15);
	});

	it('goes one past the top or the bottom when it leaves the list end', () => {
		expect(priorityForIndex(list, 3, 0)).toBe(31);
		expect(priorityForIndex(list, 1, 2)).toBe(9);
	});

	it('does nothing at the edge of the list or when the cause is already there', () => {
		expect(canMove(list, 1, 'up')).toBe(false);
		expect(moveStep(list, 1, 'up')).toBeNull();
		expect(canMove(list, 3, 'down')).toBe(false);
		expect(priorityForIndex(list, 2, 1)).toBeNull();
	});

	it('moves only inside the pin state of the cause', () => {
		const mixed = [rank(1, 50, true), rank(2, 40, true), rank(3, 60), rank(4, 10)].sort(
			byPriorityDesc
		);
		expect(canMove(mixed, 2, 'down')).toBe(false);
		expect(canMove(mixed, 3, 'up')).toBe(false);
		expect(moveStep(mixed, 2, 'up')).toBe(51);
		expect(moveStep(mixed, 3, 'down')).toBe(9);
	});

	it('drops a cause before or after the one it lands on', () => {
		expect(dropStep(list, 3, 1, 'before')).toBe(31);
		expect(dropStep(list, 3, 1, 'after')).toBe(25);
		expect(dropStep(list, 1, 3, 'after')).toBe(9);
		expect(dropStep(list, 1, 3, 'before')).toBe(15);
		expect(dropStep(list, 1, 2, 'before')).toBeNull();
		expect(dropStep(list, 1, 1, 'after')).toBeNull();
		expect(dropStep(list, 2, 1, 'after')).toBeNull();
	});

	it('refuses a drop across the pin edge', () => {
		const mixed = [rank(1, 50, true), rank(2, 40)];
		expect(dropStep(mixed, 2, 1, 'before')).toBeNull();
	});

	it('keeps the order the move was asked for', () => {
		const target = moveStep(list, 3, 'up')!;
		const after = list.map((c) =>
			c.cause_id === 3 ? { ...c, operator_priority: target, rank_priority: target } : c
		);
		expect([...after].sort(byPriorityDesc).map((c) => c.cause_id)).toEqual([1, 3, 2]);
	});
});
