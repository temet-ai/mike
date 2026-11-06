import { describe, expect, it } from 'vitest';
import type { EvalLabelDto } from '$lib/api/eval';
import type { OperatorTurnFactsDto } from '$lib/api/evalRuns';
import { accuracy, itemsQueryString } from '$lib/api/evalRuns';
import {
	VOCABULARY,
	context,
	item,
	label,
	reviewLabels
} from '$lib/test-support/evalReviewFixtures';
import {
	DEFAULT_FILTERS,
	activeFilterCount,
	byTurn,
	candidates,
	citedSpan,
	disagreementCount,
	disputedAxes,
	filtersFromParams,
	firstUndecided,
	cardHeading,
	formatDay,
	formatDuration,
	formatWhen,
	isTextTarget,
	judgeInputSections,
	judgeSawLastMessage,
	labellingWho,
	labellingWhy,
	latestReview,
	markSegments,
	methodSentence,
	neighbour,
	nextUndecided,
	notGradedReason,
	otherAxisNote,
	paramsFor,
	queueQuery,
	quotedEvidence,
	shortcutFor,
	stillRunning,
	stopWords,
	turnPosition,
	wholePercent,
	LABELLING_RULES,
	judgeMissedLast,
	nameSides
} from './evalReview';

const key = (
	k: string,
	mods: Partial<{
		ctrlKey: boolean;
		metaKey: boolean;
		altKey: boolean;
		shiftKey: boolean;
		code: string;
	}> = {}
) => ({
	key: k,
	code: /^\d$/.test(k) ? `Digit${k}` : `Key${k.toUpperCase()}`,
	ctrlKey: false,
	metaKey: false,
	altKey: false,
	shiftKey: false,
	...mods
});

function facts(extra: Partial<OperatorTurnFactsDto> = {}): OperatorTurnFactsDto {
	return {
		turn_id: 1,
		session_id: 's',
		start_request_id: 1,
		end_request_id: 2,
		start_request_number: 1,
		end_request_number: 2,
		stop_request_id: 2,
		created_at: '2026-07-27T08:03:32Z',
		duration_ms: 1322000,
		tool_call_count: 212,
		tool_error_count: 5,
		files_touched: [],
		tools_used: [],
		skills_used: [],
		agents_dispatched: [],
		stop_response: 'Baseline committed.',
		stop_reason: 'end_turn',
		interrupted: false,
		...extra
	};
}

describe('itemsQueryString', () => {
	it('encodes filters in a stable order and joins include parts', () => {
		expect(
			itemsQueryString({
				disagreement: 'reaction',
				include: ['review', 'verdict'],
				limit: 500,
				gold_reaction: 'corrected'
			})
		).toBe('?disagreement=reaction&gold_reaction=corrected&include=review%2Cverdict&limit=500');
	});

	it('keeps false booleans and drops empty values', () => {
		expect(itemsQueryString({ reaction_correct: false, severity: '', include: [] })).toBe(
			'?reaction_correct=false'
		);
		expect(itemsQueryString({})).toBe('');
	});
});

describe('filters and URL state', () => {
	it('round-trips every filter and the selected turn', () => {
		const filters = { mode: 'delivery' as const, gold: 'partial', judge: 'delivered', agree: true };
		const params = paramsFor(filters, 10487);
		expect(params.get('turn')).toBe('10487');
		expect(filtersFromParams(params)).toEqual(filters);
		expect(activeFilterCount(filters)).toBe(3);
	});

	it('falls back to the reaction tab for retired and unknown modes', () => {
		expect(filtersFromParams(new URLSearchParams('mode=any'))).toEqual(DEFAULT_FILTERS);
		expect(filtersFromParams(new URLSearchParams('mode=all&agree=yes'))).toEqual(DEFAULT_FILTERS);
		expect(filtersFromParams(new URLSearchParams('gold=delivery:partial')).gold).toBe('partial');
		expect(paramsFor(DEFAULT_FILTERS, null).toString()).toBe('');
	});

	it('asks the server for this tab only, on this tab’s labels', () => {
		expect(
			queueQuery({ mode: 'delivery', gold: 'partial', judge: 'delivered', agree: false })
		).toEqual({
			include: ['review'],
			limit: 500,
			disagreement: 'delivery',
			gold_delivery: 'partial',
			verdict_delivery: 'delivered'
		});
		expect(queueQuery({ ...DEFAULT_FILTERS, agree: true }).disagreement).toBeUndefined();
	});
});

