import type { PageLoad } from './$types';
import { getSettings } from '$lib/api/settings';
import { getEvalRun, listEvalBundles, listEvalGates, listEvalRuns } from '$lib/api/evalRuns';
import type { BundleDto, EvalGateDto, EvalRunDto } from '$lib/api/evalRuns';
import type { QuestionCounts, QuestionKey } from '$lib/components/judges/judges';
import { runRole } from '$lib/components/judges/judges';
import { disagreementCounts, liveIdFrom } from '$lib/components/judges/judgePageData';

export interface TestRunPageData {
	runId: number;
	run: EvalRunDto | null;
	error: string | null;
	runs: EvalRunDto[];
	bundles: BundleDto[];
	gates: EvalGateDto[];
	liveId: number;
	counts: Partial<Record<QuestionKey, QuestionCounts>> | null;
}

export const load: PageLoad = async ({ params }): Promise<TestRunPageData> => {
	const runId = Number(params.runId);
	const empty = { runs: [], bundles: [], gates: [], liveId: 0, counts: null };
	if (!Number.isInteger(runId) || runId <= 0) {
		return { runId, run: null, error: `"${params.runId}" is not an accuracy run number`, ...empty };
	}
	let run: EvalRunDto;
	try {
		run = await getEvalRun(runId);
	} catch (e) {
		return { runId, run: null, error: e instanceof Error ? e.message : String(e), ...empty };
	}
	const role = runRole(run);
	const [runs, bundles, gates, settings, counts] = await Promise.allSettled([
		listEvalRuns({ role, limit: 200 }),
		listEvalBundles(),
		listEvalGates(),
		getSettings(),
		role === 'turn_judge' && run.report?.reaction
			? disagreementCounts(runId)
			: Promise.resolve(null)
	]);
	return {
		runId,
		run,
		error: null,
		runs: runs.status === 'fulfilled' ? runs.value.items : [],
		bundles: bundles.status === 'fulfilled' ? bundles.value.bundles : [],
		gates: gates.status === 'fulfilled' ? gates.value.items : [],
		liveId: liveIdFrom(settings.status === 'fulfilled' ? settings.value.settings : null, role),
		counts: counts.status === 'fulfilled' ? counts.value : null
	};
};
