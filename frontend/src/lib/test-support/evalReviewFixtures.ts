import type { EvalLabelDto, VocabularyAxis } from '$lib/api/eval';
import type { OperatorTurnDto, SessionSummary } from '$lib/types';
import type {
	BundleDto,
	EvalReviewContextDto,
	EvalReviewDetailDto,
	EvalRunDto,
	EvalRunItemDto,
	OperatorTurnFactsDto
} from '$lib/api/evalRuns';

export const SESSION = '7c9e6679-7425-40de-944b-e07fc1f90ae7';
export const SHA = 'b9ddc31e1417974bcf1212d5c582a421f346158441e8b01a477b09d79be00c26';

const DEFINITIONS: Record<string, string> = {
	corrected:
		'The next genuine message says or implies the assistant got something wrong in this turn: a mistake, a misread ask, a false or unchecked claim, an ignored instruction, wasted effort, or work not done. Re-asking and challenging questions count.',
	redirected:
		"The operator changes direction, scope or priority for reasons that are not the assistant's fault.",
	accepted:
		'The operator builds on the turn, approves it, thanks, answers a fair question, or gives the next step in the same line of work.',
	no_response: 'No genuine message followed. Not a verdict, and never "abandoned".',
	unclear: 'A next message exists but cannot be read as any of the above without guessing.',
	shipped:
		'Delivered what was asked and an irreversible delivery action happened in this turn: commit, push, pull request, merge, deploy or release.',
	delivered:
		'Reports the asked-for work done or the question answered, with no irreversible action.',
	partial:
		'Some of the ask is done and the rest is explicitly left, blocked, deferred or still running.',
	none: 'No real work: refused, only planned, cut short before any work, or only chatter.',
	question:
		'The main output is a question or decision put to the operator, with little of the ask done.',
	not_applicable: 'The prompt asks for no work: thanks, a status ping, an automated event.'
};

export const VOCABULARY: VocabularyAxis[] = [
	{
		axis: 'reaction',
		subject_kinds: ['turn'],
		labels: ['corrected', 'redirected', 'accepted', 'no_response', 'unclear'].map((label, i) => ({
			label,
			sort_order: i + 1,
			description: DEFINITIONS[label] ?? label
		}))
	},
	{
		axis: 'delivery',
		subject_kinds: ['turn'],
		labels: ['shipped', 'delivered', 'partial', 'none', 'question', 'not_applicable'].map(
			(label, i) => ({
				label,
				sort_order: i + 1,
				description: DEFINITIONS[label] ?? label
			})
		)
	}
];

export const RUN: EvalRunDto = {
	id: 3,
	created_at: '2026-09-25T19:02:21Z',
	finished_at: '2026-09-25T19:21:00Z',
	purpose: 'measure',
	status: 'complete',
	bundle_id: 15,
	rubric_version: 'v2-2026-09-23',
	input_builder_version: 'judge-input-v2.1',
	slices: [],
	model: 'google/gemini-3.8-flash',
	split: 'dev',
	tiers: ['seed', 'gold', 'relayed'],
	holdout_batch: null,
	labels_as_of: '2026-09-25T19:02:21Z',
	label_count: 278,
	label_ids_hash: 'h',
	repeat_of_run_id: null,
	max_usd: 5,
	cost_usd: 0.478158,
	items_judged: 277,
	items_refused: 0,
	items_failed: 0,
	report: {
		n: 277,
		items_skipped: 1,
		reaction: { n: 277, agreement: 239, kappa: 0.8022766548951837 },
		delivery: { n: 277, agreement: 214, kappa: 0.684099055066797 },
		corrected: {
			tp: 57,
			fp: 8,
			fn: 17,
			precision: 0.8769230769230769,
			recall: 0.7702702702702703,
			f1: 0.82
		}
	},
	argv: {}
};

export const BUNDLE_15: BundleDto = {
	id: 15,
	role: 'turn_judge',
	origin: 'candidate',
	status: 'candidate',
	content_sha256: SHA
};
export const BUNDLE_14: BundleDto = {
	id: 14,
	role: 'turn_judge',
	origin: 'candidate',
	status: 'live_ungated',
	content_sha256: SHA
};

