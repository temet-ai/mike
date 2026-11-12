import type {
	ActionEffectItem,
	EffectResult,
	GuardWake,
	ImprovementSummary,
	LessonCandidate,
	LessonDetail,
	MeasuredEffectResult,
	TrendPointDto,
	TrendResponse
} from '$lib/api/improvement';
import { toTrendPoints } from '$lib/api/improvement';
import type { DirectiveProposal } from '$lib/api/directiveProposals';

/*
 * "What the judge did" fixtures, shaped like the wire contracts in
 * artifacts/2026-09-29/judgement-at-work/design-spec.md §4. Used by every Improvement/* and
 * Routes/SessionsImprovement* story.
 */

/** A correction the operator typed in full: 500+ characters, several paragraphs, never clipped. */
const LONG_CORRECTION = `That test still fails locally — you didn't actually run it after the change. I can see the output in the terminal: the retry loop still gives up after the first attempt because the backoff is computed from the wrong field.

You said "verified" twice in the last reply, and neither time was there a test run behind it. Please stop saying something is verified unless you re-ran the exact test that was failing and showed me its output. If the test can't run in this environment, say so plainly instead of claiming a pass.

Re-run scripts/sb-test for the retry module now and paste the verdict line.`;

export const LESSON_CANDIDATES: LessonCandidate[] = [
	{
		id: 71,
		text: 'Leave the code better than you found it.',
		kind: 'correction',
		source: 'judge',
		evidenceCount: 1,
		correctingQuote:
			'This has been addressed i think before, we had a rule about leaving the code better than you found it. We need to incorporate that.',
		createdAt: '2026-09-28T09:12:00Z',
		sessionTitle: 'Review and refine system prompt with Mike MCP',
		sessionHref: '/sessions/hash-a1b2c3',
		turnHref: '/sessions/hash-a1b2c3/requests/501',
		projectLabel: 'temet-proxy',
		askText: 'Okay, do it and then lets go through the judgement parts'
	},
	{
		id: 68,
		text: 'Never claim a fix is verified without re-running the failing test.',
		kind: 'warning',
		source: 'judge',
		evidenceCount: 3,
		correctingQuote: LONG_CORRECTION,
		createdAt: '2026-09-27T16:40:00Z',
		sessionTitle: 'Fix the capture-DB retry loop',
		sessionHref: '/sessions/hash-d4e5f6',
		turnHref: '/sessions/hash-d4e5f6/requests/220',
		projectLabel: 'temet-proxy',
		askText:
			'The capture-DB retry loop gives up after one attempt. Find out why, fix it, and verify the fix.'
	},
	{
		id: 74,
		text: 'When a page has to show a long quoted message, show all of it and let it wrap or scroll; never cut it short with an ellipsis or a character limit, because the operator reads these pages to see exactly what was said.',
		kind: 'preference',
		source: 'judge',
		evidenceCount: 2,
		correctingQuote:
			'No, do not cut the quote off. I need to see every word of what I said, that is the whole point of the page.',
		createdAt: '2026-09-26T13:05:00Z',
		sessionTitle: 'Redesign the lesson review cards',
		sessionHref: '/sessions/hash-c7c7c7',
		turnHref: '/sessions/hash-c7c7c7/requests/61',
		projectLabel: 'night-watch-notes',
		askText: 'Make the lesson cards more compact so I can scan them.'
	},
	{
		id: 66,
		text: 'Commit and push finished work in the same session.',
		kind: 'convention',
		source: 'directive',
		evidenceCount: 1,
		correctingQuote: 'You finished it, so land it. Do not ask me whether to commit.',
		createdAt: '2026-09-25T08:30:00Z',
		sessionTitle: 'Tidy the release notes',
		sessionHref: '/sessions/hash-e8e8e8',
		turnHref: '/sessions/hash-e8e8e8/requests/14',
		projectLabel: 'No project',
		askText: ''
	}
];

