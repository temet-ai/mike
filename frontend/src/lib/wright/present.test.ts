import { describe, expect, it } from 'vitest';
import type { WrightAction, WrightRemedy } from '$lib/api/wright';
import { problemFixture } from '$lib/components/__fixtures__/wright';
import { mergeInPlace, withHeld } from './feed';
import { hashWithoutPair, pairTokenFromHash } from './pairing';
import {
	dayLabel,
	describeSteer,
	findingEvidenceRef,
	fixVerdict,
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
		name: 'close_orphan_launch',
		kind: 'tidy',
		action: 'close_orphan_launch',
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
			describeSteer({ type: 'pause_remedy', name: 'close_orphan_launch', until: 'tomorrow' })
		).toMatchObject({
			icon: 'pause',
			title: 'Paused: Close orphan launch',
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
