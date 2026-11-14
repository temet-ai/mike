<script lang="ts" module>
	import type { SettingMeta } from '$lib/api/settings';

	export const DECISIONS_GROUP = 'Decisions';
	export const DECISIONS_FEATURE = 'decisions';

	export function decisionsSettings(settings: readonly SettingMeta[]): SettingMeta[] {
		return settings.filter((s) => s.group === DECISIONS_GROUP && !s.key.endsWith('.mode'));
	}
</script>

<script lang="ts">
	import type { FeatureDto, SettingValue } from '$lib/api/settings';
	import { FEATURE_COPY } from '$lib/settings/featureCopy';
	import FeatureToggleRow from './FeatureToggleRow.svelte';
	import SettingRow from './SettingRow.svelte';
	import SettingsSection from './SettingsSection.svelte';

	interface Props {
		features: FeatureDto[];
		settings: SettingMeta[];
		rowLoading?: Record<string, boolean>;
		rowError?: Record<string, string | null>;
		onToggleFeature?: (key: string, enabled: boolean) => void;
		onSaveSetting?: (key: string, value: SettingValue) => void;
	}

	const {
		features,
		settings,
		rowLoading = {},
		rowError = {},
		onToggleFeature,
		onSaveSetting
	}: Props = $props();

	const feature = $derived(features.find((f) => f.key === DECISIONS_FEATURE) ?? null);
	const rows = $derived(decisionsSettings(settings));
	const copy = FEATURE_COPY[DECISIONS_FEATURE] ?? { label: 'Jev decisions', description: '' };
</script>

<SettingsSection title="Decisions" id="decisions" testId="decisions-settings">
	{#snippet description()}
		Applies to the next decision; nothing needs a restart.
	{/snippet}
	{#if feature !== null}
		<FeatureToggleRow
			name={DECISIONS_FEATURE}
			label={copy.label}
			description={copy.description}
			enabled={feature.enabled}
			loading={rowLoading[DECISIONS_FEATURE] ?? false}
			error={rowError[DECISIONS_FEATURE] ?? null}
			onToggle={(v) => onToggleFeature?.(DECISIONS_FEATURE, v)}
		/>
	{/if}
	{#each rows as setting (setting.key)}
		<SettingRow
			{setting}
			loading={rowLoading[setting.key] ?? false}
			error={rowError[setting.key] ?? null}
			onSave={(key, value) => onSaveSetting?.(key, value)}
		/>
	{/each}
</SettingsSection>