describe('the fixed list', () => {
	const items = byTurn([
		item(30),
		item(10, { reviewed_at: '2026-09-28T10:00:00Z' }),
		item(20),
		item(40, { reviewed_at: 'x' })
	]);

	it('orders by turn id, never by state', () => {
		expect(items.map((i) => i.operator_turn_id)).toEqual([10, 20, 30, 40]);
	});

	it('walks the list in order whatever each turn’s state', () => {
		expect(neighbour(items, 20, -1)).toBe(10);
		expect(neighbour(items, 30, 1)).toBe(40);
		expect(neighbour(items, 40, 1)).toBeNull();
		expect(neighbour(items, 99, 1)).toBe(10);
	});

	it('opens the first undecided turn, and after a save the next undecided one, wrapping', () => {
		expect(firstUndecided(items)).toBe(20);
		expect(nextUndecided(items, 20)).toBe(30);
		expect(nextUndecided(items, 30)).toBe(20);
		expect(nextUndecided([item(5)], 5)).toBeNull();
	});
});

describe('shortcuts', () => {
	it('navigates on j and k only when unmodified', () => {
		expect(shortcutFor(key('j'))).toEqual({ kind: 'next' });
		expect(shortcutFor(key('k'))).toEqual({ kind: 'previous' });
		expect(shortcutFor(key('j', { ctrlKey: true }))).toBeNull();
		expect(shortcutFor(key('k', { shiftKey: true }))).toBeNull();
	});

	it('never writes an answer on a bare key', () => {
		for (const k of ['1', '2', '3', 'g', 'r', 'd', 'Enter']) expect(shortcutFor(key(k))).toBeNull();
		expect(shortcutFor(key('Enter', { ctrlKey: true }))).toBeNull();
		expect(shortcutFor(key('Enter', { metaKey: true }))).toBeNull();
		expect(shortcutFor(key('g', { altKey: true }))).toBeNull();
		expect(shortcutFor(key('j', { altKey: true }))).toBeNull();
	});

	it('chooses with Alt+1..6 in on-screen order, matched on the physical key', () => {
		expect(shortcutFor({ ...key('1', { altKey: true }), key: '¡' })).toEqual({
			kind: 'choose',
			index: 0
		});
		expect(shortcutFor(key('6', { altKey: true }))).toEqual({ kind: 'choose', index: 5 });
		expect(shortcutFor(key('7', { altKey: true }))).toBeNull();
		expect(shortcutFor(key('1', { altKey: true, shiftKey: true }))).toBeNull();
		expect(shortcutFor(key('1', { altKey: true, ctrlKey: true }))).toBeNull();
	});

	it('treats text fields as typing targets and radios as not', () => {
		const el = (tagName: string, type = '') =>
			({ tagName, type, isContentEditable: false }) as unknown as EventTarget;
		expect(isTextTarget(el('TEXTAREA'))).toBe(true);
		expect(isTextTarget(el('INPUT', 'text'))).toBe(true);
		expect(isTextTarget(el('INPUT', 'radio'))).toBe(false);
		expect(isTextTarget(el('DIV'))).toBe(false);
		expect(
			isTextTarget({ tagName: 'DIV', isContentEditable: true } as unknown as EventTarget)
		).toBe(true);
		expect(isTextTarget(null)).toBe(false);
	});
});

