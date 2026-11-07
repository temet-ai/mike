import type { EvalAlarmDto } from '$lib/api/evalMetrics';
import type { BundleDto, EvalGateDto, EvalRunDto } from '$lib/api/evalRuns';
import { wordFor } from '$lib/components/eval/labelControl';
import {
	capitalise,
	formatClock,
	formatDate,
	formatDay,
	formatWhen,
	wholePercent
} from '$lib/components/eval/evalReview';

export type JudgeRole = 'turn_judge' | 'session_judge';

export type AlarmHome = 'turn' | 'session' | 'steering' | 'embedding' | 'workers';

export const JUDGE_NAME: Record<JudgeRole, string> = {
	turn_judge: 'Turn judge',
	session_judge: 'Session judge'
};

export const LABELLING_QUERY: Record<JudgeRole, string> = {
	turn_judge: '',
	session_judge: '?for=sessions'
};

export function runWhen(run: EvalRunDto): string {
	return [formatDay(run.created_at), formatClock(run.created_at)].filter(Boolean).join(' ');
}

export function runName(run: EvalRunDto): string {
	return `Labelling accuracy, ${runWhen(run)} · configuration ${run.bundle_id} (${shortModel(run.model)})`;
}

function bundleIdOf(subject: string): number | null {
	const m = /^bundle:(\d+)$/.exec(subject);
	return m ? Number(m[1]) : null;
}

export function alarmHome(alarm: EvalAlarmDto, roles: Map<number, string>): AlarmHome {
	const subject = alarm.subject;
	if (alarm.kind === 'embedding_drift') return 'embedding';
	if (alarm.kind === 'judge_call_loop') return 'session';
	if (subject === 'turn_judge' || subject.startsWith('shift:turn_judge')) return 'turn';
	if (subject === 'session_judge' || subject.startsWith('shift:session_judge')) return 'session';
	const bundle = bundleIdOf(subject);
	const role = bundle !== null ? roles.get(bundle) : undefined;
	if (role === 'turn_judge') return 'turn';
	if (role === 'session_judge') return 'session';
	if (role === 'steerer') return 'steering';
	return 'workers';
}

export function alarmsFor(
	alarms: EvalAlarmDto[],
	bundles: BundleDto[],
	home: AlarmHome
): EvalAlarmDto[] {
	const roles = new Map(bundles.map((b) => [b.id, b.role]));
	return alarms.filter((a) => alarmHome(a, roles) === home);
}

type Detail = Record<string, unknown>;

function num(value: unknown): number | null {
	return typeof value === 'number' && Number.isFinite(value) ? value : null;
}

function obj(value: unknown): Detail | null {
	return value && typeof value === 'object' && !Array.isArray(value) ? (value as Detail) : null;
}

export function share(k: number, n: number): string {
	if (n <= 0) return '—';
	const pct = (k / n) * 100;
	return pct >= 10 ? `${Math.round(pct)}%` : `${pct.toFixed(1)}%`;
}

function backlogSentence(d: Detail): string {
	const stale = num(d['stale_turns']) ?? 0;
	const oldest = formatDate(
		typeof d['oldest_closed_at'] === 'string' ? d['oldest_closed_at'] : null
	);
	const calls = obj(d['calls']);
	const failed = (num(calls?.['http_error']) ?? 0) + (num(calls?.['transport_error']) ?? 0);
	const ok = num(calls?.['ok']) ?? 0;
	const parts = [`${stale.toLocaleString('en-US')} finished turns have not been judged.`];
	if (oldest) parts.push(`The oldest finished on ${oldest}.`);
	if (failed > 0)
		parts.push(
			`${failed.toLocaleString('en-US')} of the last ${(ok + failed).toLocaleString('en-US')} calls to the model failed.`
		);
	return parts.join(' ');
}

function mixSentence(d: Detail): string {
	const cur = obj(obj(d['current'])?.['classes']) ?? {};
	const base = obj(obj(d['baseline'])?.['classes']) ?? {};
	const n = num(obj(d['current'])?.['n']) ?? 0;
	const bn = num(obj(d['baseline'])?.['n']) ?? 0;
	let best: { cls: string; k: number; change: number } | null = null;
	for (const cls of new Set([...Object.keys(cur), ...Object.keys(base)])) {
		const k = num(cur[cls]) ?? 0;
		const bk = num(base[cls]) ?? 0;
		const change = (n > 0 ? k / n : 0) - (bn > 0 ? bk / bn : 0);
		if (!best || change > best.change) best = { cls, k, change };
	}
	const head = 'The labels this judge writes have shifted.';
	if (!best || n === 0) return head;
	const bk = num(base[best.cls]) ?? 0;
	return `${head} Last 7 days: ${share(best.k, n)} of turns labelled ${capitalise(wordFor(best.cls))} (${best.k} of ${n}). The 28 days before: ${share(bk, bn)}.`;
}