export function item(turn: number, extra: Partial<EvalRunItemDto> = {}): EvalRunItemDto {
	return {
		run_id: 3,
		operator_turn_id: turn,
		reaction_label_id: turn * 10,
		delivery_label_id: turn * 10 + 1,
		turn_verdict_id: turn,
		outcome: 'judged',
		stratum: 'all',
		inclusion_prob: null,
		reaction_correct: false,
		delivery_correct: true,
		created_at: '2026-09-25T19:03:00Z',
		gold_reaction: 'corrected',
		gold_delivery: 'delivered',
		gold_control: false,
		verdict_reaction: 'accepted',
		verdict_delivery: 'delivered',
		verdict_legacy_label: null,
		next_state: 'resolved',
		verdict: {
			id: turn,
			reaction: extra.verdict_reaction ?? 'accepted',
			delivery: extra.verdict_delivery ?? 'delivered',
			confidence: 0.9,
			rationale: `Stored rationale for turn ${turn}.`,
			unmet_ask: '',
			evidence_quality: 'verified',
			severity: null,
			model: 'google/gemini-3.8-flash',
			bundle_id: 15
		},
		gold_severity: 'minor',
		current_reaction: 'corrected',
		current_delivery: 'delivered',
		...extra
	};
}

export const TURN_13778 = item(13778, {
	verdict_reaction: 'redirected',
	gold_severity: 'severe',
	verdict: {
		id: 623,
		reaction: 'redirected',
		delivery: 'delivered',
		confidence: 0.95,
		rationale:
			"The assistant analyzed why the 'one more thing' habit persisted despite filter fixes and updated the prompt rules across both layers. The user expressed deep frustration with the ongoing behavior and redirected the strategy to a complete overhaul involving teams of agents and online research.",
		unmet_ask: '',
		evidence_quality: 'verified',
		severity: null,
		model: 'google/gemini-3.8-flash',
		bundle_id: 15
	}
});

export const SKIPPED_16303 = item(16303, {
	outcome: 'skipped_unresolved',
	reaction_label_id: null,
	delivery_label_id: null,
	turn_verdict_id: null,
	reaction_correct: null,
	delivery_correct: null,
	gold_reaction: null,
	gold_delivery: null,
	verdict_reaction: null,
	verdict_delivery: null,
	verdict: null,
	gold_severity: null,
	current_reaction: null,
	current_delivery: null
});

export function label(
	turn: number,
	axis: string,
	value: string,
	extra: Partial<EvalLabelDto> = {}
): EvalLabelDto {
	return {
		id: turn * 10 + (axis === 'reaction' ? 0 : 1),
		subject_kind: 'turn',
		subject_id: turn,
		operator_turn_id: turn,
		session_uuid: SESSION,
		axis,
		label: value,
		action: 'assert',
		tier: 'seed',
		method: 'double_blind_adjudicated',
		split: 'dev',
		tags: [],
		severity: 'severe',
		confidence: null,
		evidence: 'Operator: "this is the same habit again, overhaul it with a team of agents".',
		note: null,
		source: 'fleet_mirror/ground_truth',
		provenance: { adjudicated: true },
		verdict_snapshot: null,
		supersedes_id: null,
		created_at: '2026-09-24T10:00:00Z',
		is_current: true,
		...extra
	};
}

export function context(
	turn: number,
	extra: Partial<EvalReviewContextDto> = {}
): EvalReviewContextDto {
	return {
		operator_turn_id: turn,
		session_uuid: SESSION,
		closed_at: '2026-09-20T10:00:00Z',
		user_prompt: 'Why does the "one more thing" habit keep coming back? Fix it for good.',
		final_assistant_text:
			'I traced the habit to two prompt layers and updated both rules. The filter now blocks the pattern.',
		facts_present: true,
		text_chars: 96,
		text_cut_at: 96,
		tool_call_count: 12,
		tool_error_count: 0,
		next_state: 'resolved',
		next_genuine_turn_id: turn + 1,
		next_genuine_request_id: 99001,
		next_genuine_at: '2026-09-20T10:05:00Z',
		next_genuine_text:
			'This is the same habit again. Stop patching — overhaul it with a team of agents and research online.',
		prompt_state: 'present',
		assistant_state: 'present',
		next_message_state: 'present',
		...extra
	};
}

