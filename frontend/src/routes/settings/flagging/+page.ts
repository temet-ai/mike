import type { PageLoad } from './$types';
import { getFeatures, getSettings } from '$lib/api/settings';
import type { FeatureDto, SettingMeta } from '$lib/api/settings';

export interface FlaggingPageData {
	features: FeatureDto[] | null;
	loadError: string | null;
	settings: SettingMeta[] | null;
	settingsLoadError: string | null;
}

export const load: PageLoad = async (): Promise<FlaggingPageData> => {
	const [featuresResult, settingsResult] = await Promise.allSettled([getFeatures(), getSettings()]);

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

	return { features, loadError, settings, settingsLoadError };
};