export interface AlarmAction {
	label: string;
	path:
		| '/settings/integrations'
		| '/sessions/labelling'
		| '/sessions/labelling/accuracy'
		| '/sessions/labelling/review';
	suffix: string;
}

export interface AlarmWords {
	sentence: string;
	sessionUuid: string | null;
	showDetail: boolean;
	advice: string | null;
	actions: AlarmAction[];
}

function backlogHelp(d: Detail): Pick<AlarmWords, 'advice' | 'actions'> {
	const calls = obj(d['calls']);
	const failed = (num(calls?.['http_error']) ?? 0) + (num(calls?.['transport_error']) ?? 0);
	const settings: AlarmAction[] = [
		{
			label: 'Turns per sweep',
			path: '/sessions/labelling',
			suffix: '#setting-judge_worker.batch_size'
		},
		{
			label: 'Seconds between sweeps',
			path: '/sessions/labelling',
			suffix: '#setting-judge_worker.interval_secs'
		}
	];
	if (failed > 0)
		return {
			advice:
				"Calls to the model are failing, so turns wait. The turn judge's row in the LLM spend table shows how many calls failed; fix the cause there first.",
			actions: [
				{ label: 'LLM spend, failed calls', path: '/settings/integrations', suffix: '#ct-title' },
				...settings
			]
		};
	return {
		advice:
			'No calls failed, so the judge is not keeping up with new turns. Judge more turns per sweep, or sweep more often.',
		actions: settings
	};
}

function mixHelp(d: Detail): Pick<AlarmWords, 'advice' | 'actions'> {
	const bundles = obj(d['current'])?.['bundles'];
	const top = Array.isArray(bundles) ? obj(bundles[0]) : null;
	const id = num(top?.['bundle_id']);
	const who = id !== null ? `Most labels in the last 7 days come from configuration ${id}. ` : '';
	return {
		advice: `${who}A shift right after a configuration change usually means the new configuration labels differently, not that more turns went wrong. Check how often it is right on the turns it labels Corrected before acting.`,
		actions: [
			{
				label: 'Review turns where the judge says Corrected',
				path: '/sessions/labelling/review',
				suffix: '?judge=corrected'
			},
			{ label: 'Labelling accuracy', path: '/sessions/labelling/accuracy', suffix: '' }
		]
	};
}

export function alarmWords(alarm: EvalAlarmDto, home: AlarmHome | null = null): AlarmWords {
	const d = alarm.detail ?? {};
	const plain = (
		sentence: string,
		sessionUuid: string | null = null,
		help: Pick<AlarmWords, 'advice' | 'actions'> = { advice: null, actions: [] }
	): AlarmWords => ({
		sentence,
		sessionUuid,
		showDetail: false,
		...help
	});
	switch (alarm.kind) {
		case 'unjudged_backlog':
			return plain(backlogSentence(d), null, backlogHelp(d));
		case 'label_mix_shift':
			return plain(mixSentence(d), null, mixHelp(d));
		case 'judge_stalled': {
			const last = formatWhen(
				typeof d['last_verdict_at'] === 'string' ? d['last_verdict_at'] : null
			);
			return plain(
				`The judge has stopped labelling turns.${last ? ` It last labelled one on ${last}.` : ''}`
			);
		}
		case 'budget_refusals': {
			const count = num(d['refusals']) ?? num(d['count']);
			return plain(
				`Calls to the model were refused because the spending limit was reached.${count !== null ? ` ${count.toLocaleString('en-US')} calls were refused.` : ''}`,
				null,
				{
					advice: 'Raise the daily limit or wait for it to reset.',
					actions: [{ label: 'LLM spend limit', path: '/settings/integrations', suffix: '' }]
				}
			);
		}
		case 'change_without_gate': {
			const id = bundleIdOf(alarm.subject);
			const since = formatDate(alarm.first_seen_at);
			return plain(
				`${id !== null ? `Configuration ${id}` : 'A configuration'} is in use without its accuracy measured against the accuracy bar. First seen ${since ?? 'at an unknown date'}.`,
				null,
				home === 'turn' || home === 'session'
					? {
							advice: `Measure it with an accuracy run, started from the command line: ${home === 'session' ? 'mike eval run --role session_judge' : 'mike eval run'}.`,
							actions: [
								{
									label: 'Labelling accuracy',
									path: '/sessions/labelling/accuracy',
									suffix: home === 'session' ? '?for=sessions' : ''
								}
							]
						}
					: undefined
			);
		}
		case 'gold_label_by_agent': {
			const session =
				typeof d['session_uuid'] === 'string'
					? d['session_uuid']
					: typeof d['session_id'] === 'string'
						? d['session_id']
						: null;
			return plain('An AI agent wrote a label that should come from you.', session);
		}
		case 'judge_call_loop': {
			const count = num(d['calls']) ?? num(d['count']);
			return plain(
				`The session judge called the model for the same session again and again.${count !== null ? ` ${count.toLocaleString('en-US')} calls in a day.` : ''}`
			);
		}
		default:
			return {
				sentence: `${capitalise(alarm.kind.replaceAll('_', ' '))}.`,
				sessionUuid: null,
				showDetail: true,
				advice: null,
				actions: []
			};
	}
}