describe('questions and candidates', () => {
	const t10487 = item(10487, {
		gold_reaction: 'accepted',
		gold_delivery: 'partial',
		verdict_reaction: 'accepted',
		verdict_delivery: 'delivered',
		reaction_correct: true,
		delivery_correct: false
	});
	const both = item(12180, {
		gold_delivery: 'partial',
		verdict_delivery: 'delivered',
		delivery_correct: false
	});

	it('asks only the disputed questions, the active tab’s first', () => {
		expect(disputedAxes(t10487, 'delivery')).toEqual(['delivery']);
		expect(disputedAxes(both, 'delivery')).toEqual(['delivery', 'reaction']);
		expect(disputedAxes(both, 'reaction')).toEqual(['reaction', 'delivery']);
		expect(disputedAxes(item(1, { verdict_reaction: 'corrected' }), 'delivery')).toEqual([
			'delivery'
		]);
		expect(disputedAxes(item(2, { outcome: 'skipped_unresolved' }), 'reaction')).toEqual([
			'reaction'
		]);
	});

	it('names each answer by who gave it: the label now, the earlier label, the judge', () => {
		const same = [label(10487, 'delivery', 'partial')];
		expect(candidates(t10487, same, 'delivery')).toEqual([
			{ label: 'partial', holders: ['label'], tag: 'the label' },
			{ label: 'delivered', holders: ['judge'], tag: 'the judge' }
		]);
		expect(
			candidates(t10487, [label(10487, 'delivery', 'none')], 'delivery').map((c) => c.tag)
		).toEqual(['the label', 'the earlier label', 'the judge']);
		expect(candidates(t10487, [label(10487, 'delivery', 'delivered')], 'delivery')).toEqual([
			{ label: 'delivered', holders: ['label', 'judge'], tag: 'the label and the judge' },
			{ label: 'partial', holders: ['earlier'], tag: 'the earlier label' }
		]);
		expect(
			candidates(
				item(3, { outcome: 'skipped_unresolved', gold_delivery: 'partial' }),
				[],
				'delivery'
			)
		).toEqual([{ label: 'partial', holders: ['label'], tag: 'the label' }]);
		expect(cardHeading(['label'])).toBe("This turn's label");
		expect(cardHeading(['judge'])).toBe("The judge's answer");
		expect(cardHeading(['label', 'judge'])).toBe("This turn's label and the judge's answer");
	});

	it('notes an undisputed question only when its label changed since the accuracy run', () => {
		expect(otherAxisNote(t10487, [label(10487, 'reaction', 'redirected')], 'reaction')).toBe(
			'How you reacted is not in dispute: the label and the judge both said Accepted when the judge was tested. The label has since been changed to Redirected.'
		);
		expect(otherAxisNote(t10487, [label(10487, 'reaction', 'accepted')], 'reaction')).toBeNull();
		expect(otherAxisNote(t10487, [label(10487, 'delivery', 'none')], 'delivery')).toBeNull();
	});

	it('reads the latest saved answers per question', () => {
		expect(latestReview([])).toBeNull();
		const saved = latestReview(reviewLabels(13778, 'redirected', 'note text'));
		expect(saved).toEqual({
			answers: { reaction: 'redirected', delivery: 'delivered' },
			at: '2026-09-28T09:00:00Z',
			note: 'note text'
		});
		const older = {
			...label(1, 'reaction', 'accepted'),
			created_at: '2026-09-27T09:00:00Z'
		} as EvalLabelDto;
		const newer = {
			...label(1, 'delivery', 'partial'),
			created_at: '2026-09-28T09:00:00Z'
		} as EvalLabelDto;
		expect(latestReview([newer, older])?.answers).toEqual({ delivery: 'partial' });
	});
});

