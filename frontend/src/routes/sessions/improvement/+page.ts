import type { PageLoad } from './$types';
import {
	getCorrectionTrend,
	getImprovementSummary,
	getRepeats,
	listActionEffects,
	listLessonCandidates
} from '$lib/api/improvement';
import type {
	ActionEffectItem,
	GuardWake,
	ImprovementSummary,
	LessonCandidate,
	RepeatsResponse,
	TrendResponse
} from '$lib/api/improvement';
import { getDeliveredWakes } from '$lib/api/guardEffect';
import { listProposals } from '$lib/api/directiveProposals';
import type { DirectiveProposal } from '$lib/api/directiveProposals';

/** "Waiting for you" only shows wakes/appends the operator hasn't labelled yet (design-spec.md §3.1). */
async function listUnlabelledWakes(): Promise<GuardWake[]> {
	const wakes = await getDeliveredWakes();
	return wakes.filter((w) => w.operatorLabel === null);
}

export interface ImprovementPageData {
	candidates: LessonCandidate[] | null;
	candidatesError: string | null;
	proposals: DirectiveProposal[] | null;
	proposalsError: string | null;
	wakes: GuardWake[] | null;
	wakesError: string | null;
	summary: ImprovementSummary | null;
	summaryError: string | null;
	actions: ActionEffectItem[] | null;
	actionsError: string | null;
	trend: TrendResponse | null;
	trendError: string | null;
	repeats: RepeatsResponse | null;
	repeatsError: string | null;
}

function errorMessage(reason: unknown): string {
	return reason instanceof Error ? reason.message : String(reason);
}

export const load: PageLoad = async (): Promise<ImprovementPageData> => {
	const [
		candidatesResult,
		proposalsResult,
		wakesResult,
		summaryResult,
		actionsResult,
		trendResult,
		repeatsResult
	] = await Promise.allSettled([
		listLessonCandidates(),
		listProposals('open'),
		listUnlabelledWakes(),
		getImprovementSummary(),
		listActionEffects(),
		getCorrectionTrend({ weeks: 8 }),
		getRepeats({ weeks: 12 })
	]);

	return {
		candidates: candidatesResult.status === 'fulfilled' ? candidatesResult.value.candidates : null,
		candidatesError:
			candidatesResult.status === 'rejected' ? errorMessage(candidatesResult.reason) : null,
		proposals: proposalsResult.status === 'fulfilled' ? proposalsResult.value.proposals : null,
		proposalsError:
			proposalsResult.status === 'rejected' ? errorMessage(proposalsResult.reason) : null,
		wakes: wakesResult.status === 'fulfilled' ? wakesResult.value : null,
		wakesError: wakesResult.status === 'rejected' ? errorMessage(wakesResult.reason) : null,
		summary: summaryResult.status === 'fulfilled' ? summaryResult.value : null,
		summaryError: summaryResult.status === 'rejected' ? errorMessage(summaryResult.reason) : null,
		actions: actionsResult.status === 'fulfilled' ? actionsResult.value.actions : null,
		actionsError: actionsResult.status === 'rejected' ? errorMessage(actionsResult.reason) : null,
		trend: trendResult.status === 'fulfilled' ? trendResult.value : null,
		trendError: trendResult.status === 'rejected' ? errorMessage(trendResult.reason) : null,
		repeats: repeatsResult.status === 'fulfilled' ? repeatsResult.value : null,
		repeatsError: repeatsResult.status === 'rejected' ? errorMessage(repeatsResult.reason) : null
	};
};