export function detailFor(
	it: EvalRunItemDto,
	extra: Partial<EvalReviewDetailDto> = {}
): EvalReviewDetailDto {
	const gold = it.gold_reaction
		? [
				label(it.operator_turn_id, 'reaction', it.gold_reaction),
				label(it.operator_turn_id, 'delivery', it.gold_delivery ?? 'delivered')
			]
		: [];
	return {
		item: it,
		context: context(it.operator_turn_id),
		labels: { run_gold: gold, current: gold, reviews: [] },
		...extra
	};
}

export function reviewLabels(turn: number, reaction: string, note: string | null): EvalLabelDto[] {
	const base = {
		tier: 'gold' as const,
		method: 'operator_click',
		source: 'spa',
		severity: null,
		evidence: null,
		note,
		provenance: { context: 'eval_review', eval_run_id: 3 },
		created_at: '2026-09-28T09:00:00Z'
	};
	return [
		label(turn, 'reaction', reaction, { ...base, id: turn * 10 + 5 }),
		label(turn, 'delivery', 'delivered', { ...base, id: turn * 10 + 4 })
	];
}

/*
 * Run-3-shaped turns for the Review labels page. Turn ids, labels, counts and times follow
 * the real run 3 rows; every conversation text is written for these fixtures, never copied from
 * the capture.
 */

export const SESSION_10487 = '1aa88740-9dbc-4f62-beca-9021cab941fe';

export function runRow(
	turn: number,
	gold: [string, string],
	judge: [string, string],
	current: [string, string] = gold,
	extra: Partial<EvalRunItemDto> = {}
): EvalRunItemDto {
	return item(turn, {
		gold_reaction: gold[0],
		gold_delivery: gold[1],
		verdict_reaction: judge[0],
		verdict_delivery: judge[1],
		current_reaction: current[0],
		current_delivery: current[1],
		reaction_correct: gold[0] === judge[0],
		delivery_correct: gold[1] === judge[1],
		gold_severity: null,
		reviewed_at: null,
		...extra
	});
}

export const RUN3_DELIVERY_FIRST_10: EvalRunItemDto[] = [
	runRow(10487, ['accepted', 'partial'], ['accepted', 'delivered'], ['redirected', 'partial']),
	runRow(10954, ['no_response', 'partial'], ['no_response', 'delivered']),
	runRow(10955, ['corrected', 'shipped'], ['corrected', 'delivered'], ['accepted', 'shipped']),
	runRow(11015, ['corrected', 'partial'], ['corrected', 'delivered']),
	runRow(11221, ['accepted', 'delivered'], ['accepted', 'partial']),
	runRow(11356, ['no_response', 'delivered'], ['no_response', 'shipped']),
	runRow(11591, ['accepted', 'partial'], ['accepted', 'delivered']),
	runRow(11794, ['corrected', 'none'], ['corrected', 'partial']),
	runRow(12180, ['corrected', 'partial'], ['accepted', 'delivered']),
	runRow(12427, ['no_response', 'partial'], ['no_response', 'shipped'])
];

const PROMPT_10487 =
	'I have a client session next week about the friction in their sign-in flow. Their single sign-on step is slow and developers lose time to it every day. Can I build a small browser extension that records every time I have to sign in, and how long it takes from the first screen to a successful login?';

export const QUOTE_10487 =
	'"You stepped away to make tea" is also a good signal, since the sign-in prompt pulls you off the screen and onto a phone, and if you get distracted that is a failure of the sign-in system too.';

const REPLY_10487 = `${QUOTE_10487} Compare the two: open the tracker, do some work on a ticket, carry on; or open it, hit the sign-in screen, and either give up, come back later, or wait. That is a good signal to hand to someone.`;

