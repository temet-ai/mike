import { ApiError } from '$lib/api';
import { LabelWriteError } from '$lib/api/eval';
import type { EvalLabelDto, VocabularyAxis } from '$lib/api/eval';
import type {
	EvalReviewContextDto,
	EvalRunItemDto,
	OperatorTurnFactsDto,
	RunItemsQuery
} from '$lib/api/evalRuns';
import { optionsFor, wordFor } from './labelControl';

export type Axis = 'reaction' | 'delivery';
export type ReviewMode = Axis;

export const AXES: readonly Axis[] = ['reaction', 'delivery'];

export interface ReviewFilters {
	mode: ReviewMode;
	gold: string;
	judge: string;
	agree: boolean;
	uncertain?: boolean;
}

export const DEFAULT_FILTERS: ReviewFilters = {
	mode: 'reaction',
	gold: '',
	judge: '',
	agree: false
};

export const MODE_TABS: Array<{ value: ReviewMode; label: string; help: string }> = [
	{
		value: 'delivery',
		label: 'What was delivered',
		help: "Turns where the judge's answer and the label differ on what the assistant delivered."
	},
	{
		value: 'reaction',
		label: 'How you reacted',
		help: "Turns where the judge's answer and the label differ on how you reacted in your next message."
	}
];

export const QUEUE_LIMIT = 500;

export const QUESTION: Record<Axis, string> = {
	delivery: 'What did the assistant deliver?',
	reaction: 'How did you react in your next message?'
};

export const ROW_LABEL: Record<Axis, string> = {
	delivery: 'What was delivered',
	reaction: 'How you reacted'
};

function isAxis(value: string | null | undefined): value is Axis {
	return value === 'reaction' || value === 'delivery';
}

function plainLabel(value: string | null): string {
	if (!value) return '';
	const colon = value.indexOf(':');
	return colon === -1 ? value : value.substring(colon + 1);
}

export function filtersFromParams(params: URLSearchParams): ReviewFilters {
	const mode = params.get('mode');
	return {
		mode: isAxis(mode) ? mode : DEFAULT_FILTERS.mode,
		gold: plainLabel(params.get('gold')),
		judge: plainLabel(params.get('judge')),
		agree: params.get('agree') === '1',
		...uncertainFromParam(params.get('uncertain'))
	};
}

function uncertainFromParam(value: string | null): { uncertain?: boolean } {
	if (value === '1') return { uncertain: true };
	if (value === '0') return { uncertain: false };
	return {};
}

export function paramsFor(filters: ReviewFilters, turn: number | null): URLSearchParams {
	const params = new URLSearchParams();
	if (filters.mode !== DEFAULT_FILTERS.mode) params.set('mode', filters.mode);
	if (filters.gold) params.set('gold', filters.gold);
	if (filters.judge) params.set('judge', filters.judge);
	if (filters.agree) params.set('agree', '1');
	if (filters.uncertain !== undefined) params.set('uncertain', filters.uncertain ? '1' : '0');
	if (turn !== null) params.set('turn', String(turn));
	return params;
}

export function activeFilterCount(filters: ReviewFilters): number {
	return [filters.gold !== '', filters.judge !== '', filters.agree].filter(Boolean).length;
}

export function queueQuery(filters: ReviewFilters): RunItemsQuery {
	const query: RunItemsQuery = { include: ['review'], limit: QUEUE_LIMIT };
	if (filters.uncertain === true) query.review = 'uncertain';
	if (!filters.agree) query.disagreement = filters.mode;
	if (filters.gold)
		query[filters.mode === 'reaction' ? 'gold_reaction' : 'gold_delivery'] = filters.gold;
	if (filters.judge)
		query[filters.mode === 'reaction' ? 'verdict_reaction' : 'verdict_delivery'] = filters.judge;
	return query;
}

export function byTurn(items: EvalRunItemDto[]): EvalRunItemDto[] {
	return [...items].sort((a, b) => a.operator_turn_id - b.operator_turn_id);
}

