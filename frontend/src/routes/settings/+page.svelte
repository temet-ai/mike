<script lang="ts">
	import SettingsPage from '$lib/components/settings/SettingsPage.svelte';
	import PageIntro from '$lib/components/shared/PageIntro.svelte';
	import SearchField from '$lib/components/shared/SearchField.svelte';
	import FilteredEmptyState from '$lib/components/shared/FilteredEmptyState.svelte';
	import { SearchX } from '@lucide/svelte';

	import { onMount } from 'svelte';
	import { onApiCacheUpdate } from '$lib/stores/cache/apiCache.svelte';
	import FeatureToggleRow from '$lib/components/settings/FeatureToggleRow.svelte';
	import SettingRow from '$lib/components/settings/SettingRow.svelte';
	import SettingsSection from '$lib/components/settings/SettingsSection.svelte';
	import CapabilityPanel from '$lib/components/settings/CapabilityPanel.svelte';
	import LaunchBackendValidationCard from '$lib/components/settings/LaunchBackendValidationCard.svelte';
	import CompressionForwardingState from '$lib/components/settings/CompressionForwardingState.svelte';
	import HelpHint from '$lib/components/shared/HelpHint.svelte';
	import { Button } from '$lib/components/ui/button';
	import { resolve } from '$app/paths';
	import { ApiError } from '$lib/api';
	import { getFeatures, getSettings, patchFeature, patchSetting } from '$lib/api/settings';
	import type {
		FeatureDto,
		CompressionEffective,
		SettingMeta,
		SettingValue
	} from '$lib/api/settings';
	import { getCapabilities } from '$lib/api/capabilities';
	import type { CapabilityRow } from '$lib/api/capabilities';
	import type { SettingsPageData } from './+page';
	import { FEATURE_COPY, OTHER_PAGE_FEATURES } from '$lib/settings/featureCopy';
	import type { OtherPageHref } from '$lib/settings/featureCopy';

	function isMovedGroup(group: string): boolean {
		const g = group.toLowerCase();
		if (g === 'decisions') return true;
		return [
			'steer',
			'embedding',
			'weasel',
			'analysis',
			'lesson',
			'judge',
			'rerank',
			'search ranking'
		].some((m) => g.includes(m));
	}

	const { data }: { data: SettingsPageData } = $props();

	let features = $state<FeatureDto[]>([]);
	let loadError = $state<string | null>(null);
	let pageLoading = $state(true);
	let settings = $state<SettingMeta[]>([]);
	let fileReadCompressionEffective = $state<CompressionEffective | null>(null);
	let shellOutputCompressionEffective = $state<CompressionEffective | null>(null);
	let settingQuery = $state('');
	const normalizedQuery = $derived(settingQuery.trim().toLowerCase());
	let settingsLoadError = $state<string | null>(null);
	let settingsLoading = $state(true);
	let rowLoading = $state<Record<string, boolean>>({});
	let rowError = $state<Record<string, string | null>>({});
	let settingRowLoading = $state<Record<string, boolean>>({});
	let settingRowError = $state<Record<string, string | null>>({});

	let capabilities = $state<CapabilityRow[]>([]);
	let capabilitiesLoadError = $state<string | null>(null);
	let capabilitiesLoading = $state(true);

	$effect(() => {
		features = data.features ?? [];
		loadError = data.loadError;
		pageLoading = false;
		settings = data.settings ?? [];
		fileReadCompressionEffective = data.fileReadCompressionEffective;
		shellOutputCompressionEffective = data.shellOutputCompressionEffective;
		settingsLoadError = data.settingsLoadError;
		settingsLoading = false;
		capabilities = data.capabilities ?? [];
		capabilitiesLoadError = data.capabilitiesLoadError;
		capabilitiesLoading = false;
	});

	$effect(() => {
		settingQuery = data.find ?? '';
	});

	onMount(() => {
		if (data.loadError !== null) void loadFeatures();
		if (data.settingsLoadError !== null) void loadSettings();
		if (data.capabilitiesLoadError !== null) void loadCapabilities();
		const unsubFeatures = onApiCacheUpdate(
			'/api/settings/features',
			() => {
				void loadFeatures();
			},
			{ prefix: true }
		);
		const unsubSettings = onApiCacheUpdate(
			'/api/settings',
			() => {
				void loadSettings();
			},
			{ prefix: true }
		);
		const unsubCapabilities = onApiCacheUpdate(
			'/api/capabilities',
			() => {
				void loadCapabilities();
			},
			{ prefix: true }
		);
		return () => {
			unsubFeatures();
			unsubSettings();
			unsubCapabilities();
		};
	});

	async function loadFeatures(): Promise<void> {
		pageLoading = true;
		loadError = null;
		try {
			features = await getFeatures();
		} catch (err) {
			loadError = err instanceof ApiError ? err.message : String(err);
		} finally {
			pageLoading = false;
		}
	}
	async function loadSettings(): Promise<void> {
		settingsLoading = true;
		settingsLoadError = null;
		try {
			const result = await getSettings();
			settings = result.settings;
			fileReadCompressionEffective = result.file_read_compression_effective ?? null;
			shellOutputCompressionEffective = result.shell_output_compression_effective ?? null;
		} catch (err) {
			settingsLoadError = err instanceof ApiError ? err.message : String(err);
		} finally {
			settingsLoading = false;
		}
	}
	async function loadCapabilities(): Promise<void> {
		capabilitiesLoading = true;
		capabilitiesLoadError = null;
		try {
			capabilities = await getCapabilities();
		} catch (err) {
			capabilitiesLoadError = err instanceof ApiError ? err.message : String(err);
		} finally {
			capabilitiesLoading = false;
		}
	}

	async function handleFeatureToggle(key: string, newValue: boolean): Promise<void> {
		const previous = features.find((f) => f.key === key)?.enabled ?? true;
		features = features.map((f) => (f.key === key ? { ...f, enabled: newValue } : f));
		rowLoading = { ...rowLoading, [key]: true };
		rowError = { ...rowError, [key]: null };
		try {
			const saved = await patchFeature(key, newValue);
			features = features.map((f) => (f.key === key ? { ...f, ...saved } : f));
			if (key === 'file_read_compression' || key === 'shell_output_compression')
				void refreshCompressionEffective();
		} catch (err) {
			features = features.map((f) => (f.key === key ? { ...f, enabled: previous } : f));
			rowError = { ...rowError, [key]: err instanceof ApiError ? err.message : String(err) };
		} finally {
			rowLoading = { ...rowLoading, [key]: false };
		}
	}
	const FORWARDING_KEYS: ReadonlySet<string> = new Set([
		'file_read_compression.mode',
		'file_read_compression.forward_share',
		'shell_output_compression.mode',
		'shell_output_compression.forward_share'
	]);
	async function refreshCompressionEffective(): Promise<void> {
		try {
			const result = await getSettings();
			fileReadCompressionEffective = result.file_read_compression_effective ?? null;
			shellOutputCompressionEffective = result.shell_output_compression_effective ?? null;
		} catch {
			// keep the last known effective state; the row shows its own save result
		}
	}
	async function handleSettingSave(key: string, value: SettingValue): Promise<void> {
		const previous = settings.find((s) => s.key === key);
		if (!previous) return;
		if (previous.kind === 'bool')
			settings = settings.map((s) => (s.key === key ? { ...s, value } : s));
		settingRowLoading = { ...settingRowLoading, [key]: true };
		settingRowError = { ...settingRowError, [key]: null };
		try {
			const saved = await patchSetting(key, value);
			settings = settings.map((s) => (s.key === key ? { ...s, ...saved } : s));
			if (FORWARDING_KEYS.has(key)) void refreshCompressionEffective();
		} catch (err) {
			if (previous.kind === 'bool')
				settings = settings.map((s) => (s.key === key ? { ...s, value: previous.value } : s));
			settingRowError = {
				...settingRowError,
				[key]: err instanceof ApiError ? err.message : String(err)
			};
		} finally {
			settingRowLoading = { ...settingRowLoading, [key]: false };
		}
	}

	const searching = $derived(normalizedQuery.length > 0);
	function matchesQuery(...parts: string[]): boolean {
		return !searching || parts.join(' ').toLowerCase().includes(normalizedQuery);
	}
	function featureMatches(key: string): boolean {
		const copy = FEATURE_COPY[key];
		return matchesQuery(key, copy?.label ?? '', copy?.description ?? '', copy?.find ?? '');
	}

	const sessionFeatures = $derived(
		features.filter(
			(f) => f.scope === 'session' && !(f.key in OTHER_PAGE_FEATURES) && featureMatches(f.key)
		)
	);
	const globalFeatures = $derived(
		features.filter(
			(f) =>
				f.scope === 'global' &&
				!(f.key in OTHER_PAGE_FEATURES) &&
				f.key in FEATURE_COPY &&
				featureMatches(f.key)
		)
	);
	const otherPageGroups = $derived<{ page: string; href: OtherPageHref; features: FeatureDto[] }[]>(
		(() => {
			if (!searching) return [];
			const byPage = new Map<
				string,
				{ page: string; href: OtherPageHref; features: FeatureDto[] }
			>();
			for (const f of features) {
				const copy = OTHER_PAGE_FEATURES[f.key];
				if (!copy || !matchesQuery(f.key, copy.label, copy.description, copy.page, copy.find ?? ''))
					continue;
				if (!byPage.has(copy.page))
					byPage.set(copy.page, { page: copy.page, href: copy.href, features: [] });
				byPage.get(copy.page)!.features.push(f);
			}
			return [...byPage.values()];
		})()
	);
	const settingGroups = $derived<[string, SettingMeta[]][]>(
		(() => {
			const map = new Map<string, SettingMeta[]>();
			for (const s of settings) {
				if (isMovedGroup(s.group)) continue;
				if (
					normalizedQuery &&
					!`${s.label} ${s.key} ${s.description} ${s.group}`.toLowerCase().includes(normalizedQuery)
				)
					continue;
				if (!map.has(s.group)) map.set(s.group, []);
				map.get(s.group)!.push(s);
			}
			return [...map.entries()];
		})()
	);
	const shownCount = $derived(
		sessionFeatures.length +
			globalFeatures.length +
			otherPageGroups.reduce((count, group) => count + group.features.length, 0) +
			settingGroups.reduce((count, [, rows]) => count + rows.length, 0)
	);

	function toSlug(name: string): string {
		return name.toLowerCase().replace(/\s+/g, '-');
	}
