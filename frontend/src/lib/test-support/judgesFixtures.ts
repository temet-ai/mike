import type { EvalAlarmDto, RejudgeRunDto } from '$lib/api/evalMetrics';
import type { BundleDto, EvalGateDto, EvalRunDto } from '$lib/api/evalRuns';
import type { FeatureDto, SettingMeta } from '$lib/api/settings';
import { RUN } from './evalReviewFixtures';

/*
 * Labelling-area fixtures, shaped like the live inspect API on 2026-09-28: the turn judge's
 * configurations 1, 14, 15 and 19, the session judge's 2 and 13, accuracy runs 1 to 6, the
 * accuracy-bar result of run 4, the running re-judge and the open alarms. Instructions text is written for
 * these fixtures.
 */

const SHA_14 = 'b9ddc31e1417974bcf1212d5c582a421f346158441e8b01a477b09d79be00c26';

function bundle(
	id: number,
	role: string,
	status: string,
	extra: Partial<BundleDto> = {}
): BundleDto {
	return {
		id,
		role,
		origin: 'candidate',
		status,
		content_sha256: `sha-${id}`,
		created_at: '2026-09-25T07:16:03Z',
		promoted_at: null,
		override_reason: null,
		components: {},
		...extra
	};
}

export const BUNDLES: BundleDto[] = [
	bundle(19, 'turn_judge', 'candidate', {
		created_at: '2026-09-28T12:22:11Z',
		components: {
			model: 'google/gemini-3.8-flash',
			reasoning_effort: 'low',
			rubric_version: 'r3-2026-08-29'
		}
	}),
	bundle(15, 'turn_judge', 'candidate', {
		created_at: '2026-09-25T19:02:21Z',
		content_sha256: SHA_14,
		components: {
			model: 'google/gemini-3.8-flash',
			reasoning_effort: 'low',
			rubric_version: 'v2-2026-09-23'
		}
	}),
	bundle(14, 'turn_judge', 'live', {
		created_at: '2026-09-25T11:28:15Z',
		promoted_at: '2026-09-25T11:28:15Z',
		override_reason: 'Operator requested full enablement on 2026-09-25.',
		content_sha256: SHA_14,
		components: {
			model: 'google/gemini-3.8-flash',
			reasoning_effort: 'low',
			rubric_version: 'v2-2026-09-23'
		}
	}),
	bundle(13, 'session_judge', 'live_ungated', {
		created_at: '2026-09-25T11:18:49Z',
		promoted_at: '2026-09-25T11:18:49Z',
		override_reason: 'Operator requested full enablement on 2026-09-25.',
		components: { model: 'google/gemini-3.5-flash-lite', reasoning_effort: 'low' }
	}),
	bundle(11, 'steerer', 'live_ungated'),
	bundle(10, 'steerer', 'live_ungated'),
	bundle(6, 'steerer', 'live_ungated'),
	bundle(4, 'steerer', 'live_ungated'),
	bundle(2, 'session_judge', 'live_ungated', {
		components: {
			model: 'google/gemini-3.5-flash-lite',
			reasoning_effort: 'low',
			rubric_version: 2
		}
	}),
	bundle(1, 'turn_judge', 'live_ungated', {
		promoted_at: '2026-09-25T11:18:49Z',
		components: {
			model: 'google/gemini-3.8-flash',
			reasoning_effort: 'low',
			rubric_version: 'r3-2026-08-29'
		}
	})
];

function run(id: number, extra: Partial<EvalRunDto>): EvalRunDto {
	return { ...RUN, id, repeat_of_run_id: null, argv: {}, ...extra };
}

export const RUN_6 = run(6, {
	purpose: 'noise',
	bundle_id: 14,
	split: 'holdout',
	created_at: '2026-09-28T12:35:58Z',
	finished_at: '2026-09-28T12:54:18Z',
	labels_as_of: '2026-09-28T12:35:58Z',
	label_count: 200,
	items_judged: 200,
	repeat_of_run_id: 4,
	report: {
		n: 200,
		reaction: { n: 200, agreement: 163, kappa: 0.72 },
		delivery: { n: 200, agreement: 161, kappa: 0.7 }
	}
});

export const RUN_5 = run(5, {
	bundle_id: 19,
	split: 'holdout',
	created_at: '2026-09-28T12:22:11Z',
	finished_at: '2026-09-28T12:35:19Z',
	labels_as_of: '2026-09-28T12:22:11Z',
	label_count: 200,
	items_judged: 200,
	report: { n: 200, reaction: { n: 200, agreement: 117, kappa: 0.4 } }
});