const STOP_10487 = [
	'Baseline committed at `b0b295e`. The build workflow is running in the background — 11 agents across build, independent verify, fix, and a live-browser harness.',
	'**What is being built**',
	'An episode is one attempt to get through sign-in. It starts when a tab hits the sign-in page and ends on success only after the tab has been off the sign-in host for 3 seconds, because the redirect bounces between hosts and a naive rule would close the episode on the first hop.',
	'Per episode: duration, phase sequence, retries, push timeouts, classified failures, which app you were trying to reach, and whether the tab was blurred.'
].join('\n\n');

const FULL_10487 = [
	'Empty directory, so I will design first and then hand the build to workers.',
	'I checked the extension manifest rules and the storage limits before writing the episode model.',
	'Two independent verifiers found a race in the service worker; I fixed both and reran them.',
	STOP_10487,
	'The verifier found one more gap in the blur detection, so I patched it.',
	'Now run the shared-module test suite to confirm no regressions.'
].join('');

const INPUT_10487 = `OPERATOR REQUEST (user prompt):\n${PROMPT_10487}\n\nASSISTANT FINAL TEXT (tail):\nThe verifier found one more gap in the blur detection, so I patched it.Now run the shared-module test suite to confirm no regressions.\n\nNEXT GENUINE OPERATOR MESSAGE:\n${REPLY_10487}`;

const REASON_10487 =
	'Disputed field (delivery): I side with the labeller who chose PARTIAL. The extension was built and verified, but the build workflow was still running in the background when the operator replied, so part of the ask was left running.';

export const TURN_10487: EvalRunItemDto = {
	...runRow(10487, ['accepted', 'partial'], ['accepted', 'delivered'], ['redirected', 'partial']),
	turn_verdict_id: 516,
	verdict: {
		id: 516,
		reaction: 'accepted',
		delivery: 'delivered',
		confidence: 0.95,
		rationale:
			"The assistant built the sign-in timing extension and ran verification tests. The operator accepted the work and built on it with another metric idea ('stepped away to make tea').",
		unmet_ask: '',
		evidence_quality: 'verified',
		severity: null,
		model: 'google/gemini-3.8-flash',
		bundle_id: 15,
		input_text: INPUT_10487
	}
};

function runGold(
	turn: number,
	axis: string,
	value: string,
	evidence: string,
	reason: string | null
): EvalLabelDto {
	return label(turn, axis, value, {
		id: turn * 10 + (axis === 'reaction' ? 1 : 2),
		severity: null,
		evidence,
		provenance: reason
			? { adjudicated: true, final_extra: { failure: '', reason } }
			: { adjudicated: true },
		created_at: '2026-09-25T10:14:52Z'
	});
}

export const DETAIL_10487: EvalReviewDetailDto = {
	item: TURN_10487,
	context: context(10487, {
		session_uuid: SESSION_10487,
		closed_at: '2026-07-27T08:25:16Z',
		user_prompt: PROMPT_10487,
		final_assistant_text: FULL_10487,
		text_chars: FULL_10487.length,
		text_cut_at: FULL_10487.length,
		tool_call_count: 212,
		tool_error_count: 5,
		next_genuine_turn_id: 10493,
		next_genuine_request_id: 212877,
		next_genuine_at: '2026-07-27T08:24:40Z',
		next_genuine_text: REPLY_10487
	}),
	labels: {
		run_gold: [
			runGold(10487, 'reaction', 'accepted', QUOTE_10487, REASON_10487),
			runGold(10487, 'delivery', 'partial', QUOTE_10487, REASON_10487)
		],
		current: [
			label(10487, 'reaction', 'redirected', {
				id: 3350,
				method: 'single_model',
				evidence: QUOTE_10487,
				provenance: {}
			}),
			runGold(10487, 'delivery', 'partial', QUOTE_10487, REASON_10487)
		],
		reviews: []
	}
};

