import type { PageLoad } from './$types';
import { getSettings } from '$lib/api/settings';
import type { SettingMeta } from '$lib/api/settings';

export interface RerankerPageData {
	settings: SettingMeta[] | null;
	settingsLoadError: string | null;
}

export const load: PageLoad = async (): Promise<RerankerPageData> => {
	const settingsResult = await Promise.allSettled([getSettings()]);
	const [result] = settingsResult;

	const settings = result.status === 'fulfilled' ? result.value.settings : null;
	const settingsLoadError =
		result.status === 'rejected'
			? result.reason instanceof Error
				? result.reason.message
				: String(result.reason)
			: null;

	return { settings, settingsLoadError };
};
