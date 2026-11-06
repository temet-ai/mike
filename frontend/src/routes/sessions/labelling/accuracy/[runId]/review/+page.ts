import type { PageLoad } from './$types';
import { getEvalVocabulary } from '$lib/api/eval';
import type { VocabularyAxis } from '$lib/api/eval';
import { getEvalRun } from '$lib/api/evalRuns';
import type { EvalRunDto } from '$lib/api/evalRuns';

export interface EvalReviewPageData {
	runId: number;
	run: EvalRunDto | null;
	runError: string | null;
	vocabulary: VocabularyAxis[];
	vocabularyError: string | null;
}

function message(e: unknown): string {
	return e instanceof Error ? e.message : String(e);
}

export const load: PageLoad = async ({ params }): Promise<EvalReviewPageData> => {
	const runId = Number(params.runId);
	if (!Number.isInteger(runId) || runId <= 0) {
		return {
			runId,
			run: null,
			runError: `"${params.runId}" is not an accuracy run number`,
			vocabulary: [],
			vocabularyError: null
		};
	}
	const [runResult, vocabResult] = await Promise.allSettled([
		getEvalRun(runId),
		getEvalVocabulary()
	]);
	return {
		runId,
		run: runResult.status === 'fulfilled' ? runResult.value : null,
		runError: runResult.status === 'rejected' ? message(runResult.reason) : null,
		vocabulary: vocabResult.status === 'fulfilled' ? vocabResult.value : [],
		vocabularyError: vocabResult.status === 'rejected' ? message(vocabResult.reason) : null
	};
};