</script>

<svelte:head><title>Settings — mike</title></svelte:head>

{#snippet frcState()}
	{#if fileReadCompressionEffective !== null}
		<CompressionForwardingState effective={fileReadCompressionEffective} />
	{/if}
{/snippet}

{#snippet shellState()}
	{#if shellOutputCompressionEffective !== null}
		<CompressionForwardingState effective={shellOutputCompressionEffective} />
	{/if}
{/snippet}

<SettingsPage
	title="General settings"
	current="/settings"
	loadErrors={[
		capabilitiesLoadError === null ? null : `Capability status: ${capabilitiesLoadError}`,
		loadError === null ? null : `Session features: ${loadError}`,
		settingsLoadError === null ? null : `Settings: ${settingsLoadError}`
	]}
	onRetry={() => {
		if (capabilitiesLoadError !== null) void loadCapabilities();
		if (loadError !== null) void loadFeatures();
		if (settingsLoadError !== null) void loadSettings();
	}}
>
	{#snippet lede()}
		Features that apply to a session, and every other setting. The steerer<HelpHint
			slug="steerer"
		/>, workers, embedding, reranker and flagging have their own pages.
	{/snippet}

	<div class="cfg-find">
		<SearchField
			bind:value={settingQuery}
			label="Find a setting"
			clearLabel="Clear setting search"
			placeholder="Find a setting"
		/>
		<span class="cfg-find-count" role="status"
			>{shownCount} {searching ? (shownCount === 1 ? 'match' : 'matches') : 'settings'}</span
		>
	</div>

	{#if !searching}
		{#if capabilitiesLoading && capabilitiesLoadError === null}
			<p class="cfg-state" aria-busy="true">Loading capability status</p>
		{:else if capabilities.length > 0}
			<CapabilityPanel {capabilities} />
		{:else if capabilitiesLoadError === null}
			<p class="cfg-state">No capability status reported.</p>
		{/if}
	{/if}

	{#if pageLoading && loadError === null}
		<p class="cfg-state" aria-busy="true">Loading session features</p>
	{:else if sessionFeatures.length > 0}
		<SettingsSection
			title="Session features"
			id="session-features"
			badge="Applies to new sessions"
			badgeVariant="session"
		>
			{#each sessionFeatures as feature (feature.key)}
				{@const copy = FEATURE_COPY[feature.key] ?? { label: feature.key, description: '' }}
				<FeatureToggleRow
					name={feature.key}
					label={copy.label}
					description={copy.description}
					enabled={feature.enabled}
					loading={rowLoading[feature.key] ?? false}
					error={rowError[feature.key] ?? null}
					onToggle={(v) => void handleFeatureToggle(feature.key, v)}
				/>
			{/each}
		</SettingsSection>
	{:else if loadError === null && !searching}
		<p class="cfg-state">No session features to configure.</p>
	{/if}

	{#if globalFeatures.length > 0}
		<SettingsSection title="Global settings" id="global-settings">
			{#each globalFeatures as feature (feature.key)}
				{@const copy = FEATURE_COPY[feature.key] ?? { label: feature.key, description: '' }}
				<FeatureToggleRow
					name={feature.key}
					label={copy.label}
					description={copy.description}
					enabled={feature.enabled}
					loading={rowLoading[feature.key] ?? false}
					error={rowError[feature.key] ?? null}
					onToggle={(v) => void handleFeatureToggle(feature.key, v)}
				/>
			{/each}
		</SettingsSection>
	{/if}

	{#each otherPageGroups as group (group.href)}
		<SettingsSection title="{group.page} page" id="other-page-{toSlug(group.page)}">
			{#snippet action()}
				<Button variant="outline" size="sm" href={resolve(group.href)}>Open {group.page}</Button>
			{/snippet}
			{#each group.features as feature (feature.key)}
				{@const copy = OTHER_PAGE_FEATURES[feature.key]!}
				<FeatureToggleRow
					name={feature.key}
					label={copy.label}
					description={copy.description}
					enabled={feature.enabled}
					loading={rowLoading[feature.key] ?? false}
					error={rowError[feature.key] ?? null}
					onToggle={(v) => void handleFeatureToggle(feature.key, v)}
				/>
			{/each}
		</SettingsSection>
	{/each}

	{#if !searching || settingGroups.length > 0}
		<section id="configuration-catalogue" aria-label="Every setting" data-reveal>
			<PageIntro title="Every setting" description="Each row says when a change applies." />
		</section>
	{/if}

	{#if settingsLoading && settingsLoadError === null}
		<p class="cfg-state" aria-busy="true">Loading settings</p>
	{:else if settingGroups.length === 0 && settingsLoadError === null}
		{#if shownCount === 0}
			<FilteredEmptyState
				icon={SearchX}
				filtered={searching}
				emptyText="No settings to show"
				filteredText={`No setting matches “${settingQuery}”`}
				onClearFilters={() => (settingQuery = '')}
				clearLabel="Clear setting search"
			/>
		{/if}
	{:else}
		{#each settingGroups as [group, groupSettings] (group)}
			{#if group === 'Launcher'}
				{@const backendSetting = groupSettings.find((s) => s.key === 'launch_backend')}
				{#if backendSetting && typeof backendSetting.value === 'string'}
					<LaunchBackendValidationCard backend={backendSetting.value} />
				{/if}
			{/if}
			{@const fileReadState =
				group === 'File read compression' && fileReadCompressionEffective !== null}
			{@const shellOutputState =
				group === 'Shell-output compression' && shellOutputCompressionEffective !== null}
			{@const forwardingState = fileReadState || shellOutputState}
			<SettingsSection
				title={group}
				id={toSlug(group)}
				level="h3"
				badge={forwardingState ? 'Applies to new sessions' : undefined}
				badgeVariant="session"
				description={fileReadState ? frcState : shellOutputState ? shellState : undefined}
			>
				{#each groupSettings as setting (setting.key)}
					<SettingRow
						{setting}
						loading={settingRowLoading[setting.key] ?? false}
						error={settingRowError[setting.key] ?? null}
						onSave={(key, value) => void handleSettingSave(key, value)}
					/>
				{/each}
			</SettingsSection>
		{/each}
	{/if}
</SettingsPage>

<style>
	.cfg-state {
		margin: 0;
		color: var(--text-3);
		font-size: var(--text-sm);
	}
	.cfg-find {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: var(--space-3);
	}
	.cfg-find-count {
		color: var(--text-3);
		font-size: var(--text-xs);
	}
</style>
