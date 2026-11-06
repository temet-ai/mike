import type { ProposalAction } from '../types.js';

export const EXCERPTS_PER_PROJECT_CAP = 3;
export const EXCERPT_CHAR_CAP = 400;
export const EXCERPT_PROJECTS_CAP = 12;
export const TOTAL_OUTPUT_CHAR_BUDGET = 12_000;

const STOPWORDS = new Set([
	'this',
	'that',
	'with',
	'from',
	'have',
	'will',
	'your',
	'their',
	'when',
	'what',
	'where',
	'which',
	'about',
	'after',
	'before',
	'never',
	'always',
	'should',
	'shall',
	'must',
	'only',
	'also',
	'into',
	'onto',
	'over',
	'under',
	'more',
	'less',
	'very',
	'just',
	'then',
	'than',
	'they',
	'them',
	'there',
	'here',
	'make',
	'made',
	'used',
	'uses',
	'using'
]);

/** Significant, lowercased words from an edit's `match`/`text`, used to find related paragraphs
 *  in another project's CLAUDE.md/rules. Not a full NLP pipeline — a cheap heuristic on purpose,
 *  since the result only ever gates which paragraphs get COUNTED and (for a handful of projects)
 *  quoted; the assistant reads the actual excerpt text and judges agreement/conflict itself. */
