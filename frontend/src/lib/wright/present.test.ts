import { describe, expect, it } from 'vitest';
import type { WrightAction, WrightRemedy } from '$lib/api/wright';
import {
	followupFixture,
	followupLimitsFixture,
	problemFixture
} from '$lib/components/__fixtures__/wright';
import { mergeInPlace, withHeld } from './feed';
import { hashWithoutPair, pairTokenFromHash } from './pairing';
import {
	dayLabel,
	describeSteer,
	findingEvidenceRef,
	fixVerdict,
	followupBadge,
	followupKind,
	followupLine,
	followupMeta,
	followupTooltip,
	problemLinks,
	problemTitle,
	repairSteps
} from './present';

describe('pairing fragment', () => {
	it('reads the token and keeps other fragment parts', () => {
		expect(pairTokenFromHash('#pair=abc123')).toBe('abc123');
		expect(pairTokenFromHash('#problem-3&pair=abc')).toBe('abc');
		expect(pairTokenFromHash('#problem-3')).toBeNull();
		expect(pairTokenFromHash('#pair=')).toBeNull();
		expect(hashWithoutPair('#problem-3&pair=abc')).toBe('#problem-3');
		expect(hashWithoutPair('#pair=abc')).toBe('');
	});
});

describe('problem links', () => {
	it('links the request, the tool and the log, and names the problem', () => {
		const links = problemLinks(
			problemFixture({
				latest_evidence: { last_request_id: 5, tool: 'Edit', file: '/log/mike.log' }
			})
		);
		expect(links.map((l) => l.text)).toEqual([
			'Problem 707',
			'Request 5',
			'Tool errors for Edit',
			'Log /log/mike.log'
		]);
		expect(links[1]?.href).toBe('/api/requests/5');
		expect(links[2]?.href).toBe('/tools?tool_name=Edit');
		expect(links[3]?.href).toBeUndefined();
	});

	it('links a request into its session when the evidence names one', () => {
		const sid = '3dcd7aa2-3126-4122-84e0-051a00751c02';
		const links = problemLinks(
			problemFixture({ latest_evidence: { last_request_id: 5, session_id: sid } })
		);
		expect(links.find((l) => l.text === 'Request 5')?.href).toBe(`/sessions/${sid}/requests/5`);
	});

	it('titles a known check plainly and an unknown one from its name', () => {
		expect(problemTitle('undelivered_mail')).toBe('Mail waiting for a seat that is not reading');
		expect(problemTitle('some_new_check')).toBe('Some new check');
	});
});

describe('fix verdict', () => {
	const remedy: WrightRemedy = {
		name: 'tell_undelivered_senders',
		kind: 'notice',
		action: 'notify_undelivered_sender',
		check_name: 'x',
		enabled: true,
		daily_limit: 5,
		max_resources: 3,
		paused_at: null,
		pause_reason: null,
		actions_today: 0,
		ineffective_runs: 0
	};
	const action = (state: WrightAction['state']): WrightAction => ({
		id: Math.random(),
		remedy: remedy.name,
		problem_id: 1,
		state,
		by: 'wright',
		created_at: '2026-11-14T10:00:00Z',
		verified_at: null,
		effect: null,
		can_undo: false
	});

	it('is better when its actions cleared the problem and measuring before any', () => {
		expect(fixVerdict(remedy, [action('verified')]).label).toBe('Better');
		expect(fixVerdict(remedy, []).label).toBe('Measuring');
		expect(fixVerdict(remedy, [action('ineffective')]).label).toBe('No change');
	});

	it('says why a paused or turned-off fix is not acting', () => {
		const paused = fixVerdict({ ...remedy, paused_at: 'x', pause_reason: 'three refusals' }, []);
		expect(paused).toMatchObject({ label: 'Paused', line: 'three refusals' });
		expect(fixVerdict({ ...remedy, enabled: false, ineffective_runs: 3 }, []).label).toBe(
			'No change · off'
		);
	});
});

describe('repair steps', () => {
	it('walks prepared to healthy and stops at a failed gate', () => {
		expect(repairSteps('landed').map((s) => s.status)).toEqual([
			'done',
			'done',
			'current',
			'todo',
			'todo'
		]);
		expect(repairSteps('gate_failed').map((s) => s.status)).toEqual([
			'done',
			'current',
			'todo',
			'todo',
			'todo'
		]);
	});
});