export const RUN_4 = run(4, {
	bundle_id: 14,
	split: 'holdout',
	created_at: '2026-09-28T09:46:51Z',
	finished_at: '2026-09-28T12:34:54Z',
	labels_as_of: '2026-09-28T09:46:51Z',
	label_count: 200,
	items_judged: 200,
	report: {
		n: 200,
		reaction: { n: 200, agreement: 160, kappa: 0.72 },
		delivery: { n: 200, agreement: 162, kappa: 0.7 }
	}
});

export const RUN_3 = RUN;

export const RUN_1 = run(1, {
	bundle_id: 14,
	created_at: '2026-09-25T11:28:15Z',
	finished_at: '2026-09-25T11:28:18Z',
	labels_as_of: '2026-09-25T11:28:15Z',
	label_count: 1,
	items_judged: 1,
	report: {
		n: 1,
		reaction: { n: 1, agreement: 1, kappa: null },
		delivery: { n: 1, agreement: 1, kappa: null }
	}
});

export const RUN_2_SESSION = run(2, {
	bundle_id: 13,
	model: 'google/gemini-3.5-flash-lite',
	created_at: '2026-09-25T11:30:10Z',
	finished_at: '2026-09-25T11:30:13Z',
	labels_as_of: '2026-09-25T11:30:10Z',
	label_count: 1,
	items_judged: 1,
	argv: { role: 'session_judge' },
	report: {
		items: 1,
		variants: { model_only: { n: 1, confusion: { done: { done: 1 } } } }
	}
});

export const TURN_RUNS: EvalRunDto[] = [RUN_6, RUN_5, RUN_4, RUN_3, RUN_1];

export const GATES: EvalGateDto[] = [
	{
		id: 2,
		created_at: '2026-09-28T12:54:18Z',
		candidate_run_id: 4,
		baseline_run_id: 5,
		noise_run_id: 6,
		verdict: 'pass',
		floors: { precision: 0.85, recall: 0.65 },
		report: {
			corrected: { precision: 0.857, recall: 0.779 },
			regression: { corrected: { gained: 67, lost: 11 } }
		},
		override_reason: null
	}
];

export const REJUDGE_RUNNING: RejudgeRunDto = {
	id: 1,
	role: 'turn_judge',
	bundle_id: 14,
	window_since: '2026-07-27T00:00:00Z',
	window_until: '2026-09-28T12:55:28Z',
	daily_call_cap: 300,
	status: 'running',
	estimated_calls: 2874,
	estimated_usd: 4.94,
	created_by: 'mike eval rejudge',
	created_at: '2026-09-28T12:55:29Z',
	started_at: '2026-09-28T12:56:20Z',
	finished_at: null,
	eligible_total: 3326,
	ineligible_total: 2614,
	judged: 564,
	permanent_failures: 0,
	remaining: 2762,
	calls_today: 228,
	cost_usd_so_far: 1.07,
	ineligible_reprojected: 2614,
	projected_finish_date: '2026-10-08T13:26:16Z'
};

function openAlarm(
	id: number,
	kind: string,
	subject: string,
	firstSeen: string,
	detail: Record<string, unknown>
): EvalAlarmDto {
	return {
		id,
		kind,
		subject,
		first_seen_at: firstSeen,
		last_seen_at: '2026-09-28T13:00:10Z',
		cleared_at: null,
		evaluations: 4,
		quiet_evaluations: 0,
		detail,
		acknowledged_at: null,
		ack_note: null
	};
}

const GATE_CAUSE = { cause: { trigger: 'startup', keys: [], change_log_ids: [] } };

