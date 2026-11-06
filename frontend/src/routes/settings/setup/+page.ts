import type { PageLoad } from './$types';
import { getFeatures, getSettings } from '$lib/api/settings';
import type { FeatureDto, SettingMeta } from '$lib/api/settings';
import { getLlmProviders, getSetupState } from '$lib/api/setup';
import type { LlmProvidersResponse, SetupState } from '$lib/api/setup';

export interface SetupPageData {
	state: SetupState | null;
	stateError: string | null;
	features: FeatureDto[] | null;
	settings: SettingMeta[] | null;
	settingsError: string | null;
	providers: LlmProvidersResponse | null;
	providersError: string | null;
}

function failure(reason: unknown): string {
	return reason instanceof Error ? reason.message : String(reason);
}

export const load: PageLoad = async (): Promise<SetupPageData> => {
	const [state, features, settings, providers] = await Promise.allSettled([
		getSetupState(),
		getFeatures(),
		getSettings(),
		getLlmProviders()
	]);

	return {
		state: state.status === 'fulfilled' ? state.value : null,
		stateError: state.status === 'rejected' ? failure(state.reason) : null,
		features: features.status === 'fulfilled' ? features.value : null,
		settings: settings.status === 'fulfilled' ? settings.value.settings : null,
		settingsError:
			settings.status === 'rejected'
				? failure(settings.reason)
				: features.status === 'rejected'
					? failure(features.reason)
					: null,
		providers: providers.status === 'fulfilled' ? providers.value : null,
		providersError: providers.status === 'rejected' ? failure(providers.reason) : null
	};
};
