<script lang="ts">
	import SettingsPage from '$lib/components/settings/SettingsPage.svelte';

	import { onMount } from 'svelte';
	import { onApiCacheUpdate } from '$lib/stores/cache/apiCache.svelte';
	import WorkerCard from '$lib/components/settings/WorkerCard.svelte';
	import SettingRow from '$lib/components/settings/SettingRow.svelte';
	import StringListControl from '$lib/components/settings/StringListControl.svelte';
	import { ApiError } from '$lib/api';
	import { getFeatures, getSettings, patchFeature, patchSetting } from '$lib/api/settings';
	import type { FeatureDto, SettingMeta, SettingValue } from '$lib/api/settings';
	import type { FlaggingPageData } from './+page';

	const { data }: { data: FlaggingPageData } = $props();

	interface CategoryDef {
		key: string;
		label: string;
		description: string;
		enabledSettingKey: string;
		filtersSettingKey: string;
	}

	const CATEGORIES: CategoryDef[] = [
		{
			key: 'overselling',
			label: 'Overselling',
			description:
				"Flags unsubstantiated success claims — 'done', 'works', 'all tests pass' — made without supporting evidence.",
			enabledSettingKey: 'analysis.categories.overselling.enabled',
			filtersSettingKey: 'analysis.categories.overselling.false_positive_filters'
		},
		{
			key: 'justifying_shortcuts',
			label: 'Justifying shortcuts',
			description:
				'Flags rationalising a shortcut or partial fix instead of doing the proper change.',
			enabledSettingKey: 'analysis.categories.justifying_shortcuts.enabled',
			filtersSettingKey: 'analysis.categories.justifying_shortcuts.false_positive_filters'
		},
		{
			key: 'deflecting_blame',
			label: 'Deflecting blame',
			description:
				'Flags blaming tools, the environment, or the user for a failure the assistant should own.',
			enabledSettingKey: 'analysis.categories.deflecting_blame.enabled',
			filtersSettingKey: 'analysis.categories.deflecting_blame.false_positive_filters'
		},
		{
			key: 'avoiding_work',
			label: 'Avoiding work',
			description: 'Flags deferring, skipping, or hand-waving requested work.',
			enabledSettingKey: 'analysis.categories.avoiding_work.enabled',
			filtersSettingKey: 'analysis.categories.avoiding_work.false_positive_filters'
		},
		{
			key: 'faking_understanding',
			label: 'Faking understanding',
			description:
				'Flags claiming to understand code or requirements without actually verifying them.',
			enabledSettingKey: 'analysis.categories.faking_understanding.enabled',
			filtersSettingKey: 'analysis.categories.faking_understanding.false_positive_filters'
		}
	];

	let features = $state<FeatureDto[]>([]);
	let loadError = $state<string | null>(null);
	let settings = $state<SettingMeta[]>([]);
	let settingsLoadError = $state<string | null>(null);
	let rowLoading = $state<Record<string, boolean>>({});
	let rowError = $state<Record<string, string | null>>({});
	let settingRowLoading = $state<Record<string, boolean>>({});
	let settingRowError = $state<Record<string, string | null>>({});

	$effect(() => {
		features = data.features ?? [];
		loadError = data.loadError;
		settings = data.settings ?? [];
		settingsLoadError = data.settingsLoadError;
	});

	onMount(() => {
		if (data.loadError !== null) void loadFeatures();
		if (data.settingsLoadError !== null) void loadSettings();
		const a = onApiCacheUpdate(
			'/api/settings/features',
			() => {
				void loadFeatures();
			},
			{ prefix: true }
		);
		const b = onApiCacheUpdate(
			'/api/settings',
			() => {
				void loadSettings();
			},
			{ prefix: true }
		);
		return () => {
			a();
			b();
		};
	});

	async function loadFeatures(): Promise<void> {
		loadError = null;
		try {
			features = await getFeatures();
		} catch (err) {
			loadError = err instanceof ApiError ? err.message : String(err);
		}
	}
	async function loadSettings(): Promise<void> {
		settingsLoadError = null;
		try {
			const r = await getSettings();
			settings = r.settings;
		} catch (err) {
			settingsLoadError = err instanceof ApiError ? err.message : String(err);
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
		} catch (err) {
			features = features.map((f) => (f.key === key ? { ...f, enabled: previous } : f));
			rowError = { ...rowError, [key]: err instanceof ApiError ? err.message : String(err) };
		} finally {
			rowLoading = { ...rowLoading, [key]: false };
		}
	}

	async function handleSettingSave(key: string, value: SettingValue): Promise<void> {
		const previous = settings.find((s) => s.key === key);
		if (!previous) return;
		if (previous.kind === 'bool' || previous.kind === 'string_list') {
			settings = settings.map((s) => (s.key === key ? { ...s, value } : s));
		}
		settingRowLoading = { ...settingRowLoading, [key]: true };
		settingRowError = { ...settingRowError, [key]: null };
		try {
			const saved = await patchSetting(key, value);
			settings = settings.map((s) => (s.key === key ? { ...s, ...saved } : s));
		} catch (err) {
			if (previous.kind === 'bool' || previous.kind === 'string_list') {
				settings = settings.map((s) => (s.key === key ? { ...s, value: previous.value } : s));
			}
			settingRowError = {
				...settingRowError,
				[key]: err instanceof ApiError ? err.message : String(err)
			};
		} finally {
			settingRowLoading = { ...settingRowLoading, [key]: false };
		}
	}

	const byKey = $derived(new Map(settings.map((s) => [s.key, s])));
	const featureByKey = $derived(new Map(features.map((f) => [f.key, f])));
	const liveCorrectionFeature = $derived(featureByKey.get('weasel_intercept') ?? null);

	function settingBool(key: string): boolean {
		return byKey.get(key)?.value === true;
	}

	function settingStringList(key: string): string[] {
		const val = byKey.get(key)?.value;
		return Array.isArray(val) ? (val as string[]) : [];
	}

	function settingMaxLen(key: string): number | undefined {
		return byKey.get(key)?.max_len;
	}
