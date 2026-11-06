import type { PageLoad } from './$types';
import { getDeliveredWakes, getGuardEffect, groupWakesByQuestion } from '$lib/api/guardEffect';
import type { GuardEffectEstimate, GuardWake } from '$lib/api/guardEffect';
import { getGuard } from '$lib/api/guard';
import { getImprovementSummary } from '$lib/api/improvement';
import type { ImprovementSummary } from '$lib/api/improvement';
import type { GuardMode } from '$lib/api/guard';

export interface GuardPageQuestion {
	key: string;
	mode: GuardMode;
	counts24h: { rows: number; wouldFire: number } | null;
	effect: GuardEffectEstimate | null;
}

export interface GuardPageData {
	readPointHoldback: number;
	questions: GuardPageQuestion[] | null;
	questionsLoadError: string | null;
	wakesByQuestion: Record<string, GuardWake[]>;
	wakesLoadError: string | null;
	/** The last-7-days nudge counts for the strip; null when the summary could not load. */
	weekNudges: ImprovementSummary['week'] | null;
}

function errorMessage(reason: unknown): string {
	return reason instanceof Error ? reason.message : String(reason);
}

export const load: PageLoad = async (): Promise<GuardPageData> => {
	const [summaryResult, effectResult, wakesResult, improvementResult] = await Promise.allSettled([
		getGuard(),
		getGuardEffect(),
		getDeliveredWakes(),
		getImprovementSummary()
	]);

	const effectByKey = new Map<string, GuardEffectEstimate>();
	let readPointHoldback = 34; // matches promotion::effect::READ_POINT_HOLDBACK's shipped default
	if (effectResult.status === 'fulfilled') {
		readPointHoldback = effectResult.value.readPointHoldback;
		for (const q of effectResult.value.questions) {
			if (q.questionKey) effectByKey.set(q.questionKey, q);
		}
	}

	let questions: GuardPageQuestion[] | null = null;
	let questionsLoadError: string | null = null;
	if (summaryResult.status === 'fulfilled') {
		questions = summaryResult.value.questions
			.filter((q) => q.mode !== 'off')
			.map((q) => ({
				key: q.question,
				mode: q.mode,
				counts24h: { rows: q.counts_24h.rows, wouldFire: q.counts_24h.would_fire },
				effect: effectByKey.get(q.question) ?? null
			}));
	} else {
		questionsLoadError = errorMessage(summaryResult.reason);
	}
	if (questionsLoadError === null && effectResult.status === 'rejected') {
		// The question list itself loaded; only the effect estimate failed. Surface it the same
		// way rather than hiding a partial failure behind a page that otherwise looks complete.
		questionsLoadError = errorMessage(effectResult.reason);
	}

	return {
		readPointHoldback,
		questions,
		questionsLoadError,
		wakesByQuestion:
			wakesResult.status === 'fulfilled' ? groupWakesByQuestion(wakesResult.value) : {},
		wakesLoadError: wakesResult.status === 'rejected' ? errorMessage(wakesResult.reason) : null,
		weekNudges: improvementResult.status === 'fulfilled' ? improvementResult.value.week : null
	};
};