export function neighbour(
	items: EvalRunItemDto[],
	turn: number | null,
	step: 1 | -1
): number | null {
	if (items.length === 0) return null;
	const index = items.findIndex((i) => i.operator_turn_id === turn);
	if (index === -1) return items[0]?.operator_turn_id ?? null;
	return items[index + step]?.operator_turn_id ?? null;
}

export function firstUndecided(items: EvalRunItemDto[]): number | null {
	return items.find((i) => !i.reviewed_at)?.operator_turn_id ?? null;
}

export function nextUndecided(items: EvalRunItemDto[], turn: number): number | null {
	const start = items.findIndex((i) => i.operator_turn_id === turn) + 1;
	for (let step = 0; step < items.length; step++) {
		const candidate = items[(start + step) % items.length];
		if (candidate && !candidate.reviewed_at && candidate.operator_turn_id !== turn)
			return candidate.operator_turn_id;
	}
	return null;
}

export type ShortcutAction =
	{ kind: 'choose'; index: number } | { kind: 'next' } | { kind: 'previous' };

export interface KeyLike {
	key: string;
	code?: string;
	ctrlKey: boolean;
	metaKey: boolean;
	altKey: boolean;
	shiftKey: boolean;
}

export function isTextTarget(target: EventTarget | null): boolean {
	if (!target || typeof (target as HTMLElement).tagName !== 'string') return false;
	const el = target as HTMLElement;
	if (el.isContentEditable) return true;
	const tag = el.tagName.toLowerCase();
	if (tag === 'textarea' || tag === 'select') return true;
	if (tag !== 'input') return false;
	const type = (el as HTMLInputElement).type;
	return !['radio', 'checkbox', 'button', 'submit'].includes(type);
}

const ALT_DIGIT = /^Digit([1-6])$/;

export function shortcutFor(event: KeyLike): ShortcutAction | null {
	const modified = event.ctrlKey || event.metaKey;
	if (event.altKey) {
		if (modified || event.shiftKey) return null;
		const digit = ALT_DIGIT.exec(event.code ?? '')?.[1];
		return digit ? { kind: 'choose', index: Number(digit) - 1 } : null;
	}
	if (modified || event.shiftKey) return null;
	if (event.key === 'j') return { kind: 'next' };
	if (event.key === 'k') return { kind: 'previous' };
	return null;
}

export function goldOn(item: EvalRunItemDto, axis: Axis): string | null {
	return axis === 'reaction' ? item.gold_reaction : item.gold_delivery;
}

export function judgeOn(item: EvalRunItemDto, axis: Axis): string | null {
	if (item.outcome !== 'judged') return null;
	return axis === 'reaction' ? item.verdict_reaction : item.verdict_delivery;
}

export function isDisputed(item: EvalRunItemDto, axis: Axis): boolean {
	const judge = judgeOn(item, axis);
	return judge !== null && judge !== goldOn(item, axis);
}

export function disputedAxes(item: EvalRunItemDto, mode: ReviewMode): Axis[] {
	const ordered: Axis[] = [mode, mode === 'reaction' ? 'delivery' : 'reaction'];
	const disputed = ordered.filter((axis) => isDisputed(item, axis));
	return disputed.length > 0 ? disputed : [mode];
}

export function labelOn(labels: EvalLabelDto[], axis: string): EvalLabelDto | null {
	return labels.find((l) => l.axis === axis) ?? null;
}

export type Holder = 'label' | 'earlier' | 'judge';

export interface Candidate {
	label: string;
	holders: Holder[];
	tag: string;
}

const HOLDER_TAG: Record<Holder, string> = {
	label: 'the label',
	earlier: 'the earlier label',
	judge: 'the judge'
};

export function answerTag(holders: Holder[]): string {
	return holders.map((h) => HOLDER_TAG[h]).join(' and ');
}

const HOLDER_HEADING: Record<Holder, string> = {
	label: "This turn's label",
	earlier: "This turn's earlier label",
	judge: "The judge's answer"
};