export const DIRECTIVE_PROPOSALS: DirectiveProposal[] = [
	{
		id: 12,
		uuid: 'a3b1e6d2-0000-4000-8000-000000000012',
		source: 'repeated_directive',
		scope: 'project',
		project_id: 993478,
		class: 'shared_tree_gate',
		polarity: 'never',
		status: 'open',
		proposed_kind: 'convention',
		proposed_text: 'Never run `scripts/gate` in a working tree other sessions are editing.',
		evidence_count: 3,
		session_count: 2,
		first_evidence_at: '2026-09-20T08:00:00Z',
		last_evidence_at: '2026-09-27T11:00:00Z',
		created_at: '2026-09-27T11:05:00Z',
		decided_at: null,
		decision_note: null
	}
];

export const GUARD_WAKES: GuardWake[] = [
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
		operatorLabel: null
	},
	{
		id: 897,
		evaluationUuid: 'a1b2c3d4-0000-4000-8000-000000000897',
		questionKey: 'relapse',
		questionLabel: 'Does this reply repeat a correction you already made?',
		arm: 'append',
		sessionTitle: 'Fix the capture-DB retry loop',
		sessionHref: '/sessions/hash-d4e5f6',
		projectLabel: 'temet-proxy',
		projectPath: '/mnt/data/projects/temet/temet-proxy',
		turnHref: '/sessions/hash-d4e5f6/requests/231',
		deliveredAt: '2026-09-27T17:10:00Z',
		outcome: 'corrected',
		operatorLabel: null,
		noteText:
			'Earlier in this session the operator asked you not to claim a test passed without running it. This reply says "verified" again with no test run shown.',
		repeatUuids: ['a1b2c3d4-0000-4000-8000-000000000896']
	},
	{
		id: 905,
		evaluationUuid: 'a1b2c3d4-0000-4000-8000-000000000905',
		questionKey: 'unobserved_done',
		questionLabel: "Did the last turn's work actually ship?",
		arm: 'wake',
		sessionTitle: 'Redesign the lesson review cards',
		sessionHref: '/sessions/hash-c7c7c7',
		projectLabel: 'temet-proxy',
		projectPath: '/mnt/data/projects/temet/temet-proxy',
		turnHref: '/sessions/hash-c7c7c7/requests/77',
		deliveredAt: '2026-09-29T08:40:00Z',
		outcome: null,
		operatorLabel: null,
		noteText:
			'You said the story passes, but no test ran in this turn. Run it and show the verdict.'
	}
];

export const IMPROVEMENT_SUMMARY: ImprovementSummary = {
	lessonCandidateCount: LESSON_CANDIDATES.length,
	openProposalCount: DIRECTIVE_PROPOSALS.length,
	unlabelledWakeCount: 2,
	week: {
		lessonsApproved: 29,
		lessonsRejected: 10,
		wakes: 7,
		appends: 3,
		proposalsDecided: 2,
		proposalsApproved: 1,
		proposalsRejected: 1
	}
};

export const EFFECT_NOT_ENOUGH_DATA: MeasuredEffectResult = {
	status: 'not_enough_data',
	n: 12,
	readPoint: 30
};

export const EFFECT_FEWER_CORRECTIONS: MeasuredEffectResult = {
	status: 'measured',
	before: { sharePct: 27, n: 96, windowDays: 28 },
	after: { sharePct: 14, n: 88, windowDays: 28 },
	readPoint: 30,
	ciLowPct: 3,
	ciHighPct: 19,
	verdict: 'fewer_corrections'
};

export const EFFECT_NO_CHANGE: MeasuredEffectResult = {
	status: 'measured',
	before: { sharePct: 22, n: 60, windowDays: 28 },
	after: { sharePct: 21, n: 55, windowDays: 28 },
	readPoint: 30,
	ciLowPct: -8,
	ciHighPct: 6,
	verdict: 'no_change_seen'
};

