import type {
	DecisionPointRow,
	DecisionSpend,
	DecisionsCalibration,
	PointScore
} from '$lib/api/decisions';

export function scoreFixture(
	point: string,
	over: Partial<PointScore> = {},
	calls = 120
): PointScore {
	return {
		counts: {
			point,
			calls,
			applied_on: Math.round(calls / 3),
			fail_open: 6,
			schema_errors: 1,
			refused: 2,
			question_refusals: 3,
			cost_usd: calls * 0.00021,
			latency_p50_ms: 420,
			latency_p95_ms: 1180,
			labelled: 54,
			labelled_last_7d: 19,
			...(over.counts ?? {})
		},
		auroc: 0.81,
		held_labelled: 0,
		held_wrong: 0,
		held_precision_lower: null,
		sent_labelled: 0,
		sent_right: 0,
		labels_per_day: 2.7,
		days_to_min_labels: null,
		applied_labelled: 40,
		applied_right: 33,
		applied_precision_lower: 0.68,
		findings: [],
		...over
	};
}

function point(
	id: string,
	label: string | null,
	area: string,
	over: Partial<DecisionPointRow> = {}
): DecisionPointRow {
	return {
		point: id,
		label,
		area,
		mode: 'on',
		mode_source: 'setting',
		score: null,
		...over
	};
}

export const SPEND_UNDER: DecisionSpend = { today_usd: 0.42, ceiling_usd: 5, reached: false };
export const SPEND_REACHED: DecisionSpend = { today_usd: 5.01, ceiling_usd: 5, reached: true };
export const SPEND_NO_CEILING: DecisionSpend = { today_usd: 12.5, ceiling_usd: 0, reached: false };

export function calibrationFixture(
	layerOn = true,
	spend: DecisionSpend = SPEND_UNDER
): DecisionsCalibration {
	return {
		layer: layerOn ? { on: true, reason: null } : { on: false, reason: 'off: no OpenRouter key' },
		spend,
		window_days: 14,
		last_loop_at: '2026-11-14T06:00:00Z',
		points: [
			point('turn_guard_precheck', 'Turn-guard pre-check', 'steering', {
				mode_source: 'decisions.turn_guard.mode',
				score: scoreFixture('turn_guard_precheck', {
					held_labelled: 9,
					held_wrong: 7,
					sent_labelled: 20,
					sent_right: 15,
					applied_labelled: 0,
					applied_right: 0,
					applied_precision_lower: null,
					findings: [
						'of 9 labelled held nudges, 7 were wrong, so the hold was correct; the lower bound of 0.45 is under the bar 0.60'
					]
				})
			}),
			point('verifier_precheck', 'Verifier pre-check', 'steering', {
				mode: 'shadow',
				score: scoreFixture('verifier_precheck', {}, 45)
			}),
			point('rule_check', 'Rule check', 'steering'),
			point('commit_message', 'Commit message check', 'steering', { mode: 'off' }),
			point('turn_judge', 'Turn judge', 'judges', {
				score: scoreFixture('turn_judge', {
					findings: [
						'the share of calls that failed open is 5.0%, above the bar 3.0%',
						'of 40 labelled applied decisions, the share that were right has a lower bound of 0.55, under the bar 0.60'
					]
				})
			}),
			point('session_judge', 'Session judge', 'judges', {
				score: scoreFixture('session_judge', { auroc: null, applied_labelled: 0 }, 12)
			}),
			point('lesson_supply', 'Lesson supply floor', 'memory and lessons', {
				score: scoreFixture('lesson_supply', {}, 410)
			}),
			point('topic_assign', 'Topic assignment', 'memory and lessons'),
			point('text_origin', 'Text origin', 'capture', {
				score: scoreFixture('text_origin', {}, 33)
			}),
			point('fleet_stop_check', 'Fleet stop check', 'fleet', {
				mode_source: 'fleet definition (decisions key, default on)',
				score: scoreFixture('fleet_stop_check', {}, 8)
			}),
			point('fleet_effort', 'Fleet effort split', 'fleet', {
				mode_source: 'fleet definition (decisions key, default on)'
			}),
			point('mail_reply_link', 'Mail reply link', 'launch, mail and tools')
		]
	};
}