export function cardHeading(holders: Holder[]): string {
	const [first, ...rest] = holders.map((h) => HOLDER_HEADING[h]);
	return [first ?? '', ...rest.map((h) => h.charAt(0).toLowerCase() + h.substring(1))].join(
		' and '
	);
}

export function candidates(item: EvalRunItemDto, current: EvalLabelDto[], axis: Axis): Candidate[] {
	const run = goldOn(item, axis);
	const now = labelOn(current, axis)?.label ?? null;
	const judge = judgeOn(item, axis);
	const order: Array<{ holder: Holder; label: string | null }> = [
		{ holder: 'label', label: now ?? run },
		{ holder: 'earlier', label: now !== null && run !== null && now !== run ? run : null },
		{ holder: 'judge', label: judge }
	];
	const found: Array<{ label: string; holders: Holder[] }> = [];
	for (const { holder, label } of order) {
		if (label === null) continue;
		const same = found.find((c) => c.label === label);
		if (same) same.holders.push(holder);
		else found.push({ label, holders: [holder] });
	}
	return found.map((c) => ({ ...c, tag: answerTag(c.holders) }));
}

export function capitalise(text: string): string {
	return text.charAt(0).toUpperCase() + text.substring(1);
}

export interface SavedReview {
	answers: Partial<Record<Axis, string>>;
	at: string;
	note: string | null;
}

export function latestReview(reviews: EvalLabelDto[]): SavedReview | null {
	const latest = reviews[0];
	if (!latest) return null;
	const batch = reviews.filter((r) => r.created_at === latest.created_at);
	const answers: Partial<Record<Axis, string>> = {};
	for (const axis of AXES) {
		const found = labelOn(batch, axis);
		if (found) answers[axis] = found.label;
	}
	return { answers, at: latest.created_at, note: batch.find((r) => r.note)?.note ?? null };
}

export function stillRunning(
	facts: OperatorTurnFactsDto | null,
	context: EvalReviewContextDto | null
): boolean | null {
	if (
		!facts?.created_at ||
		facts.duration_ms === null ||
		facts.duration_ms === undefined ||
		!context?.next_genuine_at
	) {
		return null;
	}
	return Date.parse(facts.created_at) + facts.duration_ms > Date.parse(context.next_genuine_at);
}

const STOP_REASON_WORDS: Record<string, string> = {
	max_tokens: 'cut off at its output limit',
	refusal: 'refused'
};

export function stopWords(facts: OperatorTurnFactsDto | null): string | null {
	if (!facts) return null;
	if (facts.interrupted === true) return 'you interrupted it';
	return facts.stop_reason ? (STOP_REASON_WORDS[facts.stop_reason] ?? null) : null;
}

export interface CitedSpan {
	key: string;
	span: string;
}

export function citedSpan(
	evidence: string | null,
	texts: Array<{ key: string; text: string | null }>
): CitedSpan | null {
	const span = evidence?.trim() ?? '';
	if (!span) return null;
	for (const { key, text } of texts) {
		if (text && text.includes(span)) return { key, span };
	}
	return null;
}

export function markSegments(
	text: string,
	span: string
): { before: string; match: string; after: string } | null {
	if (!span) return null;
	const parts = text.split(span);
	if (parts.length < 2) return null;
	return { before: parts[0] ?? '', match: span, after: parts.slice(1).join(span) };
}

export function quotedEvidence(labels: EvalLabelDto[], axis: Axis): string | null {
	const own = labelOn(labels, axis)?.evidence?.trim();
	if (own) return own;
	const other = labels.find((l) => l.evidence?.trim())?.evidence?.trim();
	return other || null;
}

export const METHOD_SENTENCE: Record<string, string> = {
	double_blind_adjudicated:
		"Two model labellers labelled this turn without seeing each other's answer; a third model settled where they differed.",
	single_model: 'One model labeller labelled this turn.',
	operator_click: 'You chose it in mike.',
	operator_confirmed_relay: 'You confirmed it after it was relayed from a session.',
	relayed_quote: 'Relayed from something you said in a session.',
	next_message_auto: 'Worked out automatically from your next message.'
};