/** A guard question's effect is always this shape on the actions feed — never not_enough_data/measured. */
export const EFFECT_RANDOMISED: EffectResult = {
	status: 'randomised',
	nHoldback: 12,
	nTreated: 41,
	readPoint: 34,
	detailHref: '/sessions/improvement/guard'
};

export const ACTION_EFFECTS: ActionEffectItem[] = [
	{
		kind: 'lesson',
		id: 68,
		label: 'A lesson: Never claim a fix is verified without re-running the failing test.',
		scope: 'temet-proxy',
		decidedAt: '2026-09-20T09:00:00Z',
		href: '/sessions/improvement/lessons/68',
		effect: EFFECT_FEWER_CORRECTIONS
	},
	{
		kind: 'guard_question',
		id: 'relapse',
		label: 'A guard question: Does this reply repeat a correction you already made?',
		scope: 'every project',
		decidedAt: null,
		href: '/sessions/improvement/guard',
		effect: EFFECT_RANDOMISED
	},
	{
		kind: 'lesson',
		id: 61,
		label: 'A lesson: Read the style guide before any visual change.',
		scope: 'temet-proxy',
		decidedAt: '2026-09-14T12:00:00Z',
		href: '/sessions/improvement/lessons/61',
		effect: EFFECT_NO_CHANGE
	},
	{
		kind: 'lesson',
		id: 72,
		label: 'A lesson: Use the shared button, never a hand-rolled one.',
		scope: 'temet-proxy',
		decidedAt: '2026-09-26T09:00:00Z',
		href: '/sessions/improvement/lessons/72',
		effect: EFFECT_NOT_ENOUGH_DATA
	},
	{
		kind: 'lesson',
		id: 69,
		label: 'A lesson: Name the model on every agent dispatch.',
		scope: 'every project',
		decidedAt: '2026-09-24T15:30:00Z',
		href: '/sessions/improvement/lessons/69',
		effect: { status: 'not_enough_data', n: 4, readPoint: 30 }
	},
	{
		kind: 'lesson',
		id: 75,
		label: 'A lesson: Reuse verified tool results when their inputs have not changed.',
		scope: 'temet-proxy',
		decidedAt: '2026-10-04T09:00:00Z',
		href: '/sessions/improvement/lessons/75',
		effect: { status: 'not_computed' }
	}
];

const KIMI = 'legacy:moonshotai/kimi-k3:r1-2026-06-22';
const GEMINI = 'legacy:google/gemini-3-flash-preview:r3-2026-08-29';
const UNKNOWN = 'legacy:none:none';

function week(
	weekEnd: string,
	k: number,
	n: number,
	judges: [string, number | null, number][]
): TrendPointDto {
	return {
		week_end: `${weekEnd}T00:00:00Z`,
		k,
		n,
		judges: judges.map(([label, bundle_id, count]) => ({ label, bundle_id, count }))
	};
}

/** Mirrors the live capture on 2026-09-29: the judge changed twice inside the window. */
export const TREND_POINT_DTOS: TrendPointDto[] = [
	week('2026-08-11', 54, 184, [
		[KIMI, null, 157],
		[UNKNOWN, null, 27]
	]),
	week('2026-08-18', 78, 321, [
		[KIMI, null, 311],
		[UNKNOWN, null, 10]
	]),
	week('2026-08-25', 68, 447, [
		[KIMI, null, 422],
		[UNKNOWN, null, 25]
	]),
	week('2026-09-01', 43, 238, [
		[KIMI, null, 204],
		[GEMINI, null, 17],
		[UNKNOWN, null, 17]
	]),
	week('2026-09-08', 10, 388, [
		[GEMINI, null, 368],
		[UNKNOWN, null, 20]
	]),
	week('2026-09-15', 6, 115, [
		[GEMINI, null, 101],
		[UNKNOWN, null, 14]
	]),
	week('2026-09-22', 19, 225, [
		[GEMINI, null, 170],
		['bundle:14', 14, 28],
		['bundle:1', 1, 19],
		[UNKNOWN, null, 7]
	]),
	week('2026-09-29', 71, 283, [
		['bundle:14', 14, 238],
		['bundle:1', 1, 45]
	])
];

