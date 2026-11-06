import { describe, expect, it } from 'vitest';
import type { EvalAlarmDto } from '$lib/api/evalMetrics';
import { latestReviewableRun } from '../../../routes/sessions/labelling/review/latest';
import {
	ALARMS,
	BUNDLES,
	GATES,
	RUN_2_SESSION,
	RUN_3,
	RUN_4,
	RUN_5,
	RUN_6
} from '$lib/test-support/judgesFixtures';
import {
	alarmHome,
	alarmWords,
	alarmsFor,
	configurationLine,
	gateFor,
	gateForBundle,
	gateWords,
	currentRunFor,
	inUseBarWords,
	newestFirst,
	questionScore,
	rubricDate,
	runName,
	runRole,
	sameAs,
	sessionScore,
	share,
	turnSetWords,
	twinNote
} from './judges';

function alarm(kind: string, subject: string, detail: Record<string, unknown> = {}): EvalAlarmDto {
	return {
		id: 1,
		kind,
		subject,
		first_seen_at: '2026-09-26T07:16:34Z',
		last_seen_at: '2026-09-28T07:16:34Z',
		cleared_at: null,
		evaluations: 3,
		quiet_evaluations: 0,
		detail,
		acknowledged_at: null,
		ack_note: null
	};
}

describe('where each alarm is shown', () => {
	const roles = new Map(BUNDLES.map((b) => [b.id, b.role]));

	it('routes each alarm to the page that operates its subject, and keeps the rest on Workers', () => {
		expect(alarmHome(alarm('unjudged_backlog', 'turn_judge'), roles)).toBe('turn');
		expect(alarmHome(alarm('label_mix_shift', 'shift:turn_judge/legacy_outcome'), roles)).toBe(
			'turn'
		);
		expect(alarmHome(alarm('change_without_gate', 'bundle:2'), roles)).toBe('session');
		expect(alarmHome(alarm('judge_call_loop', 'anything'), roles)).toBe('session');
		expect(alarmHome(alarm('change_without_gate', 'bundle:11'), roles)).toBe('steering');
		expect(alarmHome(alarm('embedding_drift', 'embedding'), roles)).toBe('embedding');
		expect(alarmHome(alarm('change_without_gate', 'bundle:999'), roles)).toBe('workers');
	});

	it('loses no open alarm: every one lands on exactly one page', () => {
		const homes = ['turn', 'session', 'steering', 'embedding', 'workers'] as const;
		const counted = homes.map((h) => alarmsFor(ALARMS, BUNDLES, h).length);
		expect(counted.reduce((a, b) => a + b, 0)).toBe(ALARMS.length);
		expect(alarmsFor(ALARMS, BUNDLES, 'steering').map((a) => a.subject)).toEqual([
			'bundle:11',
			'bundle:10',
			'bundle:6',
			'bundle:4'
		]);
	});
});

describe('alarm sentences', () => {
	it('says the backlog in one sentence, with the failure clause only when calls failed', () => {
		const words = alarmWords(ALARMS[0] as EvalAlarmDto).sentence;
		expect(words).toMatch(
			/^122 finished turns have not been judged\. The oldest finished on 3 Aug 2026\. 47 of the last 358 calls to the model failed\.$/
		);
		expect(
			alarmWords(
				alarm('unjudged_backlog', 'turn_judge', { stale_turns: 3, calls: { ok: 9, http_error: 0 } })
			).sentence
		).toBe('3 finished turns have not been judged.');
	});

	it('gives every judge alarm a next step with links, not only Acknowledge', () => {
		const failing = alarmWords(ALARMS[0] as EvalAlarmDto);
		expect(failing.advice).toMatch(/^Calls to the model are failing/);
		expect(failing.actions.map((a) => `${a.path}${a.suffix}`)).toEqual([
			'/settings/integrations#ct-title',
			'/sessions/labelling#setting-judge_worker.batch_size',
			'/sessions/labelling#setting-judge_worker.interval_secs'
		]);
		const slow = alarmWords(
			alarm('unjudged_backlog', 'turn_judge', { stale_turns: 3, calls: { ok: 9, http_error: 0 } })
		);
		expect(slow.advice).toMatch(/^No calls failed/);
		expect(slow.actions.map((a) => a.label)).toEqual(['Turns per sweep', 'Seconds between sweeps']);
		const shift = alarmWords(ALARMS[1] as EvalAlarmDto);
		expect(shift.actions[0]).toEqual({
			label: 'Review turns where the judge says Corrected',
			path: '/sessions/labelling/review',
			suffix: '?judge=corrected'
		});
	});

	it('names the label whose share moved most, then and now', () => {
		expect(alarmWords(ALARMS[1] as EvalAlarmDto).sentence).toBe(
			'The labels this judge writes have shifted. Last 7 days: 27% of turns labelled Corrected (72 of 264). The 28 days before: 8.5%.'
		);
	});

	it('says every alarm kind the backend raises in a sentence of its own, with no raw details', () => {
		for (const kind of [
			'label_mix_shift',
			'unjudged_backlog',
			'judge_stalled',
			'budget_refusals',
			'change_without_gate',
			'gold_label_by_agent',
			'judge_call_loop'
		]) {
			const words = alarmWords(alarm(kind, 'turn_judge'));
			expect(words.showDetail).toBe(false);
			expect(words.sentence).not.toContain('_');
		}
	});

	it('names a configuration changed without its accuracy measured, and falls back to plain words with details', () => {
		expect(alarmWords(alarm('change_without_gate', 'bundle:11')).sentence).toMatch(
			/^Configuration 11 is in use without its accuracy measured against the accuracy bar\. First seen \d{1,2} Sep 2026\.$/
		);
		expect(alarmWords(alarm('change_without_gate', 'bundle:11')).actions).toEqual([]);
		expect(alarmWords(alarm('change_without_gate', 'bundle:2'), 'session').advice).toContain(
			'mike eval run --role session_judge'
		);
		const other = alarmWords(alarm('embedding_drift', 'embedding'));
		expect(other.sentence).toBe('Embedding drift.');
		expect(other.showDetail).toBe(true);
	});
});