export function methodSentence(method: string): string {
	return METHOD_SENTENCE[method] ?? `Recorded by ${method.replaceAll('_', ' ')}.`;
}

export function notGradedReason(outcome: string): string {
	return outcome === 'skipped_unresolved'
		? 'your next message was never worked out'
		: outcome.replaceAll('_', ' ');
}

export function otherAxisNote(
	item: EvalRunItemDto,
	current: EvalLabelDto[],
	axis: Axis
): string | null {
	if (item.outcome !== 'judged' || isDisputed(item, axis)) return null;
	const run = goldOn(item, axis);
	const now = labelOn(current, axis)?.label ?? null;
	if (!run || !now || now === run) return null;
	return `${ROW_LABEL[axis]} is not in dispute: the label and the judge both said ${wordFor(run)} when the judge was tested. The label has since been changed to ${wordFor(now)}.`;
}

const MONTHS = ['Jan', 'Feb', 'Mar', 'Apr', 'May', 'Jun', 'Jul', 'Aug', 'Sep', 'Oct', 'Nov', 'Dec'];

function pad(n: number): string {
	return String(n).padStart(2, '0');
}

function parse(iso: string | null | undefined): Date | null {
	if (!iso) return null;
	const d = new Date(iso);
	return Number.isNaN(d.getTime()) ? null : d;
}

export function formatClock(iso: string | null | undefined): string | null {
	const d = parse(iso);
	return d ? `${pad(d.getHours())}:${pad(d.getMinutes())}` : null;
}

export function formatDate(iso: string | null | undefined): string | null {
	const d = parse(iso);
	return d ? `${d.getDate()} ${MONTHS[d.getMonth()]} ${d.getFullYear()}` : null;
}

export function formatWhen(iso: string | null | undefined): string | null {
	const d = parse(iso);
	return d ? `${formatDate(iso)} ${pad(d.getHours())}:${pad(d.getMinutes())}` : null;
}

export function formatDay(iso: string | null | undefined, now: Date = new Date()): string | null {
	const d = parse(iso);
	if (!d) return null;
	const day = `${d.getDate()} ${MONTHS[d.getMonth()]}`;
	return d.getFullYear() === now.getFullYear() ? day : `${day} ${d.getFullYear()}`;
}

export function formatDuration(ms: number | null | undefined): string | null {
	if (ms === null || ms === undefined) return null;
	if (ms < 60000) return `${Math.round(ms / 1000)} s`;
	const minutes = Math.round(ms / 60000);
	if (minutes < 60) return `${minutes} min`;
	return `${Math.floor(minutes / 60)} h ${minutes % 60} min`;
}

export function joinWords(words: string[]): string {
	return words.join(' and ');
}

export function disagreementCount(
	axis: { n: number; agreement: number } | undefined
): number | null {
	return axis ? axis.n - axis.agreement : null;
}

export function wholePercent(k: number, n: number): string {
	return n > 0 ? `${Math.round((k / n) * 100)}%` : '—';
}

export type FactsState =
	| { status: 'loading' }
	| { status: 'ok'; facts: OperatorTurnFactsDto }
	| { status: 'error'; message: string };

export function loadErrorWords(e: unknown): string {
	if (e instanceof ApiError && e.status >= 500) return `server error ${e.status}`;
	if (e instanceof ApiError && e.status === 404) return 'not found';
	return e instanceof Error ? e.message : String(e);
}

export function saveErrorText(e: unknown): string {
	if (e instanceof LabelWriteError && e.status === 403)
		return 'Not saved: labels can only be saved from the mike UI.';
	const words = e instanceof Error ? e.message : String(e);
	return `Not saved: ${words.replace(/\.$/, '')}. Pick again to retry.`;
}