export function facts(
	turn: number,
	extra: Partial<OperatorTurnFactsDto> = {}
): OperatorTurnFactsDto {
	return {
		turn_id: turn,
		session_id: SESSION,
		start_request_id: turn * 10,
		end_request_id: turn * 10 + 9,
		start_request_number: 1,
		end_request_number: 9,
		stop_request_id: turn * 10 + 5,
		created_at: '2026-09-20T09:58:00Z',
		duration_ms: 90000,
		tool_call_count: 12,
		tool_error_count: 0,
		files_touched: ['src/lib/filter.ts'],
		tools_used: ['Read', 'Edit'],
		skills_used: [],
		agents_dispatched: [],
		stop_response: 'I traced the habit to two prompt layers and updated both rules.',
		stop_reason: 'end_turn',
		interrupted: false,
		...extra
	};
}

export const FACTS_10487: OperatorTurnFactsDto = facts(10487, {
	session_id: SESSION_10487,
	start_request_id: 212616,
	end_request_id: 212888,
	start_request_number: 4,
	end_request_number: 185,
	stop_request_id: 212708,
	created_at: '2026-07-27T08:03:32Z',
	duration_ms: 1322000,
	tool_call_count: 212,
	tool_error_count: 5,
	files_touched: Array.from(
		{ length: 34 },
		(_, i) => `/work/signin-tracker/src/module-${i + 1}.js`
	),
	stop_response: STOP_10487
});

export const TURN_10571: EvalRunItemDto = {
	...runRow(10571, ['unclear', 'none'], ['corrected', 'none']),
	verdict: {
		id: 571,
		reaction: 'corrected',
		delivery: 'none',
		confidence: 0.9,
		rationale:
			"The assistant gave an empty response to the user's question about visiting Kyoto. The user then repeated the exact same question, which counts as corrected.",
		unmet_ask: 'What is the best month to visit Kyoto?',
		evidence_quality: 'verified',
		severity: null,
		model: 'google/gemini-3.8-flash',
		bundle_id: 15,
		input_text:
			'OPERATOR REQUEST (user prompt):\nWhat is the best month to visit Kyoto?\n\nNEXT GENUINE OPERATOR MESSAGE:\nWhat is the best month to visit Kyoto?'
	}
};

const REASON_10571 =
	'Disputed field (label): I side with the labeller who chose UNCLEAR. The next message repeats the prompt word for word with no comment, and this record cannot show why it was repeated.';

export const DETAIL_10571: EvalReviewDetailDto = {
	item: TURN_10571,
	context: context(10571, {
		user_prompt: 'What is the best month to visit Kyoto?',
		final_assistant_text: null,
		text_chars: 0,
		text_cut_at: 0,
		assistant_state: 'empty_turn',
		next_genuine_at: '2026-07-27T08:59:22Z',
		next_genuine_text: 'What is the best month to visit Kyoto?'
	}),
	labels: {
		run_gold: [
			runGold(10571, 'reaction', 'unclear', 'What is the best month to visit Kyoto?', REASON_10571),
			runGold(10571, 'delivery', 'none', 'What is the best month to visit Kyoto?', REASON_10571)
		],
		current: [
			runGold(10571, 'reaction', 'unclear', 'What is the best month to visit Kyoto?', REASON_10571),
			runGold(10571, 'delivery', 'none', 'What is the best month to visit Kyoto?', REASON_10571)
		],
		reviews: []
	}
};

export const TURN_12180: EvalRunItemDto = {
	...runRow(12180, ['corrected', 'partial'], ['accepted', 'delivered']),
	verdict: {
		id: 1218,
		reaction: 'accepted',
		delivery: 'delivered',
		confidence: 0.85,
		rationale:
			'The assistant replaced the viewer as asked and the operator moved on to the next task.',
		unmet_ask: '',
		evidence_quality: 'verified',
		severity: null,
		model: 'google/gemini-3.8-flash',
		bundle_id: 15,
		input_text: 'OPERATOR REQUEST (user prompt):\nSwap the log viewer for the new one.'
	}
};

