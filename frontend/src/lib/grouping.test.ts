import { describe, it, expect } from 'vitest';
import {
	groupByProject,
	selectKickoffRequest,
	isLocalCommandOnlyPrompt,
	type KickoffCandidate
} from './grouping';
import { buildSessionSummary } from '$lib/components/__fixtures__/session';

describe('groupByProject', () => {
	it('returns an empty array for empty input', () => {
		expect(groupByProject([])).toEqual([]);
	});

	it('groups sessions under a single project', () => {
		const sessions = [
			buildSessionSummary({ id: 'a', project: 'alpha' }),
			buildSessionSummary({ id: 'b', project: 'alpha' })
		];
		const groups = groupByProject(sessions);
		expect(groups).toHaveLength(1);
		expect(groups[0]!.project).toBe('alpha');
		expect(groups[0]!.sessions).toHaveLength(2);
	});

	it('sorts multiple projects by max first_seen (newest-started project first)', () => {
		const sessions = [
			buildSessionSummary({ id: 'a', project: 'zulu', first_seen: '2026-04-10T08:00:00Z' }),
			buildSessionSummary({ id: 'b', project: 'alpha', first_seen: '2026-04-10T14:00:00Z' }),
			buildSessionSummary({ id: 'c', project: 'mike', first_seen: '2026-04-10T11:00:00Z' })
		];
		const groups = groupByProject(sessions);
		expect(groups.map((g) => g.project)).toEqual(['alpha', 'mike', 'zulu']);
	});

	it('group order is INVARIANT when only last_seen changes', () => {
		const base = [
			buildSessionSummary({
				id: 'a',
				project: 'zulu',
				first_seen: '2026-04-10T08:00:00Z',
				last_seen: '2026-04-10T08:30:00Z'
			}),
			buildSessionSummary({
				id: 'b',
				project: 'alpha',
				first_seen: '2026-04-10T14:00:00Z',
				last_seen: '2026-04-10T14:30:00Z'
			}),
			buildSessionSummary({
				id: 'c',
				project: 'alpha',
				first_seen: '2026-04-10T09:00:00Z',
				last_seen: '2026-04-10T09:30:00Z'
			})
		];
		const before = groupByProject(base);
		const beforeOrder = before.map((g) => ({
			project: g.project,
			ids: g.sessions.map((s) => s.id)
		}));

		const mutated = [
			{ ...base[0]!, last_seen: '2026-04-11T23:00:00Z' },
			{ ...base[1]!, last_seen: '2026-04-11T20:00:00Z' },
			{ ...base[2]!, last_seen: '2026-04-11T22:00:00Z' }
		];
		const after = groupByProject(mutated);
		const afterOrder = after.map((g) => ({
			project: g.project,
			ids: g.sessions.map((s) => s.id)
		}));

		expect(afterOrder).toEqual(beforeOrder);
		expect(afterOrder).toEqual([
			{ project: 'alpha', ids: ['b', 'c'] },
			{ project: 'zulu', ids: ['a'] }
		]);
	});

	it('falls back to alphabetical order when projects share the same max first_seen', () => {
		const ts = '2026-04-10T12:00:00Z';
		const sessions = [
			buildSessionSummary({ id: 'a', project: 'zulu', first_seen: ts }),
			buildSessionSummary({ id: 'b', project: 'alpha', first_seen: ts }),
			buildSessionSummary({ id: 'c', project: 'mike', first_seen: ts })
		];
		const groups = groupByProject(sessions);
		expect(groups.map((g) => g.project)).toEqual(['alpha', 'mike', 'zulu']);
	});

	it("uses each project's max first_seen for cross-project ordering", () => {
		const sessions = [
			buildSessionSummary({ id: 'a', project: 'alpha', first_seen: '2026-04-10T12:00:00Z' }),
			buildSessionSummary({ id: 'b', project: 'zulu', first_seen: '2026-04-10T08:00:00Z' }),
			buildSessionSummary({ id: 'c', project: 'zulu', first_seen: '2026-04-10T20:00:00Z' })
		];
		const groups = groupByProject(sessions);
		expect(groups.map((g) => g.project)).toEqual(['zulu', 'alpha']);
	});

	it('places null-project sessions in "Ungrouped" last', () => {
		const sessions = [
			buildSessionSummary({ id: 'a', project: null }),
			buildSessionSummary({ id: 'b', project: 'alpha' })
		];
		const groups = groupByProject(sessions);
		expect(groups).toHaveLength(2);
		expect(groups[0]!.project).toBe('alpha');
		expect(groups[1]!.project).toBe('Ungrouped');
		expect(groups[1]!.sessions[0]!.id).toBe('a');
	});

	it('handles mixed null and named projects correctly', () => {
		const sessions = [
			buildSessionSummary({ id: 'a', project: 'beta' }),
			buildSessionSummary({ id: 'b', project: null }),
			buildSessionSummary({ id: 'c', project: 'alpha' }),
			buildSessionSummary({ id: 'd', project: null })
		];
		const groups = groupByProject(sessions);
		expect(groups).toHaveLength(3);
		expect(groups[0]!.project).toBe('alpha');
		expect(groups[0]!.sessions).toHaveLength(1);
		expect(groups[1]!.project).toBe('beta');
		expect(groups[1]!.sessions).toHaveLength(1);
		expect(groups[2]!.project).toBe('Ungrouped');
		expect(groups[2]!.sessions).toHaveLength(2);
	});

	it('sorts sessions within a group newest-started-first by first_seen', () => {
		const sessions = [
			buildSessionSummary({ id: 'old', project: 'proj', first_seen: '2026-04-10T08:00:00Z' }),
			buildSessionSummary({ id: 'new', project: 'proj', first_seen: '2026-04-10T14:00:00Z' }),
			buildSessionSummary({ id: 'mid', project: 'proj', first_seen: '2026-04-10T11:00:00Z' })
		];
		const groups = groupByProject(sessions);
		expect(groups[0]!.sessions.map((s) => s.id)).toEqual(['new', 'mid', 'old']);
	});

	it('within-group order is INVARIANT when only last_seen changes', () => {
		const sessions = [
			buildSessionSummary({
				id: 'old',
				project: 'proj',
				first_seen: '2026-04-10T08:00:00Z',
				last_seen: '2026-04-11T23:00:00Z'
			}),
			buildSessionSummary({
				id: 'new',
				project: 'proj',
				first_seen: '2026-04-10T14:00:00Z',
				last_seen: '2026-04-11T01:00:00Z'
			}),
			buildSessionSummary({
				id: 'mid',
				project: 'proj',
				first_seen: '2026-04-10T11:00:00Z',
				last_seen: '2026-04-11T12:00:00Z'
			})
		];
		const groups = groupByProject(sessions);
		expect(groups[0]!.sessions.map((s) => s.id)).toEqual(['new', 'mid', 'old']);
	});

	it('buckets project names case-insensitively, preserving first-seen casing', () => {
		const sessions = [
			buildSessionSummary({ id: 'a', project: 'Alpha' }),
			buildSessionSummary({ id: 'b', project: 'alpha' }),
			buildSessionSummary({ id: 'c', project: 'ALPHA' })
		];
		const groups = groupByProject(sessions);
		expect(groups).toHaveLength(1);
		expect(groups[0]!.project).toBe('Alpha');
		expect(groups[0]!.sessions).toHaveLength(3);
	});
});