export const ALARMS: EvalAlarmDto[] = [
	openAlarm(2713, 'unjudged_backlog', 'turn_judge', '2026-09-28T13:00:10Z', {
		calls: { http_error: 47, ok: 311 },
		oldest_closed_at: '2026-08-03T10:29:33Z',
		stale_turns: 122
	}),
	openAlarm(2278, 'label_mix_shift', 'shift:turn_judge/legacy_outcome', '2026-09-28T03:07:41Z', {
		baseline: { classes: { abandoned: 328, accepted: 496, corrected: 86, shipped: 103 }, n: 1013 },
		current: { classes: { abandoned: 27, accepted: 137, corrected: 72, shipped: 28 }, n: 264 }
	}),
	openAlarm(213, 'change_without_gate', 'bundle:11', '2026-09-26T07:16:34Z', GATE_CAUSE),
	openAlarm(212, 'change_without_gate', 'bundle:10', '2026-09-26T07:16:34Z', GATE_CAUSE),
	openAlarm(210, 'change_without_gate', 'bundle:6', '2026-09-26T07:16:34Z', GATE_CAUSE),
	openAlarm(209, 'change_without_gate', 'bundle:4', '2026-09-26T07:16:34Z', GATE_CAUSE),
	openAlarm(208, 'change_without_gate', 'bundle:2', '2026-09-26T07:16:34Z', GATE_CAUSE)
];

function setting(
	key: string,
	kind: SettingMeta['kind'],
	label: string,
	value: SettingMeta['value'],
	description: string
): SettingMeta {
	return {
		key,
		group: key.split('.')[0] ?? key,
		kind,
		scope: 'global',
		applies: 'next_cycle',
		default: value,
		label,
		description,
		value
	};
}

export const JUDGE_SETTINGS: SettingMeta[] = [
	setting('judge_worker.model', 'string', 'Model', 'google/gemini-3.8-flash', 'OpenRouter model.'),
	setting(
		'judge_worker.system_prompt',
		'text',
		'System prompt',
		'You label a finished turn: what was delivered, and how the operator reacted.',
		'Instructions.'
	),
	setting('judge_worker.batch_size', 'int', 'Turns per sweep', 50, 'How many turns one sweep claims.'),
	setting(
		'judge_worker.confidence_threshold',
		'float',
		'Confidence threshold',
		0.5,
		'Below this the answer is recorded as unknown.'
	),
	setting('judge_worker.interval_secs', 'int', 'Seconds between sweeps', 60, 'Wait between sweeps.'),
	setting('turn_judge.live_bundle_id', 'int', 'Configuration in use', 14, 'In use.'),
	setting(
		'turn_judge.immediate',
		'bool',
		'Judge a turn as soon as it is answered',
		true,
		'Judged soon after you reply.'
	),
	setting(
		'turn_judge.immediate_delay_ms',
		'int',
		'Immediate judging delay (ms)',
		1500,
		'How long it waits after a turn closes.'
	),
	setting('turn_judge.live_window_days', 'int', 'Live judge window (days)', 7, 'Days of turns.'),
	setting(
		'turn_judge.rejudge_daily_call_cap',
		'int',
		'History re-judge calls per day',
		300,
		'Most calls per day.'
	),
	setting('session_judge.model', 'string', 'Model', 'google/gemini-3.5-flash-lite', 'Model.'),
	setting(
		'session_judge.system_prompt',
		'text',
		'System prompt',
		'You read a finished session and say how it ended.',
		'Instructions.'
	),
	setting('session_judge.batch_size', 'int', 'Sessions per poll', 25, 'Sessions per poll.'),
	setting('session_judge.live_bundle_id', 'int', 'Configuration in use', 13, 'In use.'),
	setting(
		'session_judge.poll_interval_secs',
		'int',
		'Seconds between polls',
		120,
		'Wait between polls.'
	)
];

export const JUDGE_FEATURES: FeatureDto[] = [
	{ key: 'judge_worker', enabled: true, scope: 'global', applies: 'live_disable' },
	{ key: 'session_judge', enabled: true, scope: 'global', applies: 'on_restart' }
];

/*
 * The judges pages are worded for the operator, never for someone who thinks in eval-harness
 * terms. WORDING_BANNED is the standing check every Labelling-area story runs against its rendered
 * text; `mike eval run` / `mike eval rejudge` command text is the one allowed exception, since a
 * component prints those as a literal CLI command — textOutsideCode strips <code> content first
 * so that exception doesn't hide a real leak elsewhere on the page.
 */
export const WORDING_BANNED =
	/\b(gold|answer key|ground truth|recorded label|axis|tier|method|seed|holdout|candidate|bundle|verdict|rubric|kappa|eval|evaluations?|adjudicated|stratum|test runs?|go-live|disagreements?|trust checks?)\b|κ|\u2026/i;

export function textOutsideCode(root: Element): string {
	const clone = root.cloneNode(true) as Element;
	clone.querySelectorAll('code').forEach((el) => el.remove());
	return clone.textContent ?? '';
}
