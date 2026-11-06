import type { PageLoad } from './$types';
import { getFeatures, getSettings } from '$lib/api/settings';
import type { FeatureDto, CompressionEffective, SettingMeta } from '$lib/api/settings';
import { getCapabilities } from '$lib/api/capabilities';
import type { CapabilityRow } from '$lib/api/capabilities';

export interface SettingsPageData {
	features: FeatureDto[] | null;
	loadError: string | null;
	settings: SettingMeta[] | null;
	fileReadCompressionEffective: CompressionEffective | null;
	shellOutputCompressionEffective: CompressionEffective | null;
	steererOpenrouterConfiguredViaEnv: boolean;
	settingsLoadError: string | null;
	capabilities: CapabilityRow[] | null;
	capabilitiesLoadError: string | null;
	find?: string;
}

export const load: PageLoad = async ({ url }): Promise<SettingsPageData> => {
	const [featuresResult, settingsResult, capabilitiesResult] = await Promise.allSettled([
		getFeatures(),
		getSettings(),
		getCapabilities()
	]);

	const features = featuresResult.status === 'fulfilled' ? featuresResult.value : null;
	const loadError =
		featuresResult.status === 'rejected'
			? featuresResult.reason instanceof Error
				? featuresResult.reason.message
				: String(featuresResult.reason)
			: null;

	const settings = settingsResult.status === 'fulfilled' ? settingsResult.value.settings : null;
	const fileReadCompressionEffective =
		settingsResult.status === 'fulfilled'
			? (settingsResult.value.file_read_compression_effective ?? null)
			: null;
	const shellOutputCompressionEffective =
		settingsResult.status === 'fulfilled'
			? (settingsResult.value.shell_output_compression_effective ?? null)
			: null;
	const steererOpenrouterConfiguredViaEnv =
		settingsResult.status === 'fulfilled'
			? settingsResult.value.steerer_openrouter_configured_via_env
			: false;
	const settingsLoadError =
		settingsResult.status === 'rejected'
			? settingsResult.reason instanceof Error
				? settingsResult.reason.message
				: String(settingsResult.reason)
			: null;

	const capabilities = capabilitiesResult.status === 'fulfilled' ? capabilitiesResult.value : null;
	const capabilitiesLoadError =
		capabilitiesResult.status === 'rejected'
			? capabilitiesResult.reason instanceof Error
				? capabilitiesResult.reason.message
				: String(capabilitiesResult.reason)
			: null;

	return {
		features,
		loadError,
		settings,
		fileReadCompressionEffective,
		shellOutputCompressionEffective,
		steererOpenrouterConfiguredViaEnv,
		settingsLoadError,
		capabilities,
		capabilitiesLoadError,
		find: url.searchParams.get('find') ?? ''
	};
};