describe('steers and evidence refs', () => {
	it('describes the closed set of steers', () => {
		expect(
			describeSteer({ type: 'pause_remedy', name: 'tell_undelivered_senders', until: 'tomorrow' })
		).toMatchObject({
			icon: 'pause',
			title: 'Paused: Tell undelivered senders',
			detail: 'Until tomorrow'
		});
		expect(describeSteer({ type: 'surprise' }).icon).toBe('other');
	});

	it('links sessions, requests and problems, and leaves files as labels', () => {
		expect(findingEvidenceRef('session', 'abc').href).toBe('/sessions/abc');
		expect(findingEvidenceRef('problem', '4').href).toBe('#problem-4');
		expect(findingEvidenceRef('file', 'a.rs').href).toBeUndefined();
	});
});

describe('feed merge', () => {
	it('updates rows in place and holds the rows that are not on screen', () => {
		const current = [
			{ id: 2, v: 'a' },
			{ id: 1, v: 'a' }
		];
		const fetched = [
			{ id: 3, v: 'n' },
			{ id: 2, v: 'b' },
			{ id: 1, v: 'a' }
		];
		const merged = mergeInPlace(current, fetched, (r) => r.id);
		expect(merged.rows).toEqual([
			{ id: 2, v: 'b' },
			{ id: 1, v: 'a' }
		]);
		expect(merged.held).toEqual([{ id: 3, v: 'n' }]);
		expect(withHeld(merged.rows, merged.held, (a, b) => b.id - a.id).map((r) => r.id)).toEqual([
			3, 2, 1
		]);
	});
});

describe('day labels', () => {
	it('says today, yesterday, then the date', () => {
		const now = new Date(2026, 10, 14, 12, 0).getTime();
		expect(dayLabel(new Date(2026, 10, 14, 9, 0).toISOString(), now)).toBe('Today');
		expect(dayLabel(new Date(2026, 10, 13, 9, 0).toISOString(), now)).toBe('Yesterday');
		expect(dayLabel(new Date(2026, 9, 15, 9, 0).toISOString(), now)).toBe('15 Oct');
	});
});

describe('follow-ups', () => {
	const limits = followupLimitsFixture({ max_attempts: 3 });

	it('names each state and its badge', () => {
		expect(followupBadge('open')).toEqual({ label: 'Not filed', variant: 'chip-muted' });
		expect(followupBadge('handed_off').label).toBe('Being fixed');
		expect(followupBadge('stuck').variant).toBe('chip-warning');
		expect(followupBadge('fixed').variant).toBe('chip-active');
	});

	it('labels the kind, falling back to the code for a new one', () => {
		expect(followupKind('root_cause')).toBe('Why it keeps happening');
		expect(followupKind('something_new' as never)).toBe('Something new');
	});

	it('builds the meta line and leaves out an empty owner', () => {
		expect(followupMeta(followupFixture({ owning_part: 'crates/x' }))).toBe(
			'Code bug · A tool called with the wrong inputs · owned by crates/x'
		);
		expect(followupMeta(followupFixture({ owning_part: null, check_name: null }))).toBe('Code bug');
	});

	it('says the attempt limit once it is known', () => {
		const f = followupFixture({
			state: 'handed_off',
			attempts: 2,
			fixer_machine: 'omit',
			handed_off_at: null
		});
		expect(followupLine(f, limits)).toBe('omit, attempt 2 of 3.');
		expect(followupLine(f, null)).toBe('omit, attempt 2.');
	});

	it('writes the state line for each state', () => {
		expect(followupLine(followupFixture({ state: 'open' }))).toBe('No issue yet.');
		expect(followupLine(followupFixture({ state: 'filed', issue_number: 9 }))).toBe(
			'Issue #9 is open. No session has picked it up.'
		);
		expect(followupLine(followupFixture({ state: 'stuck', attempts: 3 }))).toBe(
			'3 sessions ended without a fix. Wright has stopped trying.'
		);
		expect(followupLine(followupFixture({ state: 'stuck', attempts: 1, issue_url: null }))).toBe(
			'1 session ended without a fix. Wright has stopped trying. No issue was filed.'
		);
		expect(
			followupLine(
				followupFixture({ state: 'dropped', closed_at: null, closed_reason: 'not_planned' })
			)
		).toBe('Closed: Not planned.');
	});

	it('explains Dropped as closed as not planned and names the daily caps', () => {
		expect(followupTooltip('dropped')).toBe(
			'The GitHub issue was closed as not planned, by the fixer session or by anyone.'
		);
		expect(followupTooltip('open', limits)).toContain('at most 3 a day');
		expect(followupTooltip('filed', limits)).toContain('at most 2 a day');
		expect(followupTooltip('open')).not.toContain('at most');
	});
});