describe('turn facts', () => {
	const ctx = context(10487, { next_genuine_at: '2026-07-27T08:24:40Z' });

	it('compares when you replied with when the turn finished', () => {
		expect(stillRunning(facts(), ctx)).toBe(true);
		expect(stillRunning(facts({ duration_ms: 60000 }), ctx)).toBe(false);
		expect(stillRunning(facts({ created_at: null }), ctx)).toBeNull();
		expect(stillRunning(facts(), { ...ctx, next_genuine_at: null })).toBeNull();
		expect(stillRunning(null, ctx)).toBeNull();
	});

	it('names only unusual endings, in plain words', () => {
		expect(stopWords(facts())).toBeNull();
		expect(stopWords(facts({ interrupted: true }))).toBe('you interrupted it');
		expect(stopWords(facts({ stop_reason: 'max_tokens' }))).toBe('cut off at its output limit');
		expect(stopWords(facts({ stop_reason: 'refusal' }))).toBe('refused');
		expect(stopWords(facts({ stop_reason: 'tool_use' }))).toBeNull();
		expect(stopWords(facts({ stop_reason: null }))).toBeNull();
	});

	it('formats durations and local times', () => {
		expect(formatDuration(1322000)).toBe('22 min');
		expect(formatDuration(45000)).toBe('45 s');
		expect(formatDuration(3900000)).toBe('1 h 5 min');
		expect(formatDuration(null)).toBeNull();
		expect(formatWhen(new Date(2026, 6, 27, 10, 3).toISOString())).toBe('27 Jul 2026 10:03');
		expect(formatWhen(null)).toBeNull();
	});
});

describe('the labellers’ quote', () => {
	it('marks the first exact occurrence, searching your reply, then the last message, then your prompt', () => {
		const texts = [
			{ key: 'reply', text: 'no quote here' },
			{ key: 'last', text: 'it says "make tea" twice: "make tea"' },
			{ key: 'prompt', text: '"make tea"' }
		];
		expect(citedSpan('  "make tea" ', texts)).toEqual({ key: 'last', span: '"make tea"' });
		expect(citedSpan('absent', texts)).toBeNull();
		expect(citedSpan('   ', texts)).toBeNull();
		expect(citedSpan(null, texts)).toBeNull();
		expect(markSegments('it says "make tea" twice: "make tea"', '"make tea"')).toEqual({
			before: 'it says ',
			match: '"make tea"',
			after: ' twice: "make tea"'
		});
		expect(markSegments('abc', 'x')).toBeNull();
	});

	it('uses one quote, the question’s own when the two differ', () => {
		const gold = [
			label(1, 'reaction', 'accepted', { evidence: 'reaction quote' }),
			label(1, 'delivery', 'partial', { evidence: ' delivery quote ' })
		];
		expect(quotedEvidence(gold, 'delivery')).toBe('delivery quote');
		expect(
			quotedEvidence(
				[
					label(1, 'reaction', 'accepted', { evidence: 'only' }),
					label(1, 'delivery', 'partial', { evidence: null })
				],
				'delivery'
			)
		).toBe('only');
		expect(quotedEvidence([], 'delivery')).toBeNull();
	});
});

const JC11 = {
	labellers: {
		A: 'sonnet effort medium, turn-first',
		B: 'opus, reaction-first',
		adjudicator: 'opus'
	},
	A: {
		label: 'CORRECTED',
		delivery: 'partial',
		failure: 'Turn ended mid-task without completing that verification or reporting completion'
	},
	B: {
		label: 'CORRECTED',
		delivery: 'partial',
		failure: 'Ended the turn mid-implementation without confirming the copy was removed.'
	},
	final_reason: null
};
const NOW = new Date(2026, 8, 28, 12, 0);