</script>

<svelte:head><title>Response flagging — Settings — mike</title></svelte:head>

<SettingsPage
	title="Response flagging"
	current="/settings/flagging"
	loadErrors={[
		loadError === null ? null : `Features: ${loadError}`,
		settingsLoadError === null ? null : `Settings: ${settingsLoadError}`
	]}
	onRetry={() => {
		void loadFeatures();
		void loadSettings();
	}}
>
	{#snippet lede()}
		Flags responses that claim success without evidence, shift blame, or dodge requested work.
	{/snippet}

	<h2 class="sr-only">Master switch</h2>

	<WorkerCard
		name="analysis"
		label="Response flagging"
		description="Turns all flagging on or off. Off, no category runs."
		enabled={settingBool('analysis.enabled')}
		onToggle={(v) => void handleSettingSave('analysis.enabled', v)}
		error={settingRowError['analysis.enabled'] ?? null}
		applies="on_restart"
		accent="teal"
	/>

	<h2 class="st-subhead">Categories</h2>
	<p class="st-subhead-desc">
		Runs over responses already captured, one analyser per category — each can be enabled and tuned
		independently of the others.
	</p>

	{#each CATEGORIES as cat (cat.key)}
		<WorkerCard
			name={cat.key}
			label={cat.label}
			description={cat.description}
			enabled={settingBool(cat.enabledSettingKey)}
			onToggle={(v) => void handleSettingSave(cat.enabledSettingKey, v)}
			error={settingRowError[cat.enabledSettingKey] ?? null}
			applies="on_restart"
			accent="teal"
		>
			<div class="st-field">
				<span class="st-field-label">False-positive filters</span>
				<p class="st-field-help">
					Phrases that, when present near a match, suppress it — to cut false positives. Empty = no
					suppression.
				</p>
				<p class="st-field-hint">
					Filter changes apply on restart, like the category toggle above.
				</p>
				<StringListControl
					items={settingStringList(cat.filtersSettingKey)}
					maxLen={settingMaxLen(cat.filtersSettingKey)}
					onSave={(next) => void handleSettingSave(cat.filtersSettingKey, next)}
				/>
				{#if settingRowError[cat.filtersSettingKey]}
					<p class="st-row-error" role="alert">{settingRowError[cat.filtersSettingKey]}</p>
				{/if}
			</div>
		</WorkerCard>
	{/each}

	<h2 class="st-subhead">Live correction</h2>
	<p class="st-subhead-desc">
		Watches a reply as it streams. At a listed phrase the reply is cut and a corrected answer
		follows in the same message. The categories above only score a reply after it has arrived.
	</p>

	{#if liveCorrectionFeature !== null}
		<WorkerCard
			name="weasel_intercept"
			label="Live correction"
			description="Cuts a reply where a phrase below appears and continues it with a corrected answer."
			enabled={liveCorrectionFeature.enabled}
			onToggle={(v) => void handleFeatureToggle('weasel_intercept', v)}
			error={rowError['weasel_intercept'] ?? null}
			accent="danger"
		>
			<div class="field">
				<span class="field-label">Flagged phrases</span>
				<p class="field-help">
					Literal phrases that trigger the live correction; each match is flagged. Every entry
					renders in full — nothing truncated.
				</p>
				<StringListControl
					items={settingStringList('weasel_intercept.forbidden_words')}
					maxLen={settingMaxLen('weasel_intercept.forbidden_words')}
					onSave={(n) => void handleSettingSave('weasel_intercept.forbidden_words', n)}
				/>
				{#if settingRowError['weasel_intercept.forbidden_words']}
					<p class="field-error" role="alert">
						{settingRowError['weasel_intercept.forbidden_words']}
					</p>
				{/if}
			</div>

			{@const correctionPromptSetting = byKey.get('weasel_intercept.correction_prompt')}
			{#if correctionPromptSetting}
				<div class="st-cfg-list">
					<SettingRow
						setting={correctionPromptSetting}
						loading={settingRowLoading['weasel_intercept.correction_prompt'] ?? false}
						error={settingRowError['weasel_intercept.correction_prompt'] ?? null}
						onSave={(k, v) => void handleSettingSave(k, v)}
					/>
				</div>
			{/if}
		</WorkerCard>
	{/if}
</SettingsPage>

<style>
	.st-subhead {
		font-family: var(--font-display);
		font-size: 12px;
		font-weight: 600;
		text-transform: uppercase;
		letter-spacing: 0.06em;
		color: var(--text-3);
		margin: 6px 0 0;
	}
	.st-subhead-desc {
		margin: 2px 0 -4px;
		font-size: 12px;
		color: var(--text-2);
		line-height: 1.5;
	}
	.st-cfg-list {
		display: flex;
		flex-direction: column;
		border: 1px solid var(--border-default);
		border-radius: 6px;
		overflow: hidden;
		background: var(--surface-card);
	}

	.field {
		display: flex;
		flex-direction: column;
		gap: 6px;
		padding: 14px 20px;
		border-bottom: 1px solid var(--border-subtle);
	}
	.field:last-child {
		border-bottom: none;
	}
	.field-label {
		font-size: 13px;
		font-weight: 600;
		color: var(--text-strong);
	}
	.field-help {
		margin: 0;
		font-size: 12px;
		color: var(--text-2);
		line-height: 1.5;
	}
	.field-error {
		margin: 4px 0 0;
		font-size: 12px;
		color: var(--status-danger-fg);
		line-height: 1.55;
	}

	.st-field {
		display: flex;
		flex-direction: column;
		gap: 6px;
	}
	.st-field-label {
		font-size: 13px;
		font-weight: 600;
		color: var(--text-strong);
	}
	.st-field-help {
		margin: 0;
		font-size: 12px;
		color: var(--text-2);
		line-height: 1.5;
	}
	.st-field-hint {
		margin: 0;
		font-size: 11px;
		color: var(--status-info-fg);
		line-height: 1.5;
	}
	.st-row-error {
		margin: 0;
		font-size: 12px;
		color: var(--status-danger-fg);
		line-height: 1.55;
	}
</style>