export interface BarStatus {
	tone: 'muted' | 'success' | 'danger';
	text: string;
	goBack?: number;
}

type Provenance = Record<string, unknown>;

function record(value: unknown): Provenance | null {
	return value && typeof value === 'object' && !Array.isArray(value) ? (value as Provenance) : null;
}

function text(value: unknown): string | null {
	return typeof value === 'string' && value.trim() ? value.trim() : null;
}

function modelName(description: unknown): string | null {
	const first = text(description)?.split(/[\s,]+/)[0];
	return first ? capitalise(first) : null;
}

export interface LabellerNames {
	A: string | null;
	B: string | null;
	C: string | null;
}

export function labellerNames(provenance: Provenance): LabellerNames {
	const labellers = record(provenance['labellers']);
	return {
		A: modelName(labellers?.['A']),
		B: modelName(labellers?.['B']),
		C: modelName(labellers?.['adjudicator'])
	};
}

function sideAnswer(provenance: Provenance, side: 'A' | 'B', axis: string): string | null {
	const found = record(provenance[side]);
	if (!found) return null;
	const raw = text(axis === 'reaction' ? found['label'] : found[axis]);
	return raw ? raw.toLowerCase() : null;
}

export function labellingWho(row: EvalLabelDto, axis: string, now: Date = new Date()): string {
	const date = formatDay(row.created_at, now);
	const on = date ? `, on ${date}` : '';
	const word = wordFor(row.label);
	switch (row.method) {
		case 'double_blind_adjudicated': {
			const names = labellerNames(row.provenance);
			const pair = names.A && names.B ? ` (${names.A} and ${names.B})` : '';
			const head = `Two AI models${pair}, working separately${on}.`;
			const a = sideAnswer(row.provenance, 'A', axis);
			const b = sideAnswer(row.provenance, 'B', axis);
			if (!a || !b) return head;
			if (a === b) return `${head} Both said ${word}.`;
			const third = names.C ? ` (${names.C})` : '';
			return `${head} They differed (${wordFor(a)} and ${wordFor(b)}); a third model${third} settled it as ${word}.`;
		}
		case 'single_model':
			return `One AI model${on}.`;
		case 'operator_click':
			return `You${on}.`;
		case 'operator_confirmed_relay':
			return `You${on}, confirming a label passed on from a session.`;
		case 'relayed_quote':
			return `Something you said in a session${on}.`;
		case 'next_message_auto':
			return `Worked out from your next message${on}.`;
		default:
			return date ? `Not recorded. The label was written on ${date}.` : 'Not recorded.';
	}
}

const WHO_SHORT: Record<string, string> = {
	double_blind_adjudicated: 'two AI models',
	single_model: 'one AI model',
	operator_click: 'you',
	operator_confirmed_relay: 'you',
	relayed_quote: 'something you said in a session',
	next_message_auto: 'your next message'
};

export type WhyItem =
	| { kind: 'said'; who: string; text: string }
	| { kind: 'text'; text: string }
	| { kind: 'noted'; items: string[] }
	| { kind: 'rules'; items: string[]; open: boolean };

// Source: labelling-rubric.v1.md in night-watch-notes (plans/judgement-context/jc11), the rubric the
// reference labels were made with. Keep the two in step.
export const LABELLING_RULES: Record<Axis, string[]> = {
	reaction: [
		'A reply that accepts part of the turn and corrects another part is Corrected. A correction is the thing the labels must not miss.',
		'A reply that asks a question about the turn is Accepted, unless the question is rhetorical or challenging ("why did you...?", "are you sure?", "did you actually test that?"). Those are Corrected.',
		'When you stopped the turn part way and then wrote, the label follows what you wrote. Stopping it to send the assistant another way is usually Corrected.',
		'When the turn ended by asking you a question and you simply answered it, that is Accepted, unless your answer shows the question was not needed ("obviously yes, just do it", "I already told you"). Those are Corrected.'
	],
	delivery: [
		"Judged from how the assistant's last message ends. Its report of what it did is taken at face value unless the text itself shows otherwise.",
		'Partial: some of what was asked is done and the rest is explicitly left, blocked, put off, or still running when the turn ends (waiting on a background job or agent).',
		'Question: the main thing the turn produced is a question or decision put to you, with little of the work done.',
		'Not applicable: the prompt asked for no work, such as a thank-you or a status check.'
	]
};