export function rubricDate(version: unknown): string | null {
	if (typeof version !== 'string') return null;
	const m = /(\d{4}-\d{2}-\d{2})$/.exec(version);
	return m?.[1] ? formatDate(`${m[1]}T12:00:00`) : null;
}

export function modelOf(bundle: BundleDto | null | undefined): string | null {
	const model = bundle?.components?.model;
	return typeof model === 'string' && model ? model : null;
}

export function shortModel(model: string): string {
	const slash = model.lastIndexOf('/');
	return slash === -1 ? model : model.substring(slash + 1);
}

export function thinkingOf(bundle: BundleDto | null | undefined): string | null {
	const effort = bundle?.components?.reasoning_effort;
	return typeof effort === 'string' && effort ? `thinking: ${effort}` : null;
}

export function configurationLine(bundle: BundleDto | null | undefined): string {
	const rules = rubricDate(bundle?.components?.rubric_version);
	return [modelOf(bundle), thinkingOf(bundle), rules ? `rules of ${rules}` : null]
		.filter((x): x is string => x !== null)
		.join(' · ');
}

export function sameAs(bundle: BundleDto, all: BundleDto[]): BundleDto | null {
	return (
		all.find(
			(b) => b.id !== bundle.id && b.id < bundle.id && b.content_sha256 === bundle.content_sha256
		) ?? null
	);
}

export function sameTurnSet(a: EvalRunDto, b: EvalRunDto): boolean {
	return (
		a.split === b.split &&
		a.label_count === b.label_count &&
		(a.argv?.['holdout_batch'] ?? null) === (b.argv?.['holdout_batch'] ?? null)
	);
}

export function twinNote(bundle: BundleDto, all: BundleDto[], runs: EvalRunDto[]): string | null {
	const twin = sameAs(bundle, all);
	if (!twin) return null;
	const mine = currentRunFor(runs, bundle.id);
	const theirs = currentRunFor(runs, twin.id);
	const head = `same model and instructions as ${twin.id}`;
	if (!mine || !theirs || sameTurnSet(mine, theirs)) return head;
	return `${head}; its score differs because it was measured on different turns`;
}

export function runRole(run: EvalRunDto): JudgeRole {
	const role = run.argv?.['role'];
	if (role === 'session_judge') return 'session_judge';
	if (run.report && 'variants' in run.report) return 'session_judge';
	return 'turn_judge';
}

export function newestFirst(runs: EvalRunDto[]): EvalRunDto[] {
	return [...runs].sort((a, b) => Date.parse(b.created_at) - Date.parse(a.created_at));
}

export function currentRunFor(runs: EvalRunDto[], bundleId: number): EvalRunDto | null {
	return newestFirst(runs).find((r) => r.bundle_id === bundleId && r.status === 'complete') ?? null;
}

export function gateFor(gates: EvalGateDto[], runId: number): EvalGateDto | null {
	return (
		[...gates]
			.filter((g) => g.candidate_run_id === runId || g.noise_run_id === runId)
			.sort((a, b) => Date.parse(b.created_at) - Date.parse(a.created_at))[0] ?? null
	);
}