const CLEAR_PROMPT =
	'<local-command-caveat>Caveat: The messages below were generated by the user while running local commands. DO NOT respond to these messages or otherwise consider them in your response unless the user explicitly asks you to.</local-command-caveat>\n<command-name>/clear</command-name>\n            <command-message>clear</command-message>\n            <command-args></command-args>\n<local-command-stdout></local-command-stdout>\n?';

const GOAL_PROMPT =
	"<command-name>/goal</command-name>\n            <command-message>goal</command-message>\n            <command-args>we have the documentation in artifacts to split out the response data into it's own table.</command-args>\n<local-command-stdout>Goal set: we have the documentation in artifacts to split out the response data into it's own table.</local-command-stdout>\nProceed. I will check back in the morning. To gauge your finished work and do a full critical review";

function makeRow(o: Partial<KickoffCandidate> & { id: number }): KickoffCandidate {
	return {
		path: '/v1/messages',
		max_tokens: 32000,
		created_at: '2026-05-16T18:26:24.000Z',
		turn_origin: 'primary',
		human_prompt: 'a real typed prompt',
		...o
	};
}

describe('isLocalCommandOnlyPrompt', () => {
	it('is true for a bare /clear (only a lone ? survives stripping)', () => {
		expect(isLocalCommandOnlyPrompt(CLEAR_PROMPT)).toBe(true);
	});

	it('is true for an empty prompt', () => {
		expect(isLocalCommandOnlyPrompt('')).toBe(true);
	});

	it('is false for a /goal that carries typed prose alongside the command', () => {
		expect(isLocalCommandOnlyPrompt(GOAL_PROMPT)).toBe(false);
	});

	it('is false for a plain typed prompt', () => {
		expect(isLocalCommandOnlyPrompt('investigate the log conflicts')).toBe(false);
	});

	it('keeps inner text of a <session>-wrapped prompt (genuine)', () => {
		expect(isLocalCommandOnlyPrompt('<session>\nSomething on the ui is jumping\n</session>')).toBe(
			false
		);
	});

	it('is true when only command wrappers + whitespace remain', () => {
		expect(isLocalCommandOnlyPrompt('<command-name>/compact</command-name>\n   \n')).toBe(true);
	});
});