describe('who labelled the turn, and why', () => {
	const row = label(10695, 'reaction', 'corrected', {
		id: 5333,
		tags: ['stopped-short'],
		evidence: 'ensure it is done and working properly',
		provenance: JC11,
		created_at: new Date(2026, 8, 28, 9, 46).toISOString()
	});
	const earlier = label(10695, 'reaction', 'corrected', {
		id: 2946,
		method: 'single_model',
		tags: ['stopped-short', 'asked-instead-of-acting'],
		provenance: { failure: 'The assistant never answered whether the fourth copy was gone.' },
		created_at: new Date(2026, 8, 25, 10, 14).toISOString()
	});
	const facts = [
		label(10695, 'fact_own_prompt_corrects_prior', 'true', { id: 5335 }),
		label(10695, 'fact_irreversible_action', 'false', { id: 5337 })
	];
	const tag = label(10695, 'failure_tag', 'handed-back-actions-or-decisions', { id: 4633 });
	const vocabulary = [
		...VOCABULARY,
		{
			axis: 'fact_own_prompt_corrects_prior',
			subject_kinds: ['turn'],
			labels: [
				{
					label: 'true',
					sort_order: 1,
					description: "This turn's own prompt corrects an earlier turn."
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
					description: 'Hands back something the assistant could do itself.'
				}
			]
		}
	];

	it('says who labelled it in one sentence, naming the models and whether they agreed', () => {
		expect(labellingWho(row, 'reaction', NOW)).toBe(
			'Two AI models (Sonnet and Opus), working separately, on 28 Sep. Both said Corrected.'
		);
		const split = {
			...row,
			provenance: { ...JC11, B: { ...JC11.B, label: 'ACCEPTED' } }
		};
		expect(labellingWho(split, 'reaction', NOW)).toBe(
			'Two AI models (Sonnet and Opus), working separately, on 28 Sep. They differed (Corrected and Accepted); a third model (Opus) settled it as Corrected.'
		);
		expect(labellingWho(earlier, 'reaction', NOW)).toBe('One AI model, on 25 Sep.');
		expect(labellingWho({ ...earlier, method: 'operator_click' }, 'reaction', NOW)).toBe(
			'You, on 25 Sep.'
		);
		expect(
			labellingWho({ ...earlier, created_at: new Date(2025, 0, 2).toISOString() }, 'reaction', NOW)
		).toBe('One AI model, on 2 Jan 2025.');
	});

	it('gives the reasons, the other labellings and the noted facts in plain words, once', () => {
		const why = labellingWhy({
			labels: [row, earlier, ...facts, tag],
			row,
			axis: 'reaction',
			vocabulary,
			quoteShown: true,
			now: NOW
		});
		expect(why).toEqual([
			{ kind: 'said', who: 'Sonnet', text: JC11.A.failure },
			{ kind: 'said', who: 'Opus', text: JC11.B.failure },
			{
				kind: 'text',
				text: 'On 25 Sep, one AI model also said Corrected: The assistant never answered whether the fourth copy was gone.'
			},
			{
				kind: 'noted',
				items: [
					"This turn's own prompt corrects an earlier turn.",
					'Stopped short.',
					'Hands back something the assistant could do itself.',
					'Asked instead of acting.'
				]
			},
			{ kind: 'rules', items: LABELLING_RULES.reaction, open: false }
		]);
		expect(JSON.stringify(why)).not.toMatch(/jc11|seed|holdout|\d\d:\d\d/);
	});

	it('always offers the mixed-case rules, opened when the settling reason cites them', () => {
		const settled = (reason: string) =>
			labellingWhy({
				labels: [],
				row: { ...row, provenance: { final_reason: reason } },
				axis: 'reaction',
				vocabulary,
				quoteShown: true,
				now: NOW
			}).find((w) => w.kind === 'rules');
		expect(
			settled('label: sided with A; the partial-accept-plus-correction tie-break applies')
		).toEqual({
			kind: 'rules',
			items: LABELLING_RULES.reaction,
			open: true
		});
		expect(settled('label: sided with A; the reply corrects point 2')).toMatchObject({
			open: false
		});
	});

	it('quotes the labellers only when the quote is not already marked on the page', () => {
		const why = labellingWhy({
			labels: [row],
			row,
			axis: 'delivery',
			vocabulary,
			quoteShown: false,
			now: NOW
		});
		expect(why).toContainEqual({
			kind: 'text',
			text: 'They quoted: "ensure it is done and working properly"'
		});
	});
});