describe('failed calls on the backlog alarm', () => {
  it('counts transport failures as failed calls, alone and beside HTTP failures', () => {
    const mixed = alarmWords(
      alarm('unjudged_backlog', 'turn_judge', {
        stale_turns: 3,
        calls: { ok: 9, http_error: 1, transport_error: 2 }
      })
    );
    expect(mixed.sentence).toBe(
      '3 finished turns have not been judged. 3 of the last 12 calls to the model failed.'
    );
    expect(mixed.advice).toMatch(/^Calls to the model are failing/);

    const transportOnly = alarmWords(
      alarm('unjudged_backlog', 'turn_judge', { stale_turns: 3, calls: { ok: 9, transport_error: 2 } })
    );
    expect(transportOnly.sentence).toBe(
      '3 finished turns have not been judged. 2 of the last 11 calls to the model failed.'
    );
  });
});

describe('configurations and accuracy runs in words', () => {
	const b14 = BUNDLES.find((b) => b.id === 14);
	const b15 = BUNDLES.find((b) => b.id === 15);

	it('describes a configuration by model, thinking and the date of its rules', () => {
		expect(rubricDate('v2-2026-09-23')).toBe('23 Sep 2026');
		expect(rubricDate(2)).toBeNull();
		expect(configurationLine(b14)).toBe(
			'google/gemini-3.8-flash · thinking: low · rules of 23 Sep 2026'
		);
		expect(b15 && sameAs(b15, BUNDLES)?.id).toBe(14);
		expect(b14 && sameAs(b14, BUNDLES)).toBeNull();
		expect(b15 && twinNote(b15, BUNDLES, [RUN_3, RUN_4, RUN_6])).toBe(
			'same model and instructions as 14; its score differs because it was measured on different turns'
		);
		expect(b15 && twinNote(b15, BUNDLES, [RUN_4])).toBe('same model and instructions as 14');
	});

	it('says which turns a run was tested on, and how it scored', () => {
		expect(turnSetWords(RUN_4)).toBe(
			'200 turns set aside for measuring accuracy, labelled up to 28 Sep 2026'
		);
		expect(turnSetWords(RUN_3)).toBe(
			'277 of 278 turns used while building the judge, labelled up to 25 Sep 2026'
		);
		expect(turnSetWords(RUN_2_SESSION)).toBe('tried on 1 session, labelled up to 25 Sep 2026');
		expect(questionScore(RUN_4, 'reaction')).toEqual({
			key: 'reaction',
			agreed: 160,
			n: 200,
			percent: '80%'
		});
		expect(questionScore(RUN_5, 'delivery')).toBeNull();
		expect(sessionScore(RUN_2_SESSION)).toEqual({ agreed: 1, n: 1, percent: '100%' });
		expect(runRole(RUN_2_SESSION)).toBe('session_judge');
		expect(runRole(RUN_4)).toBe('turn_judge');
		expect(share(86, 1013)).toBe('8.5%');
	});

	it('picks one current run per configuration: its newest finished run, repeats included', () => {
		const runs = [RUN_3, RUN_4, RUN_5, RUN_6];
		expect(newestFirst(runs).map((r) => r.id)).toEqual([6, 5, 4, 3]);
		expect(currentRunFor(runs, 14)?.id).toBe(6);
		expect(latestReviewableRun(runs, 14)?.id).toBe(6);
		expect(latestReviewableRun(runs, 0)?.id).toBe(6);
		expect(latestReviewableRun([RUN_4, RUN_5], 14)?.id).toBe(4);
	});

	it('finds the accuracy-bar result of a run and its repeat, with the bar stated', () => {
		const runs = [RUN_3, RUN_4, RUN_5, RUN_6];
		expect(gateFor(GATES, 4)?.id).toBe(2);
		expect(gateFor(GATES, 6)?.id).toBe(2);
		expect(gateFor(GATES, 5)).toBeNull();
		expect(gateForBundle(GATES, runs, 14)?.id).toBe(2);
		expect(gateWords(gateForBundle(GATES, runs, 19))).toBe(
			'Not yet measured against the accuracy bar set for being put in use.'
		);
		expect(gateWords(gateFor(GATES, 4))).toMatch(
			/^Passed the accuracy bar set for being put in use, on \d{1,2} Sep 2026\. When it says Corrected, the label agrees 86 times in 100 \(needs 85\)\. Of the turns labelled Corrected, it finds 78 in 100 \(needs 65\)\.$/
		);
		const b14 = BUNDLES.find((b) => b.id === 14);
		if (!b14) throw new Error('fixture');
		const early = { ...b14, override_reason: 'operator asked' };
		expect(inUseBarWords(early, gateFor(GATES, 4))).toMatch(
			/^Put in use before its accuracy was measured\. Measured later, on \d{1,2} Sep 2026, it passed the accuracy bar set for being put in use\. When it says Corrected/
		);
		expect(inUseBarWords(early, null)).toMatch(
			/^Put in use without its accuracy being measured first\./
		);
		expect(runName(RUN_4)).toMatch(
			/^Labelling accuracy, 28 Sep( 2026)? \d\d:\d\d · configuration 14 \(gemini-3\.8-flash\)$/
		);
	});
});
