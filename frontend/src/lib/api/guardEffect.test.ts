import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import {
	getDeliveredWakes,
	isJudged,
	getGuardEffect,
	groupWakesByQuestion,
	collapseRepeats,
	evaluationUuidsOf,
	guardQuestionLabel,
	labelGuardEvaluation
} from './guardEffect';
import type { GuardWake } from './guardEffect';

function jsonResponse(body: unknown, status = 200): Response {
	return {
		ok: status >= 200 && status < 300,
		status,
		statusText: status === 200 ? 'OK' : 'Error',
		json: async () => body
	} as Response;
}

function wake(id: number, questionKey: string): GuardWake {
	return {
		id,
		evaluationUuid: `00000000-0000-4000-8000-00000000000${id}`,
		questionKey,
		questionLabel: guardQuestionLabel(questionKey),
		arm: 'wake',
		sessionTitle: 'Investigate slow 40-minute Flux job',
		sessionHref: '/sessions/abc',
		projectLabel: 'temet-proxy',
		projectPath: '/mnt/data/projects/temet/temet-proxy',
		turnHref: '/sessions/abc/requests/1',
		deliveredAt: '2026-09-28T10:00:00Z',
		outcome: 'redirected',
		operatorLabel: null
	};
}

describe('guardQuestionLabel', () => {
	it('names every guard question in plain words', () => {
		expect(guardQuestionLabel('unbacked_wait')).toBe(
			'Did the model wait on something it never backed up?'
		);
		expect(guardQuestionLabel('relapse')).toBe(
			'Does this reply repeat a correction you already made?'
		);
		expect(guardQuestionLabel('unobserved_done')).toBe("Did the last turn's work actually ship?");
	});

	it('falls back to a humanized key for an unrecognised question', () => {
		expect(guardQuestionLabel('some_new_question')).toBe('some new question');
	});
});

describe('groupWakesByQuestion', () => {
	it('groups delivered wakes under their own question key, preserving order', () => {
		const wakes = [wake(1, 'relapse'), wake(2, 'unobserved_done'), wake(3, 'relapse')];
		const grouped = groupWakesByQuestion(wakes);
		expect(Object.keys(grouped)).toEqual(['relapse', 'unobserved_done']);
		expect(grouped.relapse?.map((w) => w.id)).toEqual([1, 3]);
		expect(grouped.unobserved_done?.map((w) => w.id)).toEqual([2]);
	});

	it('returns an empty object for no wakes', () => {
		expect(groupWakesByQuestion([])).toEqual({});
	});
});

describe('collapseRepeats', () => {
	it('folds one question firing repeatedly in one turn into the newest card', () => {
		const a = { ...wake(7, 'unobserved_done'), evaluationUuid: 'u-new' };
		const b = { ...wake(7, 'unobserved_done'), evaluationUuid: 'u-old' };
		const other = { ...wake(7, 'relapse'), evaluationUuid: 'u-relapse' };
		const labelled = {
			...wake(7, 'unobserved_done'),
			evaluationUuid: 'u-done',
			operatorLabel: 'wrong' as const
		};
		const out = collapseRepeats([a, b, other, labelled]);
		expect(out.map((w) => w.evaluationUuid)).toEqual(['u-new', 'u-relapse', 'u-done']);
		expect(evaluationUuidsOf(out[0]!)).toEqual(['u-new', 'u-old']);
		expect(evaluationUuidsOf(out[1]!)).toEqual(['u-relapse']);
	});
});

