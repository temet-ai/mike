import type { PageLoad } from './$types';
import { getFeatures, getSettings, getSyncStatus } from '$lib/api/settings';
import type { FeatureDto, SettingMeta, SyncStatusDto } from '$lib/api/settings';
import { getEvalAlarms } from '$lib/api/evalMetrics';
import type { EvalAlarmsResponse } from '$lib/api/evalMetrics';
import { listEvalBundles } from '$lib/api/evalRuns';
import type { BundleDto } from '$lib/api/evalRuns';

export interface WorkersPageData {
	features: FeatureDto[] | null;
	loadError: string | null;
	settings: SettingMeta[] | null;
	settingsLoadError: string | null;
	syncStatus: SyncStatusDto | null;
	syncStatusError: string | null;
	alarms: EvalAlarmsResponse | null;
	alarmsError: string | null;
	bundles?: BundleDto[] | null;
}

function reason(result: PromiseSettledResult<unknown>): string | null {
	if (result.status === 'fulfilled') return null;
	return result.reason instanceof Error ? result.reason.message : String(result.reason);
}

function valueOf<T>(result: PromiseSettledResult<T>): T | null {
	return result.status === 'fulfilled' ? result.value : null;
}

export const load: PageLoad = async (): Promise<WorkersPageData> => {
	const [featuresResult, settingsResult, syncResult] = await Promise.allSettled([
		getFeatures(),
		getSettings(),
		getSyncStatus()
	]);
	const [alarmsResult, bundlesResult] = await Promise.allSettled([
		getEvalAlarms({ state: 'open' }),
		listEvalBundles()
	]);

	const features = featuresResult.status === 'fulfilled' ? featuresResult.value : null;
	const loadError =
		featuresResult.status === 'rejected'
			? featuresResult.reason instanceof Error
				? featuresResult.reason.message
				: String(featuresResult.reason)
			: null;

	const settings = settingsResult.status === 'fulfilled' ? settingsResult.value.settings : null;
	const settingsLoadError =
		settingsResult.status === 'rejected'
			? settingsResult.reason instanceof Error
				? settingsResult.reason.message
				: String(settingsResult.reason)
			: null;

	const syncStatus = syncResult.status === 'fulfilled' ? syncResult.value : null;
	const syncStatusError =
		syncResult.status === 'rejected'
			? syncResult.reason instanceof Error
				? syncResult.reason.message
				: String(syncResult.reason)
			: null;

	return {
		features,
		loadError,
		settings,
		settingsLoadError,
		syncStatus,
		syncStatusError,
		alarms: valueOf(alarmsResult),
		alarmsError: reason(alarmsResult),
		bundles: valueOf(bundlesResult)?.bundles ?? null
	};
};
