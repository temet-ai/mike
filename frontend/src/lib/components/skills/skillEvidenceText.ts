import { formatDay } from '$lib/components/eval/evalReview';
import {
	percent,
	type ReviewDecision,
	type ReviewerVerdict,
	type ReviewPlace,
	type SkillLiveUse
} from '$lib/api/skills';

/** The reviewer's decision in a few words, for a figure. */
export const DECISION_TEXT: Record<ReviewDecision, string> = {
	approve: 'Approved',
	patch: 'Approved after a rewrite',
	nothing_to_save: 'Nothing worth saving',
	wrong_place: 'Belongs somewhere else',
	unusable: 'Answer could not be read'
};

const PLACE_TEXT: Record<ReviewPlace, string> = {
	rule: 'a rule',
	existing_skill: 'an existing skill',
	agent_persona: 'an agent persona',
	memory_file: 'a memory file',
	nowhere: 'nowhere, it is not worth keeping'
};

/** The reviewer's verdict on a draft as one or two sentences: what it decided and why. */
export function reviewerSentence(review: ReviewerVerdict): string {
	const named = review.overlap ? review.overlap : null;
	const head = ((): string => {
		switch (review.decision) {
			case 'approve':
				return 'The reviewer approved this draft as written.';
			case 'patch':
				return 'The reviewer approved this draft after rewriting it; Claude reads the rewritten text.';
			case 'nothing_to_save':
				return named
					? `The reviewer found nothing worth saving, because ${named} already covers it.`
					: 'The reviewer found nothing worth saving.';
			case 'wrong_place': {
				const place = review.belongs_in ? PLACE_TEXT[review.belongs_in] : 'another place';
				return named
					? `The reviewer found this belongs in ${place} (${named}), not in a new skill.`
					: `The reviewer found this belongs in ${place}, not in a new skill.`;
			}
			case 'unusable':
				return "The reviewer's answer could not be read.";
		}
	})();
	return review.reason ? `${head} ${review.reason}` : head;
}

function rate(part: number, whole: number): number | null {
	return whole === 0 ? null : part / whole;
}

/** What a shipped skill's real use looks like, in plain sentences. */
export function liveUseSentence(use: SkillLiveUse): string {
	const since = formatDay(use.window_start) ?? use.window_start;
	const loads = `Loaded ${use.loads} ${use.loads === 1 ? 'time' : 'times'} in ${use.load_sessions} ${use.load_sessions === 1 ? 'session' : 'sessions'} since ${since}.`;
	if (use.judged_turns === 0) {
		return `${loads} None of the turns that loaded it has been judged yet.`;
	}
	const own = `Of the ${use.judged_turns} judged turns that loaded it, ${use.corrected_turns} were corrected (${percent(rate(use.corrected_turns, use.judged_turns))})`;
	const base =
		use.base_judged_turns === 0
			? 'the project has no other judged turns in that time to compare with.'
			: `across the whole project ${use.base_corrected_turns} of ${use.base_judged_turns} were (${percent(rate(use.base_corrected_turns, use.base_judged_turns))}).`;
	return `${loads} ${own}; ${base}`;
}

/** Why mike removed a shipped skill on its own, or null when it has not. */
export function retirementSentence(use: SkillLiveUse | null): string | null {
	if (!use?.retired_at) return null;
	const day = formatDay(use.retired_at) ?? use.retired_at;
	const back = 'It is drafted again only when its topic gains a new lesson.';
	if (use.retired_reason === 'corrected_above_base') {
		return `mike removed this skill on ${day} because it was not helping: the turns that loaded it were corrected more often (${percent(rate(use.corrected_turns, use.judged_turns))}) than the project's turns over the same time (${percent(rate(use.base_corrected_turns, use.base_judged_turns))}). ${back}`;
	}
	return `mike removed this skill on ${day} because Claude had not loaded it for 30 days. ${back}`;
}

/** The reviewer's advice on where a draft's lessons belong, and what mike can do about it. */
export interface PlacementAdvice {
	sentence: string;
	followUp: string | null;
}

/**
 * The advice block's words for a verdict that says the lessons do not need a new skill
 * (`wrong_place` or `nothing_to_save`), or null for any other verdict.
 */
export function placementAdvice(review: ReviewerVerdict): PlacementAdvice | null {
	const name = review.overlap ? review.overlap : null;
	if (review.decision === 'nothing_to_save') {
		return {
			sentence: name
				? `The reviewer found nothing worth saving, because ${name} already covers these lessons.`
				: 'The reviewer found nothing worth saving in these lessons.',
			followUp: null
		};
	}
	if (review.decision !== 'wrong_place') return null;
	const overlap = name ? ` It overlaps with ${name}.` : '';
	switch (review.belongs_in) {
		case 'existing_skill':
			return {
				sentence: name
					? `The reviewer advised against a new skill, because the skill ${name} already covers these lessons.`
					: 'The reviewer advised against a new skill, because an existing skill already covers these lessons.',
				followUp: `mike does not yet move these lessons into ${name ?? 'that skill'} by itself.`
			};
		case 'rule':
			return {
				sentence: `The reviewer advised that these lessons work better as a rule in the project than as a skill.${overlap}`,
				followUp:
					"mike does not write rules itself. Copy the lessons, then paste them into the project's CLAUDE.md or a file in .claude/rules."
			};
		case 'agent_persona':
			return {
				sentence: `The reviewer advised that these lessons belong in an agent's instructions, not in a skill.${overlap}`,
				followUp: null
			};
		case 'memory_file':
			return {
				sentence: `The reviewer advised that these lessons belong in a memory file, not in a skill.${overlap}`,
				followUp: null
			};
		case 'nowhere':
			return {
				sentence: `The reviewer advised that these lessons are not worth keeping anywhere, so no skill should be written.${overlap}`,
				followUp: null
			};
		default:
			return {
				sentence: `The reviewer advised that these lessons belong somewhere other than a skill.${overlap}`,
				followUp: null
			};
	}
}

/**
 * Which text an install line holds. `quiet` is true for the expected case, the line that holds
 * the draft the row shows, so only a line that differs stands out.
 */
export function installTextLabel(
	install: { draft_id: number | null },
	rowDraftId: number | null,
	retired: boolean
): { text: string; quiet: boolean } {
	const verb = retired ? 'held' : 'holds';
	if (install.draft_id === null) {
		return { text: `${verb} mike's generated list of lessons`, quiet: false };
	}
	if (rowDraftId !== null && install.draft_id === rowDraftId) {
		return { text: `${verb} this draft`, quiet: true };
	}
	return { text: `${verb} an earlier revision`, quiet: false };
}

/**
 * The accepted lessons as one markdown bullet each, in the order given, full text. A line break
 * inside a lesson becomes a newline plus two spaces so the lesson stays one bullet.
 */
export function lessonsAsBullets(lessons: { text: string; archived_at: string | null }[]): {
	count: number;
	text: string;
} {
	const kept = lessons.filter((l) => !l.archived_at);
	return {
		count: kept.length,
		text: kept.map((l) => `- ${l.text.replace(/\r?\n/g, '\n  ')}`).join('\n')
	};
}