export const TREND_RESPONSE: TrendResponse = {
	weekly: toTrendPoints(TREND_POINT_DTOS),
	markers: [
		{
			date: '2026-09-20T10:00:00Z',
			label:
				'A lesson was approved: Never claim a fix is verified without re-running the failing test.',
			href: '/sessions/improvement/lessons/68'
		}
	]
};

/** Eight weeks with one judge throughout, so no change warning shows. */
export const TREND_RESPONSE_ONE_JUDGE: TrendResponse = {
	weekly: toTrendPoints(
		[62, 71, 65, 74, 80, 78, 73, 68].map((n, i) =>
			week(
				new Date(Date.UTC(2026, 7, 11 + i * 7)).toISOString().split('T')[0] ?? '',
				[15, 18, 14, 19, 15, 16, 15, 10][i] ?? 0,
				n,
				[['bundle:14', 14, n]]
			)
		)
	),
	markers: []
};

export const LESSON_DETAIL_RAW: LessonDetail = {
	id: 71,
	text: 'Leave the code better than you found it.',
	kind: 'correction',
	source: 'judge',
	stage: 'raw',
	evidenceCount: 1,
	createdAt: '2026-09-27T11:32:00Z',
	askText: 'Okay, do it and then lets go through the judgement parts',
	assistantEndingText:
		'six numbered decisions, including one that left an established rule unincorporated.',
	correctingQuote:
		'This has been addressed i think before, we had a rule about leaving the code better than you found it. We need to incorporate that.',
	sessionTitle: 'Review and refine system prompt with Mike MCP',
	sessionHref: '/sessions/hash-a1b2c3',
	turnHref: '/sessions/hash-a1b2c3/requests/501',
	judgeRationale:
		'The reply points out a missed, previously-established rule that must be incorporated.',
	similarCorrections: [
		{
			turnHref: '/sessions/hash-d4e5f6/requests/220',
			sessionTitle: 'Fix the capture-DB retry loop',
			createdAt: '2026-09-12T10:00:00Z',
			correctingQuote:
				"That test still fails locally — you didn't actually run it after the change.",
			existingLessonText: null,
			existingLessonHref: null
		}
	],
	reinforcingCorrections: [],
	injectionCount: null,
	wentLiveAt: null,
	effect: null,
	outcomes: { judged: 0, used: 0, correctedAnyway: 0, ignored: 0 },
	retirement: null
};

export const LESSON_DETAIL_LIVE: LessonDetail = {
	...LESSON_DETAIL_RAW,
	id: 68,
	text: 'Never claim a fix is verified without re-running the failing test.',
	stage: 'daily',
	evidenceCount: 3,
	reinforcingCorrections: [
		{
			turnHref: '/sessions/hash-77bb22/requests/305',
			sessionTitle: 'Confirm the retention sweep',
			createdAt: '2026-09-22T08:00:00Z',
			correctingQuote: 'You said this passed but the log shows it never ran.',
			existingLessonText: null,
			existingLessonHref: null
		}
	],
	injectionCount: 4,
	wentLiveAt: '20 Sep 2026',
	effect: EFFECT_FEWER_CORRECTIONS,
	outcomes: { judged: 9, used: 7, correctedAnyway: 1, ignored: 1 }
};

export const LESSON_DETAIL_RETIRED: LessonDetail = {
	...LESSON_DETAIL_LIVE,
	id: 64,
	text: 'Mailbox delivery can land in subagent requests, not the main loop.',
	effect: null,
	outcomes: { judged: 12, used: 1, correctedAnyway: 2, ignored: 9 },
	retirement: {
		kind: 'superseded',
		reason:
			'Contradicted by the newer lesson 68, and the adjudicator agreed. The newer lesson says the bug is fixed.',
		retiredAt: '2026-11-12T01:04:00Z',
		byLessonId: 68
	}
};