export function nameSides(text: string, names: LabellerNames): string {
	return text.replace(/\b(sided with )(A|B)\b/gi, (_match, lead: string, side: string) => {
		const key = side.toUpperCase() as 'A' | 'B';
		const name = names[key];
		const fallback = key === 'A' ? 'the first labeller' : 'the second labeller';
		return `${lead}${key} (${name ?? fallback})`;
	});
}

function reasonOf(provenance: Provenance): string | null {
	return (
		text(provenance['final_reason']) ?? text(record(provenance['final_extra'])?.['reason']) ?? null
	);
}

function definitionOf(vocabulary: VocabularyAxis[], axis: string, label: string): string | null {
	return optionsFor(vocabulary, axis).find((o) => o.label === label)?.description ?? null;
}

function tagWords(vocabulary: VocabularyAxis[], tag: string): string | null {
	if (tag === 'none') return null;
	const defined = definitionOf(vocabulary, 'failure_tag', tag);
	if (defined) return defined;
	const plain = capitalise(tag.replaceAll('-', ' ').replaceAll('_', ' ').trim());
	return plain.endsWith('.') ? plain : `${plain}.`;
}

export interface WhyContext {
	labels: EvalLabelDto[];
	row: EvalLabelDto;
	axis: Axis;
	vocabulary: VocabularyAxis[];
	quoteShown: boolean;
	shippedDisputed?: boolean;
	now?: Date;
}

function isLive(l: EvalLabelDto): boolean {
	return l.action === 'assert' && l.is_current !== false;
}

function otherLabellings(ctx: WhyContext, now: Date): WhyItem[] {
	const word = wordFor(ctx.row.label);
	return ctx.labels
		.filter((l) => l.axis === ctx.axis && l.id !== ctx.row.id && l.action === 'assert')
		.sort((a, b) => Date.parse(b.created_at) - Date.parse(a.created_at))
		.map((l) => {
			const date = formatDay(l.created_at, now);
			const who = WHO_SHORT[l.method] ?? 'another labelling';
			const verb = wordFor(l.label) === word ? 'also said' : 'said';
			const why = text(l.provenance['failure']) ?? reasonOf(l.provenance);
			const replaced = l.is_current === false ? ' in an earlier labelling, since replaced' : '';
			const head = `${date ? `On ${date}, ` : ''}${who} ${verb} ${wordFor(l.label)}${replaced}`;
			return { kind: 'text', text: why ? `${capitalise(head)}: ${why}` : `${capitalise(head)}.` };
		});
}

function notedItems(ctx: WhyContext): string[] {
	const out: string[] = [];
	const add = (line: string | null) => {
		if (line && !out.includes(line)) out.push(line);
	};
	for (const l of ctx.labels.filter((x) => x.axis.startsWith('fact_') && isLive(x))) {
		if (l.axis === 'fact_irreversible_action' && ctx.shippedDisputed) {
			add(
				l.label === 'true'
					? definitionOf(ctx.vocabulary, l.axis, 'true')
					: 'The labelling found no commit, push, merge, deploy or release in this turn.'
			);
		} else if (l.label === 'true') {
			add(definitionOf(ctx.vocabulary, l.axis, 'true'));
		}
	}
	for (const tag of ctx.row.tags) add(tagWords(ctx.vocabulary, tag));
	for (const l of ctx.labels.filter((x) => x.axis === 'failure_tag' && isLive(x)))
		add(tagWords(ctx.vocabulary, l.label));
	for (const l of ctx.labels.filter((x) => x.axis === ctx.axis && x.id !== ctx.row.id))
		for (const tag of l.tags) add(tagWords(ctx.vocabulary, tag));
	return out;
}