export const DETAIL_12180: EvalReviewDetailDto = {
	item: TURN_12180,
	context: context(12180, {
		user_prompt: 'Swap the log viewer for the new one.',
		final_assistant_text: 'I replaced the old viewer with the new one and removed the old files.',
		text_chars: 70,
		text_cut_at: 70,
		next_genuine_at: '2026-08-03T12:28:16Z',
		next_genuine_text: 'The old viewer was also pretty good. I would not throw it away.'
	}),
	labels: {
		run_gold: [
			runGold(12180, 'reaction', 'corrected', 'The old viewer was also pretty good.', null),
			runGold(12180, 'delivery', 'partial', 'The old viewer was also pretty good.', null)
		],
		current: [
			runGold(12180, 'reaction', 'corrected', 'The old viewer was also pretty good.', null),
			runGold(12180, 'delivery', 'partial', 'The old viewer was also pretty good.', null)
		],
		reviews: []
	}
};

export const FACTS_12180: OperatorTurnFactsDto = facts(12180, {
	created_at: '2026-08-03T12:04:00Z',
	duration_ms: 600000,
	stop_response: 'I replaced the old viewer with the new one and removed the old files.'
});

export function runItemsPage(items: EvalRunItemDto[], total = items.length) {
	return {
		items,
		total,
		reviewed: items.filter((i) => i.reviewed_at).length,
		limit: 500,
		offset: 0
	};
}

/*
 * A 13-turn session shaped like 24369cc6 (run 4, turn 10695 is turn 9 of 13). Turn ids and times
 * follow the real rows; every text below is written for these fixtures.
 */

export const SESSION_24369 = '24369cc6-0eab-4bf3-9e7d-268af21036d9';

export const SUMMARY_24369: SessionSummary = {
	id: SESSION_24369,
	project: '/work/temet-proxy',
	project_id: 7,
	cwd: '/work/temet-proxy',
	project_resolution: 'git',
	first_seen: '2026-07-27T06:33:42Z',
	last_seen: '2026-07-27T10:40:14Z',
	request_count: 681,
	models: ['claude-opus'],
	last_status: 'complete',
	title: 'Review and refine the system prompt',
	narrative_overview:
		'Consolidating the prompt rules for the proxy, so that conflicting directives stop fighting and duplicate rule files go away.\n\nThe consolidation shipped: the duplicate files were deleted, one pattern list replaced three copies, and the rebuilt proxy was checked live against every gate.'
};

const TURN_IDS_24369 = [
	10454, 10462, 10467, 10488, 10492, 10496, 10690, 10692, 10695, 10949, 11003, 11007, 11010
];

export const TURNS_24369: OperatorTurnDto[] = TURN_IDS_24369.map((turn_id, i) => ({
	turn_id,
	session_id: SESSION_24369,
	project_id: 7,
	start_request_id: 212070 + i * 300,
	end_request_id: 212070 + i * 300 + 200,
	start_request_number: i * 50 + 1,
	end_request_number: i * 50 + 40,
	user_prompt:
		i === 7
			? 'How do we make this easy to keep improving? I worry the work we did is not being used.'
			: i === 8
				? 'please tell me the fourth copy is not still in place.'
				: `Prompt of turn ${i + 1}: keep going with the consolidation.`,
	final_assistant_text: '',
	tool_call_count: 10,
	tool_error_count: 0,
	files_touched: [],
	tools_used: [],
	agents_dispatched: [],
	weasel_findings_count: 0,
	total_input_tokens: 0,
	total_output_tokens: 0,
	duration_ms: 120000,
	outcome: 'unknown',
	outcome_evidence: null,
	classified_at: null,
	created_at: new Date(Date.UTC(2026, 6, 27, 6, 35 + i * 15)).toISOString(),
	updated_at: '2026-07-27T11:00:00Z',
	stop_response:
		i === 7
			? 'Your worry is fair, and today shows it: I built a **fourth** copy of a list this project already had, in `analysis/categories.rs`.\n\nOne list should feed every reader.'
			: `Last message of turn ${i + 1}.`,
	stop_reason: 'end_turn',
	prompt_kind: 'operator'
}));

const JC11_PROVENANCE = {
	labellers: { A: 'sonnet effort medium', B: 'opus, reaction-first', adjudicator: 'opus' },
	A: {
		label: 'CORRECTED',
		delivery: 'partial',
		failure: 'Ended mid-task without finishing the check it announced.'
	},
	B: {
		label: 'CORRECTED',
		delivery: 'partial',
		failure: 'Stopped without confirming the extra copy was removed.'
	},
	final_reason: null
};