describe('$lib/api/guardEffect', () => {
	let fetchMock: ReturnType<typeof vi.fn>;

	beforeEach(() => {
		fetchMock = vi.fn();
		vi.stubGlobal('fetch', fetchMock);
	});

	afterEach(() => {
		vi.unstubAllGlobals();
	});

	describe('getGuardEffect', () => {
		it('maps the real steering_guard_effect.rs response shape to camelCase', async () => {
			fetchMock.mockResolvedValueOnce(
				jsonResponse({
					as_of_secs: 1790600000,
					read_point_holdback: 34,
					questions: [
						{
							question: 'relapse',
							as_of_secs: 1790600000,
							primary: {
								treated: { n: 120, bad: 17, p_bad: 0.14 },
								holdback: { n: 40, bad: 11, p_bad: 0.27 },
								rd: 0.13,
								ci: [0.03, 0.19]
							},
							secondary: {
								treated: { n: 120, bad: 17, p_bad: 0.14 },
								holdback: { n: 40, bad: 11, p_bad: 0.27 },
								rd: 0.13,
								ci: [0.02, 0.2]
							},
							per_protocol_not_randomised: {
								treated: { n: 118, bad: 16, p_bad: 0.136 },
								holdback: { n: 40, bad: 11, p_bad: 0.27 },
								rd: 0.134,
								ci: [0.03, 0.2]
							},
							missing: { no_next_message: 2 },
							events: 160,
							status: 'readable'
						}
					]
				})
			);

			const result = await getGuardEffect();

			expect(result.asOfSecs).toBe(1790600000);
			expect(result.readPointHoldback).toBe(34);
			expect(result.questions).toEqual([
				{
					questionKey: 'relapse',
					asOfSecs: 1790600000,
					primary: {
						treated: { n: 120, bad: 17, pBad: 0.14 },
						holdback: { n: 40, bad: 11, pBad: 0.27 },
						rd: 0.13,
						ci: [0.03, 0.19]
					},
					secondary: {
						treated: { n: 120, bad: 17, pBad: 0.14 },
						holdback: { n: 40, bad: 11, pBad: 0.27 },
						rd: 0.13,
						ci: [0.02, 0.2]
					},
					perProtocolNotRandomised: {
						treated: { n: 118, bad: 16, pBad: 0.136 },
						holdback: { n: 40, bad: 11, pBad: 0.27 },
						rd: 0.134,
						ci: [0.03, 0.2]
					},
					missing: { no_next_message: 2 },
					events: 160,
					status: 'readable'
				}
			]);
		});

		it('maps a monitoring (not-yet-readable) question with null rd/ci', async () => {
			fetchMock.mockResolvedValueOnce(
				jsonResponse({
					as_of_secs: 1790600000,
					read_point_holdback: 34,
					questions: [
						{
							question: 'unobserved_done',
							as_of_secs: 1790600000,
							primary: {
								treated: { n: 21, bad: 3, p_bad: 0.143 },
								holdback: { n: 7, bad: 2, p_bad: 0.286 },
								rd: null,
								ci: null
							},
							secondary: {
								treated: { n: 21, bad: 3, p_bad: 0.143 },
								holdback: { n: 7, bad: 2, p_bad: 0.286 },
								rd: null,
								ci: null
							},
							per_protocol_not_randomised: {
								treated: { n: 21, bad: 3, p_bad: 0.143 },
								holdback: { n: 7, bad: 2, p_bad: 0.286 },
								rd: null,
								ci: null
							},
							missing: {},
							events: 28,
							status: 'monitoring'
						}
					]
				})
			);

			const [question] = (await getGuardEffect()).questions;

			expect(question?.status).toBe('monitoring');
			expect(question?.primary.rd).toBeNull();
			expect(question?.primary.ci).toBeNull();
			expect(question?.primary.holdback.n).toBe(7);
		});
	});

	describe('getDeliveredWakes', () => {
		it('maps interventions_feed.rs rows to the shared GuardWake view shape', async () => {
			fetchMock.mockResolvedValueOnce(
				jsonResponse({
					interventions: [
						{
							uuid: 'a1b2c3d4-0000-4000-8000-000000000901',
							session_uuid: 'hash-99aa11',
							session_id: 42,
							operator_turn_id: 901,
							question: 'unobserved_done',
							arm: 'treated',
							dispatched_at: '2026-09-28T10:05:00Z',
							outcome_label: null,
							label_source: null,
							delivered_request_id: 88,
							wake_state: 'pushed',
							outcome_reaction: 'redirected',
							delivery_channel: 'wake',
							session_title: 'Investigate slow 40-minute Flux job',
							project_path: '/mnt/data/projects/work/ankh-morpork/night-watch',
							note_text: 'You said the work shipped.\nNothing in this turn shows it did.'
						},
						{
							uuid: 'a1b2c3d4-0000-4000-8000-000000000902',
							session_uuid: 'hash-77bb22',
							session_id: 43,
							operator_turn_id: 902,
							question: 'relapse',
							arm: 'holdback',
							dispatched_at: '2026-09-28T11:00:00Z',
							outcome_label: 'right',
							label_source: 'operator',
							delivered_request_id: null,
							wake_state: null,
							outcome_reaction: null,
							delivery_channel: 'append',
							session_title: null,
							session_first_prompt: '  fix the flaky login test  '
						}
					]
				})
			);

			const wakes = await getDeliveredWakes();

			expect(wakes).toEqual([
				{
					id: 901,
					evaluationUuid: 'a1b2c3d4-0000-4000-8000-000000000901',
					questionKey: 'unobserved_done',
					questionLabel: "Did the last turn's work actually ship?",
					arm: 'wake',
					sessionTitle: 'Investigate slow 40-minute Flux job',
					sessionHref: '/sessions/hash-99aa11',
					projectLabel: 'night-watch',
					projectPath: '/mnt/data/projects/work/ankh-morpork/night-watch',
					turnHref: '/sessions/hash-99aa11/requests/88',
					deliveredAt: '2026-09-28T10:05:00Z',
					outcome: 'redirected',
					operatorLabel: null,
					noteText: 'You said the work shipped.\nNothing in this turn shows it did.',
					repeatUuids: []
				},
				{
					id: 902,
					evaluationUuid: 'a1b2c3d4-0000-4000-8000-000000000902',
					questionKey: 'relapse',
					questionLabel: 'Does this reply repeat a correction you already made?',
					arm: 'append',
					sessionTitle: 'fix the flaky login test',
					sessionHref: '/sessions/hash-77bb22',
					projectLabel: 'No project',
					projectPath: null,
					turnHref: '/sessions/hash-77bb22',
					deliveredAt: '2026-09-28T11:00:00Z',
					outcome: null,
					operatorLabel: 'right',
					noteText: null,
					repeatUuids: []
				}
			]);
		});

		it('falls back to the short session id for the title and to arm "wake" when delivery_channel is absent', async () => {
			fetchMock.mockResolvedValueOnce(
				jsonResponse({
					interventions: [
						{
							uuid: 'a1b2c3d4-0000-4000-8000-000000000903',
							session_uuid: 'hash-cc33',
							session_id: 44,
							operator_turn_id: 903,
							question: 'relapse',
							arm: 'treated',
							dispatched_at: '2026-09-28T12:00:00Z',
							outcome_label: null,
							label_source: null,
							delivered_request_id: null,
							wake_state: null,
							outcome_reaction: 'accepted'
							// delivery_channel and session_title intentionally omitted (pre-P4-push shape)
						}
					]
				})
			);

			const [wake] = await getDeliveredWakes();

			expect(wake?.arm).toBe('wake');
			expect(wake?.sessionTitle).toBe('hash-cc33');
			expect(wake?.outcome).toBe('accepted');
			expect(wake?.noteText).toBeNull();
		});

		it('keeps judged reactions and treats a missing or blank one as not judged yet', async () => {
			const row = (turn: number, reaction: string | null) => ({
				uuid: `a1b2c3d4-0000-4000-8000-00000000${turn}`,
				session_uuid: 'hash-dd44',
				session_id: 45,
				operator_turn_id: turn,
				question: 'relapse',
				arm: 'treated',
				dispatched_at: '2026-09-28T12:00:00Z',
				outcome_label: null,
				label_source: null,
				delivered_request_id: null,
				wake_state: null,
				outcome_reaction: reaction
			});
			fetchMock.mockResolvedValueOnce(
				jsonResponse({
					interventions: [
						row(1001, 'no_response'),
						row(1002, 'unclear'),
						row(1003, null),
						row(1004, ''),
						row(1005, 'something_new')
					]
				})
			);

			const wakes = await getDeliveredWakes();

			expect(wakes.map((w) => w.outcome)).toEqual([
				'no_response',
				'unclear',
				null,
				null,
				'unknown'
			]);
			expect(wakes.map(isJudged)).toEqual([true, true, false, false, false]);
		});

		it('returns an empty array when nothing has been delivered', async () => {
			fetchMock.mockResolvedValueOnce(jsonResponse({ interventions: [] }));
			expect(await getDeliveredWakes()).toEqual([]);
		});
	});

	describe('labelGuardEvaluation', () => {
		it('POSTs the verdict to the route the inspect API registers', async () => {
			fetchMock.mockResolvedValueOnce(jsonResponse({ label_id: 1, cancelled_wakes: [] }));
			await labelGuardEvaluation('6cab6552-e0ee-49d6-bacb-9a5d7cc17150', 'right');
			const [url, init] = fetchMock.mock.calls[0] as [string, RequestInit];
			expect(url).toBe('/api/steering/evaluations/6cab6552-e0ee-49d6-bacb-9a5d7cc17150/label');
			expect(init.method).toBe('POST');
			expect(JSON.parse(String(init.body))).toEqual({ label: 'right' });
		});
	});
});