export function labellingWhy(ctx: WhyContext): WhyItem[] {
	const now = ctx.now ?? new Date();
	const prov = ctx.row.provenance;
	const items: WhyItem[] = [];
	if (ctx.axis === 'reaction') {
		const names = labellerNames(prov);
		for (const [side, fallback] of [
			['A', 'The first model'],
			['B', 'The second model']
		] as const) {
			const failure = text(record(prov[side])?.['failure']);
			if (failure) items.push({ kind: 'said', who: names[side] ?? fallback, text: failure });
		}
	}
	const settled = reasonOf(prov);
	if (settled)
		items.push({
			kind: 'said',
			who: 'The third model, settling their differences',
			text: nameSides(settled, labellerNames(prov))
		});
	const quote = text(ctx.row.evidence);
	if (quote && !ctx.quoteShown) items.push({ kind: 'text', text: `They quoted: "${quote}"` });
	items.push(...otherLabellings(ctx, now));
	const noted = notedItems(ctx);
	if (noted.length > 0) items.push({ kind: 'noted', items: noted });
	items.push({
		kind: 'rules',
		items: LABELLING_RULES[ctx.axis],
		open: settled !== null && /tie-?break|rubric/i.test(settled)
	});
	return items;
}

function squash(value: string): string {
	return value.replace(/\s+/g, ' ').trim();
}

export function judgeSawLastMessage(
	stop: string | null | undefined,
	input: string | null | undefined
): string | null {
	const last = squash(stop ?? '');
	if (!last) return null;
	const seen = squash(input ?? '');
	if (!seen) return null;
	if (seen.includes(last)) return "The judge saw the assistant's whole last message.";
	const tail = last.substring(Math.max(0, last.length - 80));
	if (seen.includes(tail))
		return "The judge saw only the end of the assistant's last message to you.";
	return JUDGE_MISSED_LAST;
}

export const JUDGE_MISSED_LAST = "The judge did not see the assistant's last message to you.";

export function judgeMissedLast(saw: string | null): boolean {
	return saw === JUDGE_MISSED_LAST;
}

export const JUDGE_INPUT_HEADINGS: Array<{ heading: string; title: string }> = [
	{ heading: 'OPERATOR REQUEST (user prompt):', title: 'Your prompt' },
	{
		heading: 'ASSISTANT TURN TEXT (whole turn, in order; cut at the front when long):',
		title: 'What the assistant wrote, as the judge got it'
	},
	{ heading: 'TOOL USE SUMMARY:', title: 'Tool calls' },
	{
		heading: "OPERATOR'S NEXT GENUINE MESSAGE (automated events removed):",
		title: 'Your reply'
	}
];

export function judgeInputSections(
	input: string | null | undefined
): Array<{ title: string; text: string }> | null {
	if (!input) return null;
	const starts = JUDGE_INPUT_HEADINGS.map((h) => input.indexOf(h.heading));
	if (starts.some((s) => s < 0)) return null;
	if (starts.some((s, i) => i > 0 && s <= (starts[i - 1] ?? 0))) return null;
	if (starts[0] !== input.length - input.trimStart().length) return null;
	return JUDGE_INPUT_HEADINGS.map((h, i) => {
		const from = (starts[i] ?? 0) + h.heading.length;
		const to = starts[i + 1] ?? input.length;
		return { title: h.title, text: input.substring(from, to).replace(/^\n+|\s+$/g, '') };
	});
}

export interface TurnPosition {
	index: number;
	total: number;
}

export function turnPosition(
	turns: Array<{ turn_id: number }>,
	turnId: number | null | undefined
): TurnPosition | null {
	if (turnId === null || turnId === undefined) return null;
	const index = turns.findIndex((t) => t.turn_id === turnId);
	return index === -1 ? null : { index: index + 1, total: turns.length };
}
