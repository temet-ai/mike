import { getEvalAlarms, getRejudgeRuns } from '$lib/api/evalMetrics';
import type { EvalAlarmsResponse, RejudgeRunDto } from '$lib/api/evalMetrics';
import { listEvalBundles, listEvalGates, listEvalRunItems, listEvalRuns } from '$lib/api/evalRuns';
import type { BundleDto, EvalGateDto, EvalRunDto } from '$lib/api/evalRuns';
import { getEvalVocabulary } from '$lib/api/eval';
import type { VocabularyAxis } from '$lib/api/eval';
import { getFeatures, getSettings } from '$lib/api/settings';
import type { FeatureDto, SettingMeta } from '$lib/api/settings';
import { currentRunFor } from './judges';
import type { JudgeRole, QuestionCounts, QuestionKey } from './judges';

export interface JudgePageData {
	role: JudgeRole;
	features: FeatureDto[] | null;
	settings: SettingMeta[] | null;
	bundles: BundleDto[];
	runs: EvalRunDto[];
	gates: EvalGateDto[];
	alarms: EvalAlarmsResponse | null;
	rejudgeRuns: RejudgeRunDto[];
	counts: Partial<Record<QuestionKey, QuestionCounts>> | null;
	vocabulary: VocabularyAxis[];
	errors: string[];
	editId: number | null;
}

export const LIVE_KEY: Record<JudgeRole, string> = {
	turn_judge: 'turn_judge.live_bundle_id',
	session_judge: 'session_judge.live_bundle_id'
};

function reason(result: PromiseSettledResult<unknown>): string | null {
	if (result.status === 'fulfilled') return null;
	return result.reason instanceof Error ? result.reason.message : String(result.reason);
}

function valueOf<T>(result: PromiseSettledResult<T>): T | null {
	return result.status === 'fulfilled' ? result.value : null;
}

export function liveIdFrom(settings: SettingMeta[] | null, role: JudgeRole): number {
	const value = settings?.find((s) => s.key === LIVE_KEY[role])?.value;
	return typeof value === 'number' ? value : 0;
}

export async function disagreementCounts(
	runId: number
): Promise<Partial<Record<QuestionKey, QuestionCounts>>> {
	const keys: QuestionKey[] = ['reaction', 'delivery'];
	const results = await Promise.allSettled(
		keys.map((key) => listEvalRunItems(runId, { disagreement: key, include: ['review'], limit: 1 }))
	);
	const out: Partial<Record<QuestionKey, QuestionCounts>> = {};
	results.forEach((r, i) => {
		const key = keys[i];
		if (key && r.status === 'fulfilled')
			out[key] = { total: r.value.total, reviewed: r.value.reviewed };
	});
	return out;
}

export async function loadJudgePage(role: JudgeRole, url: URL): Promise<JudgePageData> {
	const settled = await Promise.allSettled([
		getFeatures(),
		getSettings(),
		listEvalBundles(),
		listEvalRuns({ role, limit: 200 }),
		listEvalGates(),
		getEvalAlarms({ state: 'open' }),
		getRejudgeRuns(),
		getEvalVocabulary()
	]);
	const [features, settings, bundles, runs, gates, alarms, rejudge, vocabulary] = settled;
	const settingList = valueOf(settings)?.settings ?? null;
	const runList = valueOf(runs)?.items ?? [];
	const live = liveIdFrom(settingList, role);
	const measured = role === 'turn_judge' ? currentRunFor(runList, live) : null;
	const counts = measured ? await disagreementCounts(measured.id) : null;
	const edit = Number(url.searchParams.get('edit'));
	return {
		role,
		features: valueOf(features),
		settings: settingList,
		bundles: valueOf(bundles)?.bundles ?? [],
		runs: runList,
		gates: valueOf(gates)?.items ?? [],
		alarms: valueOf(alarms),
		rejudgeRuns: (valueOf(rejudge) ?? []).filter((r) => r.role === role),
		counts,
		vocabulary: valueOf(vocabulary) ?? [],
		errors: settled.map(reason).filter((e): e is string => e !== null),
		editId: Number.isInteger(edit) && edit > 0 ? edit : null
	};
}