describe('selectKickoffRequest', () => {
	it('skips the /clear local-command row and picks the real /goal prompt (session cad603b3…)', () => {
		const rows: KickoffCandidate[] = [
			makeRow({
				id: 28607,
				created_at: '2026-05-16T18:26:24.326Z',
				turn_origin: 'primary_first',
				human_prompt: CLEAR_PROMPT
			}),
			makeRow({
				id: 28676,
				created_at: '2026-05-16T19:17:48.115Z',
				turn_origin: 'primary_first',
				human_prompt: '<session>\nProceed. I will check back in the morning.\n</session>'
			}),
			makeRow({
				id: 28677,
				created_at: '2026-05-16T19:17:48.125Z',
				turn_origin: 'primary',
				human_prompt: GOAL_PROMPT
			})
		];
		expect(selectKickoffRequest(rows)).toBe(28676);
	});

	it('skips the quota probe and picks the next genuine prompt (session 39c9fc5a…)', () => {
		const rows: KickoffCandidate[] = [
			makeRow({
				id: 32035,
				created_at: '2026-05-18T07:50:00.000Z',
				turn_origin: 'primary_first',
				max_tokens: 1,
				human_prompt: 'quota'
			}),
			makeRow({
				id: 32036,
				created_at: '2026-05-18T07:52:02.000Z',
				turn_origin: 'primary',
				max_tokens: 64000,
				human_prompt:
					'<session>\nI want you to go investigate some conflicts in our logs.\n</session>'
			})
		];
		expect(selectKickoffRequest(rows)).toBe(32036);
	});

	it('excludes the quota probe even when it is the only primary_first row', () => {
		const rows: KickoffCandidate[] = [
			makeRow({
				id: 35623,
				created_at: '2026-05-18T08:00:00.000Z',
				turn_origin: 'primary_first',
				max_tokens: 1,
				human_prompt: 'quota'
			}),
			makeRow({
				id: 35627,
				created_at: '2026-05-18T08:01:00.000Z',
				turn_origin: 'primary',
				max_tokens: 64000,
				human_prompt:
					'<session>\nSomething on the ui is causing a session to jump around\n</session>'
			})
		];
		expect(selectKickoffRequest(rows)).toBe(35627);
	});

	it('picks the chronologically-earliest genuine row even when rows are out of order', () => {
		const rows: KickoffCandidate[] = [
			makeRow({ id: 3, created_at: '2026-05-16T12:00:03.000Z' }),
			makeRow({ id: 1, created_at: '2026-05-16T12:00:01.000Z' }),
			makeRow({ id: 2, created_at: '2026-05-16T12:00:02.000Z' })
		];
		expect(selectKickoffRequest(rows)).toBe(1);
	});

	it('tie-breaks identical timestamps on the smaller id', () => {
		const rows: KickoffCandidate[] = [
			makeRow({ id: 9, created_at: '2026-05-16T12:00:00.000Z' }),
			makeRow({ id: 4, created_at: '2026-05-16T12:00:00.000Z' })
		];
		expect(selectKickoffRequest(rows)).toBe(4);
	});

	it('ignores non-/v1/messages rows and non-primary origins', () => {
		const rows: KickoffCandidate[] = [
			makeRow({
				id: 1,
				path: '/v1/messages/count_tokens',
				created_at: '2026-05-16T12:00:00.000Z'
			}),
			makeRow({
				id: 2,
				turn_origin: 'subagent',
				created_at: '2026-05-16T12:00:01.000Z'
			}),
			makeRow({
				id: 3,
				turn_origin: 'title_sidecar',
				created_at: '2026-05-16T12:00:02.000Z'
			}),
			makeRow({
				id: 4,
				turn_origin: 'primary',
				created_at: '2026-05-16T12:00:03.000Z'
			})
		];
		expect(selectKickoffRequest(rows)).toBe(4);
	});

	it('treats a missing turn_origin as primary', () => {
		const rows: KickoffCandidate[] = [makeRow({ id: 7, turn_origin: undefined })];
		expect(selectKickoffRequest(rows)).toBe(7);
	});

	it('falls back to the first primary /v1/messages row when every candidate is noise', () => {
		const rows: KickoffCandidate[] = [
			makeRow({
				id: 100,
				created_at: '2026-05-16T10:00:00.000Z',
				turn_origin: 'primary_first',
				max_tokens: 1,
				human_prompt: 'quota'
			}),
			makeRow({
				id: 101,
				created_at: '2026-05-16T10:01:00.000Z',
				turn_origin: 'primary_first',
				human_prompt: CLEAR_PROMPT
			})
		];
		expect(selectKickoffRequest(rows)).toBe(100);
	});

	it('returns null when there is no primary /v1/messages row at all', () => {
		const rows: KickoffCandidate[] = [
			makeRow({ id: 1, path: '/v1/messages/count_tokens' }),
			makeRow({ id: 2, turn_origin: 'subagent' })
		];
		expect(selectKickoffRequest(rows)).toBe(null);
	});

	it('returns null for an empty row list', () => {
		expect(selectKickoffRequest([])).toBe(null);
	});
});