export function extractKeyTerms(...texts: string[]): string[] {
	const words = texts.join(' ').toLowerCase().match(/[a-z][a-z'-]{3,}/g) ?? [];
	const unique: string[] = [];
	const seen = new Set<string>();
	for (const w of words) {
		if (STOPWORDS.has(w) || seen.has(w)) continue;
		seen.add(w);
		unique.push(w);
	}
	return unique.slice(0, 12);
}

/** Paragraphs (blank-line-separated) of `content` that contain at least one key term. */
export function paragraphsWithHits(content: string, keyTerms: string[]): string[] {
	if (keyTerms.length === 0) return [];
	return content
		.split(/\n\s*\n/)
		.map((p) => p.trim())
		.filter((p) => p.length > 0)
		.filter((p) => {
			const lower = p.toLowerCase();
			return keyTerms.some((t) => lower.includes(t));
		});
}

const AGREEMENT_TERMS = ['never', "don't", 'do not', 'avoid', 'must not', 'no ', 'not '];
const REQUIREMENT_TERMS = ['always', 'must', 'required', 'require', 'ensure', 'shall'];

/**
 * A cheap, honest heuristic — not a semantic judge. Only distinguishes 'agrees' from
 * 'possible_conflict' for a `drop` proposal (the common "would removing X conflict" case), by
 * looking for prohibition language ("never", "avoid") vs. requirement language ("always",
 * "must") in the hit paragraphs. Every other case with a hit is `possible_conflict` — the
 * cautious default, never a false `agrees`. Zero hits is always `none`.
 */
export function classifyVerdict(action: ProposalAction | undefined, hitParagraphs: string[]): VerdictHint {
	if (hitParagraphs.length === 0) return 'none';
	if (action !== 'drop') return 'possible_conflict';
	const lower = hitParagraphs.join('\n').toLowerCase();
	const agrees = AGREEMENT_TERMS.some((t) => lower.includes(t));
	const requires = REQUIREMENT_TERMS.some((t) => lower.includes(t));
	return agrees && !requires ? 'agrees' : 'possible_conflict';
}

export type VerdictHint = 'none' | 'possible_conflict' | 'agrees';

export interface ProjectHitExcerpt {
	path: string;
	text: string;
	truncated: boolean;
}

export interface RawProjectAnalysis {
	project_path: string;
	project_name: string | null;
	recent_sessions: number;
	match_in_prompt: boolean;
	hitParagraphs: Array<{ path: string; text: string }>;
}

export interface CompactProjectRow {
	project_path: string;
	project_name: string | null;
	recent_sessions: number;
	match_in_prompt: boolean;
	instruction_hits: number;
	verdict_hint: VerdictHint;
	excerpts?: ProjectHitExcerpt[];
}

export interface CompareAcrossProjectsResult {
	current_project: string | null;
	since_days: number;
	projects_considered: number;
	projects: CompactProjectRow[];
	omitted_projects: number;
	truncated: boolean;
}

function truncateExcerptText(text: string): { text: string; truncated: boolean } {
	if (text.length <= EXCERPT_CHAR_CAP) return { text, truncated: false };
	return { text: text.slice(0, EXCERPT_CHAR_CAP), truncated: true };
}

function compareRowRelevance(a: RawProjectAnalysis, b: RawProjectAnalysis): number {
	const aHits = a.hitParagraphs.length;
	const bHits = b.hitParagraphs.length;
	if (aHits !== bHits) return bHits - aHits;
	if (a.match_in_prompt !== b.match_in_prompt) return a.match_in_prompt ? -1 : 1;
	if (a.recent_sessions !== b.recent_sessions) return b.recent_sessions - a.recent_sessions;
	return a.project_path.localeCompare(b.project_path);
}

/**
 * Turns per-project raw analysis (already gathered — file reads and content-search all happen
 * server-side, never sent to the model in full) into the bounded, model-facing payload: excerpts
 * only for the top `EXCERPT_PROJECTS_CAP` projects by hit count, each capped to
 * `EXCERPTS_PER_PROJECT_CAP` paragraphs of `EXCERPT_CHAR_CAP` chars, and the whole `projects`
 * array trimmed from the tail (least relevant first, by `compareRowRelevance`) until the
 * serialized result fits `TOTAL_OUTPUT_CHAR_BUDGET`. Pure and synchronous, so it is the unit
 * of test for the size/ordering guarantee — no network or filesystem I/O.
 */
export function assembleCompareResponse(
	analyses: RawProjectAnalysis[],
	currentProjectPath: string | null,
	sinceDays: number,
	action: ProposalAction | undefined
): CompareAcrossProjectsResult {
	const sorted = [...analyses].sort(compareRowRelevance);
	const excerptEligible = sorted.filter((a) => a.hitParagraphs.length > 0).slice(0, EXCERPT_PROJECTS_CAP);
	const excerptProjectPaths = new Set(excerptEligible.map((a) => a.project_path));

	const rows: CompactProjectRow[] = sorted.map((a) => {
		const row: CompactProjectRow = {
			project_path: a.project_path,
			project_name: a.project_name,
			recent_sessions: a.recent_sessions,
			match_in_prompt: a.match_in_prompt,
			instruction_hits: a.hitParagraphs.length,
			verdict_hint: classifyVerdict(
				action,
				a.hitParagraphs.map((h) => h.text)
			)
		};
		if (excerptProjectPaths.has(a.project_path)) {
			row.excerpts = a.hitParagraphs.slice(0, EXCERPTS_PER_PROJECT_CAP).map((h) => {
				const { text, truncated } = truncateExcerptText(h.text);
				return { path: h.path, text, truncated };
			});
		}
		return row;
	});

	let included = rows;
	let omitted = 0;
	let truncated = rows.length > excerptEligible.length && excerptEligible.length === EXCERPT_PROJECTS_CAP;
	while (JSON.stringify(buildPayload(included, currentProjectPath, sinceDays, rows.length, omitted, true)).length > TOTAL_OUTPUT_CHAR_BUDGET && included.length > 0) {
		included = included.slice(0, -1);
		omitted += 1;
		truncated = true;
	}

	return buildPayload(included, currentProjectPath, sinceDays, rows.length, omitted, truncated);
}

function buildPayload(
	projects: CompactProjectRow[],
	currentProjectPath: string | null,
	sinceDays: number,
	projectsConsidered: number,
	omittedProjects: number,
	truncated: boolean
): CompareAcrossProjectsResult {
	return {
		current_project: currentProjectPath,
		since_days: sinceDays,
		projects_considered: projectsConsidered,
		projects,
		omitted_projects: omittedProjects,
		truncated
	};
}