describe('what the judge saw', () => {
	const stop =
		'It was still in place when you asked. ' + 'x'.repeat(120) + ' ordinary honest prose.';

	it('says whether the judge saw the whole last message, only its end, or none of it', () => {
		expect(judgeSawLastMessage(stop, `before ${stop} after`)).toBe(
			"The judge saw the assistant's whole last message."
		);
		expect(judgeSawLastMessage(stop, `ire${'x'.repeat(100)} ordinary honest prose.`)).toBe(
			"The judge saw only the end of the assistant's last message to you."
		);
		expect(judgeSawLastMessage(stop, 'something else')).toBe(
			"The judge did not see the assistant's last message to you."
		);
		expect(judgeSawLastMessage(null, 'anything')).toBeNull();
	});

	it('splits the judge input into its four sections, whole, or not at all', () => {
		const input = [
			'OPERATOR REQUEST (user prompt):',
			'please tell me the fourth copy is not still in place.',
			'',
			'ASSISTANT TURN TEXT (whole turn, in order; cut at the front when long):',
			'ire on ordinary honest prose.',
			'',
			'TOOL USE SUMMARY:',
			'tool_call_count = 107  tool_error_count = 0',
			'',
			"OPERATOR'S NEXT GENUINE MESSAGE (automated events removed):",
			'ensure it is done and working properly',
			''
		].join('\n');
		expect(judgeInputSections(input)).toEqual([
			{ title: 'Your prompt', text: 'please tell me the fourth copy is not still in place.' },
			{
				title: 'What the assistant wrote, as the judge got it',
				text: 'ire on ordinary honest prose.'
			},
			{ title: 'Tool calls', text: 'tool_call_count = 107  tool_error_count = 0' },
			{ title: 'Your reply', text: 'ensure it is done and working properly' }
		]);
		expect(judgeInputSections('OPERATOR REQUEST (user prompt):\nonly one')).toBeNull();
		expect(judgeInputSections(null)).toBeNull();
	});
});

describe('plain words', () => {
	it('keeps the session page’s one-sentence method wording', () => {
		expect(methodSentence('single_model')).toBe('One model labeller labelled this turn.');
		expect(notGradedReason('skipped_unresolved')).toBe('your next message was never worked out');
		expect(notGradedReason('failed_model_error')).toBe('failed model error');
	});

	it('places a turn in its session and formats agreement as whole percent', () => {
		const turns = [{ turn_id: 10454 }, { turn_id: 10692 }, { turn_id: 10695 }];
		expect(turnPosition(turns, 10695)).toEqual({ index: 3, total: 3 });
		expect(turnPosition(turns, 1)).toBeNull();
		expect(wholePercent(160, 200)).toBe('80%');
		expect(wholePercent(0, 0)).toBe('—');
		expect(accuracy({ n: 277, agreement: 239, kappa: 0.8 })).toBeCloseTo(0.8628, 4);
		expect(disagreementCount({ n: 277, agreement: 239 })).toBe(38);
		expect(disagreementCount(undefined)).toBeNull();
		expect(formatDay(new Date(2026, 8, 28).toISOString(), NOW)).toBe('28 Sep');
	});
});

describe('the rules behind a label', () => {
	it('names the model behind each side the third model sided with', () => {
		const names = { A: 'Sonnet', B: 'Opus', C: 'Opus' };
		expect(nameSides('label: sided with A. The reply corrects point 2.', names)).toBe(
			'label: sided with A (Sonnet). The reply corrects point 2.'
		);
		expect(nameSides('Label: I sided with B (CORRECTED).', names)).toBe(
			'Label: I sided with B (Opus) (CORRECTED).'
		);
		expect(nameSides('sided with B', { A: null, B: null, C: null })).toBe(
			'sided with B (the second labeller)'
		);
	});

	it('states the mixed-case rules for both questions in plain words', () => {
		expect(LABELLING_RULES.reaction[0]).toMatch(
			/^A reply that accepts part of the turn and corrects another part is Corrected/
		);
		expect(LABELLING_RULES.delivery.some((r) => r.startsWith('Partial:'))).toBe(true);
	});

	it('flags a judge answer made without the last message', () => {
		expect(judgeMissedLast(judgeSawLastMessage('the final answer here', 'something else'))).toBe(
			true
		);
		expect(judgeMissedLast(judgeSawLastMessage('the final answer', 'x the final answer y'))).toBe(
			false
		);
	});
});
