import type { PageLoad } from './$types';
import { getFeatures, getSettings } from '$lib/api/settings';
import type { FeatureDto, SettingMeta } from '$lib/api/settings';
import { getAssistantHealth } from '$lib/api/assistant';
import type { AssistantHealth } from '$lib/api/assistant';
import { USAGE_MODELS_DAYS, getModelUsage } from '$lib/api/usage';

export interface AssistantsPageData {
	features: FeatureDto[] | null;
	loadError: string | null;
	settings: SettingMeta[] | null;
	settingsLoadError: string | null;
	health: AssistantHealth | null;
	healthError: string | null;
	observedClaudeModelIds: string[];
}

export const load: PageLoad = async (): Promise<AssistantsPageData> => {
	const [featuresResult, settingsResult, healthResult, modelUsageResult] =
		await Promise.allSettled([
			getFeatures(),
			getSettings(),
			getAssistantHealth(),
			getModelUsage(USAGE_MODELS_DAYS)
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

	const health = healthResult.status === 'fulfilled' ? healthResult.value : null;
	const healthError =
		healthResult.status === 'rejected'
			? healthResult.reason instanceof Error
				? healthResult.reason.message
				: String(healthResult.reason)
			: null;

	const observedClaudeModelIds =
		modelUsageResult.status === 'fulfilled'
			? [
					...new Set(
						modelUsageResult.value.models
							.map((m) => m.model)
							.filter((m): m is string => m !== null && m !== '')
					)
				]
			: [];

	return {
		features,
		loadError,
		settings,
		settingsLoadError,
		health,
		healthError,
		observedClaudeModelIds
	};
};