export function gateForBundle(
	gates: EvalGateDto[],
	runs: EvalRunDto[],
	bundleId: number
): EvalGateDto | null {
	const tested = new Set(runs.filter((r) => r.bundle_id === bundleId).map((r) => r.id));
	return (
		[...gates]
			.filter((g) => tested.has(g.candidate_run_id))
			.sort((a, b) => Date.parse(b.created_at) - Date.parse(a.created_at))[0] ?? null
	);
}

function per100(value: number | null | undefined): string {
	return value === null || value === undefined ? '—' : String(Math.round(value * 100));
}

export function gateBarLines(gate: EvalGateDto): string[] {
	const corrected = gate.report?.corrected;
	return [
		`When it says Corrected, the label agrees ${per100(corrected?.precision)} times in 100 (needs ${per100(gate.floors?.precision)}).`,
		`Of the turns labelled Corrected, it finds ${per100(corrected?.recall)} in 100 (needs ${per100(gate.floors?.recall)}).`
	];
}

export function gateWords(gate: EvalGateDto | null): string {
	if (!gate) return 'Not yet measured against the accuracy bar set for being put in use.';
	const when = formatDate(gate.created_at);
	return `${gate.verdict === 'pass' ? 'Passed' : 'Did not pass'} the accuracy bar set for being put in use, on ${when ?? 'an unknown date'}. ${gateBarLines(gate).join(' ')}`;
}

export function inUseBarWords(bundle: BundleDto | null, gate: EvalGateDto | null): string {
	if (!bundle?.override_reason) return gateWords(gate);
	if (!gate)
		return 'Put in use without its accuracy being measured first. It has not been measured against the accuracy bar since.';
	const when = formatDate(gate.created_at) ?? 'an unknown date';
	const outcome = gate.verdict === 'pass' ? 'passed' : 'did not pass';
	return `Put in use before its accuracy was measured. Measured later, on ${when}, it ${outcome} the accuracy bar set for being put in use. ${gateBarLines(gate).join(' ')}`;
}

export function turnSetWords(run: EvalRunDto): string {
	const role = runRole(run);
	const unit = role === 'session_judge' ? 'session' : 'turn';
	const count = run.label_count;
	const noun = `${count.toLocaleString('en-US')} ${unit}${count === 1 ? '' : 's'}`;
	const labelled = formatDate(run.labels_as_of);
	const upTo = labelled ? `, labelled up to ${labelled}` : '';
	if (count <= 1) return `tried on ${noun}${upTo}`;
	if (run.split === 'holdout') return `${noun} set aside for measuring accuracy${upTo}`;
	const judged =
		run.items_judged !== count ? `${run.items_judged.toLocaleString('en-US')} of ` : '';
	return `${judged}${noun} used while building the judge${upTo}`;
}

export type QuestionKey = 'reaction' | 'delivery';

export interface QuestionCounts {
	total: number;
	reviewed: number;
}

export const QUESTION_NAME: Record<QuestionKey, string> = {
	reaction: 'How you reacted',
	delivery: 'What was delivered'
};

export interface QuestionScore {
	key: QuestionKey;
	agreed: number;
	n: number;
	percent: string;
}

export function questionScore(run: EvalRunDto, key: QuestionKey): QuestionScore | null {
	const axis = run.report?.[key];
	if (!axis || axis.n === 0) return null;
	return { key, agreed: axis.agreement, n: axis.n, percent: wholePercent(axis.agreement, axis.n) };
}

export interface SessionScore {
	agreed: number;
	n: number;
	percent: string;
}

export function sessionScore(run: EvalRunDto): SessionScore | null {
	const variants = obj(run.report?.['variants']);
	const model = obj(variants?.['model_only']) ?? obj(variants?.['both']);
	const confusion = obj(model?.['confusion']);
	const n = num(model?.['n']);
	if (!confusion || n === null || n === 0) return null;
	let agreed = 0;
	for (const [gold, row] of Object.entries(confusion)) agreed += num(obj(row)?.[gold]) ?? 0;
	return { agreed, n, percent: wholePercent(agreed, n) };
}

export function statusWords(status: string): string | null {
	if (status === 'complete') return null;
	if (status === 'running') return 'running';
	if (status === 'failed') return 'failed';
	if (status === 'paused_budget' || status === 'paused') return 'paused, spending limit reached';
	return status.replaceAll('_', ' ');
}

