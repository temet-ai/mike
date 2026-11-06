import type { PageLoad } from './$types';
import { getSettings } from '$lib/api/settings';
import { listEvalBundles, listEvalGates, listEvalRuns } from '$lib/api/evalRuns';
import type { BundleDto, EvalGateDto, EvalRunDto } from '$lib/api/evalRuns';
import { liveIdFrom } from '$lib/components/judges/judgePageData';

export interface EvalRunsPageData {
	turnRuns: EvalRunDto[];
	sessionRuns: EvalRunDto[];
	bundles: BundleDto[];
	gates: EvalGateDto[];
	liveTurn: number;
	liveSession: number;
	tab: 'turn_judge' | 'session_judge';
	error: string | null;
}

function words(e: unknown): string {
	return e instanceof Error ? e.message : String(e);
}

export const load: PageLoad = async ({ url }): Promise<EvalRunsPageData> => {
	const [turn, session, bundles, gates, settings] = await Promise.allSettled([
		listEvalRuns({ role: 'turn_judge', limit: 200 }),
		listEvalRuns({ role: 'session_judge', limit: 200 }),
		listEvalBundles(),
		listEvalGates(),
		getSettings()
	]);
	const list = settings.status === 'fulfilled' ? settings.value.settings : null;
	return {
		turnRuns: turn.status === 'fulfilled' ? turn.value.items : [],
		sessionRuns: session.status === 'fulfilled' ? session.value.items : [],
		bundles: bundles.status === 'fulfilled' ? bundles.value.bundles : [],
		gates: gates.status === 'fulfilled' ? gates.value.items : [],
		liveTurn: liveIdFrom(list, 'turn_judge'),
		liveSession: liveIdFrom(list, 'session_judge'),
		tab: url.searchParams.get('for') === 'sessions' ? 'session_judge' : 'turn_judge',
		error: turn.status === 'rejected' ? words(turn.reason) : null
	};
};
