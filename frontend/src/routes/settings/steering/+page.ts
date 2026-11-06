import type { PageLoad } from './$types';
import { getFeatures, getSettings } from '$lib/api/settings';
import type { FeatureDto, SettingMeta } from '$lib/api/settings';
import { listSteerers, listModels } from '$lib/api/steering';
import type { SteererDto, ModelEntry } from '$lib/types';

export interface SteeringPageData {
	features: FeatureDto[] | null;
	loadError: string | null;
	settings: SettingMeta[] | null;
	settingsLoadError: string | null;
	steerers: SteererDto[] | null;
	steerersLoadError: string | null;
	models: ModelEntry[] | null;
	modelsLoadError: string | null;
}

export const load: PageLoad = async (): Promise<SteeringPageData> => {
	const [featuresResult, settingsResult, steerersResult, modelsResult] = await Promise.allSettled([
		getFeatures(),
		getSettings(),
		listSteerers(),
		listModels()
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

	const steerers = steerersResult.status === 'fulfilled' ? steerersResult.value : null;
	const steerersLoadError =
		steerersResult.status === 'rejected'
			? steerersResult.reason instanceof Error
				? steerersResult.reason.message
				: String(steerersResult.reason)
			: null;

	const models = modelsResult.status === 'fulfilled' ? (modelsResult.value?.models ?? null) : null;
	const modelsLoadError =
		modelsResult.status === 'rejected'
			? modelsResult.reason instanceof Error
				? modelsResult.reason.message
				: String(modelsResult.reason)
			: modelsResult.status === 'fulfilled' && modelsResult.value === null
				? 'Failed to load models from OpenRouter.'
				: null;

	return {
		features,
		loadError,
		settings,
		settingsLoadError,
		steerers,
		steerersLoadError,
		models,
		modelsLoadError
	};
};