export interface SettingText {
	label?: string;
	description?: string;
}

export const TURN_JUDGE_SETTING_KEYS = [
	'turn_judge.immediate',
	'turn_judge.immediate_delay_ms',
	'judge_worker.batch_size',
	'judge_worker.interval_secs',
	'turn_judge.live_window_days',
	'judge_worker.confidence_threshold',
	'turn_judge.rejudge_daily_call_cap',
	'judge_worker.reasoning_effort',
	'judge_worker.max_output_tokens'
];

export const SESSION_JUDGE_SETTING_KEYS = [
	'session_judge.batch_size',
	'session_judge.min_session_age_secs',
	'session_judge.max_turns_in_prompt',
	'session_judge.max_chars_per_turn',
	'session_judge.poll_interval_secs',
	'session_judge.request_timeout_secs',
	'session_judge.max_attempts_per_day',
	'session_judge.v3_lookback_hours',
	'session_judge.rejudge_daily_call_cap',
	'session_judge.reasoning_effort',
	'session_judge.max_output_tokens'
];

export const SETTING_TEXT: Record<string, SettingText> = {
	'judge_worker.reasoning_effort': {
		description:
			"How much extended thinking the turn judge's own OpenRouter calls do, overriding the shared thinking setting for the turn judge alone. 'inherit' uses the shared setting; picking a level (none, minimal, low, medium, high) sets its own. Thinking is billed like other output, so a higher level costs more. Whether it makes the judge more accurate is known only by measuring: start an accuracy run after changing it."
	},
	'judge_worker.max_output_tokens': {
		description:
			"The most output tokens one of the turn judge's OpenRouter calls may generate. 0 uses the shared Max output tokens setting that every OpenRouter worker falls back to."
	},
	'session_judge.max_output_tokens': {
		description:
			"The most output tokens one of the session judge's OpenRouter calls may generate. 0 uses the shared Max output tokens setting that every OpenRouter worker falls back to."
	},
	'session_judge.reasoning_effort': {
		description:
			"How much extended thinking the session judge's own OpenRouter calls do, overriding the shared thinking setting for the session judge alone. 'inherit' uses the shared setting; picking a level (none, minimal, low, medium, high) sets its own. Thinking is billed like other output, so a higher level costs more. Whether it makes the judge more accurate is known only by measuring: start an accuracy run after changing it."
	},
	'turn_judge.immediate': {
		description:
			"When on, and a configuration is in use, a turn is judged a few seconds after your next message instead of on the next sweep. It adds no calls; it changes when the turn's text leaves the machine."
	},
	'turn_judge.immediate_delay_ms': { label: 'Wait after a turn closes (ms)' },
	'turn_judge.live_window_days': { label: 'Days of turns the sweep covers' },
	'judge_worker.confidence_threshold': {
		label: 'Confidence below which the answer is recorded as unknown',
		description:
			'An answer the model gives with less confidence than this is recorded as unknown instead of as the label it picked.'
	},
	'turn_judge.rejudge_daily_call_cap': {
		label: 'Re-judge calls per day',
		description:
			'The most calls the re-judge of older turns makes in one day (UTC). A re-judge started from the command line can set its own limit.'
	},
	'judge_worker.system_prompt': {
		label: 'Instructions',
		description:
			'The instructions the turn judge follows. They define every label the judge may give, so a label removed from this text is one the judge can no longer give. Reset to default restores the built-in text.'
	},
	'session_judge.max_attempts_per_day': {
		label: 'Attempts per session per day',
		description:
			'The most calls one session can cost in a rolling 24 hours, per model and instructions. A session at the limit is skipped until the window rolls over.'
	},
	'session_judge.v3_lookback_hours': {
		label: 'How far back new session endings are picked up (hours)',
		description:
			'The judge picks up session endings that settled within this many hours, so putting a configuration in use never re-judges older sessions by itself.'
	},
	'session_judge.rejudge_daily_call_cap': { label: 'Re-judge calls per day' },
	'session_judge.system_prompt': {
		label: 'Instructions',
		description:
			'The instructions the session judge follows: how a session can end, and the shape of the two-paragraph summary shown on the session page. Reset to default restores the built-in text.'
	},
	'session_judge.model': {
		description:
			"The model must support OpenRouter's structured output; Anthropic models reject it, so choose an OpenAI or Google model. Changing the model labels sessions again."
	}
};