export const TURN_10695: EvalRunItemDto = {
	...runRow(10695, ['corrected', 'partial'], ['accepted', 'partial']),
	run_id: 4,
	verdict: {
		id: 835,
		reaction: 'accepted',
		delivery: 'partial',
		confidence: 0.95,
		rationale: 'The operator asked to make sure the work is done, which continues the same work.',
		unmet_ask: 'Running the check that proves the extra copy is gone.',
		evidence_quality: 'verified',
		severity: null,
		model: 'google/gemini-3.8-flash',
		bundle_id: 14,
		input_text:
			'OPERATOR REQUEST (user prompt):\nplease tell me the fourth copy is not still in place.\n\nASSISTANT TURN TEXT (whole turn, in order; cut at the front when long):\nwhen you asked. It is not now: the list is gone and the reader uses the one the project already had.\n\nTOOL USE SUMMARY:\ntool_call_count = 107  tool_error_count = 0\n\nOPERATOR\'S NEXT GENUINE MESSAGE (automated events removed):\nensure it is done and working properly'
	}
};

const LABEL_10695 = label(10695, 'reaction', 'corrected', {
	id: 5333,
	tags: ['stopped-short'],
	evidence: 'ensure it is done and working properly',
	provenance: JC11_PROVENANCE,
	created_at: '2026-09-28T09:46:38Z'
});

export const DETAIL_10695: EvalReviewDetailDto = {
	item: TURN_10695,
	context: context(10695, {
		session_uuid: SESSION_24369,
		user_prompt: 'please tell me the fourth copy is not still in place.',
		next_genuine_turn_id: 10949,
		next_genuine_request_id: 215838,
		next_genuine_at: '2026-07-27T09:52:07Z',
		next_genuine_text: 'ensure it is done and working properly'
	}),
	labels: {
		run_gold: [LABEL_10695, label(10695, 'delivery', 'partial', { id: 5334, provenance: JC11_PROVENANCE })],
		current: [LABEL_10695, label(10695, 'delivery', 'partial', { id: 5334, provenance: JC11_PROVENANCE })],
		reviews: []
	}
};

export const HISTORY_10695: EvalLabelDto[] = [
	LABEL_10695,
	label(10695, 'fact_own_prompt_corrects_prior', 'true', { id: 5335, created_at: '2026-09-28T09:46:38Z' }),
	label(10695, 'failure_tag', 'handed-back-actions-or-decisions', {
		id: 4633,
		method: 'single_model',
		created_at: '2026-09-25T10:14:52Z'
	}),
	label(10695, 'reaction', 'corrected', {
		id: 2946,
		method: 'single_model',
		tags: ['stopped-short', 'asked-instead-of-acting'],
		provenance: { failure: 'It never answered whether the extra copy was gone.' },
		created_at: '2026-09-25T10:14:52Z'
	})
];

export const FACTS_10695: OperatorTurnFactsDto = facts(10695, {
	session_id: SESSION_24369,
	start_request_id: 215127,
	created_at: '2026-07-27T09:32:52Z',
	duration_ms: 1200000,
	tool_call_count: 107,
	files_touched: Array.from({ length: 11 }, (_, i) => `/work/temet-proxy/src/file-${i + 1}.rs`),
	stop_response:
		'It was still in place when you asked. It is not now: the list is gone and the reader uses the one the project already had.'
});

export const FACT_VOCABULARY: VocabularyAxis[] = [
	{
		axis: 'fact_own_prompt_corrects_prior',
		subject_kinds: ['turn'],
		labels: [
			{ label: 'true', sort_order: 1, description: "This turn's own prompt corrects an earlier turn." },
			{
				label: 'false',
				sort_order: 2,
				description: "This turn's own prompt does not correct an earlier turn."
			}
		]
	},
	{
		axis: 'failure_tag',
		subject_kinds: ['turn'],
		labels: [
			{
				label: 'handed-back-actions-or-decisions',
				sort_order: 1,
				description:
					'Hands back something the assistant could do itself: asks permission for, or flags as a decision, something already authorised or within its remit.'
			}
		]
	}
];
