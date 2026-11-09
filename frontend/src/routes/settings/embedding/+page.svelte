<script lang="ts">
	import { Button } from '$lib/components/ui/button';
	import Field from '$lib/components/ui/field/field.svelte';
	import SettingsPage from '$lib/components/settings/SettingsPage.svelte';
	import PageAlarms from '$lib/components/settings/PageAlarms.svelte';

	import { onMount } from 'svelte';
	import { onApiCacheUpdate } from '$lib/stores/cache/apiCache.svelte';
	import WorkerCard from '$lib/components/settings/WorkerCard.svelte';
	import type { WorkerModel } from '$lib/components/settings/WorkerCard.svelte';
	import SettingRow from '$lib/components/settings/SettingRow.svelte';
	import { ApiError } from '$lib/api';
	import { getSettings, patchSetting } from '$lib/api/settings';
	import type { SettingMeta, SettingValue } from '$lib/api/settings';
	import type { EmbeddingPageData } from './+page';

	const { data }: { data: EmbeddingPageData } = $props();

	// The embedding columns are vector(768); only 768-dimension models can be stored.
	// The host exposes no embedding-model listing, so the list is fixed to the models known to fit.
	const MODELS: WorkerModel[] = [
		{
			id: 'embeddinggemma',
			pricing: 'free · local',
			context: '768 dim · 2K ctx',
			description:
				"Google's EmbeddingGemma-300M, served by the host's OpenAI-compatible embedding server. 768-dimension vectors, 2K-token input. The default here."
		},
		{
			id: 'nomic-embed-text-v1.5',
			pricing: 'free · local',
			context: '768 dim · 8K ctx',
			description:
				"Nomic's open embedding model, served by the host's OpenAI-compatible embedding server. 768-dimension vectors, 8K-token input."
		},
		{
			id: 'nomic-embed-text',
			pricing: 'free · local',
			context: '768 dim',
			description:
				'The same Nomic model under its unversioned name, as an Ollama-style server lists it. 768-dimension vectors.'
		}
	];

	let settings = $state<SettingMeta[]>([]);
	let settingsLoadError = $state<string | null>(null);
	let settingRowLoading = $state<Record<string, boolean>>({});
	let settingRowError = $state<Record<string, string | null>>({});
	let disabling = $state(false);
	let disableReason = $state('');
	let disableAuditMessage = $state('');

	function toggleEmbedding(enabled: boolean): void {
		disableAuditMessage = '';
		if (enabled) void handleSettingSave('embedding.enabled', true);
		else disabling = true;
	}

	async function disableEmbedding(): Promise<void> {
		if (!disableReason.trim()) return;
		await handleSettingSave('embedding.enabled', false, disableReason);
		if (settingRowError['embedding.enabled'] === null) {
			disabling = false;
			disableReason = '';
			disableAuditMessage = 'Embedding disabled. Your reason was recorded in settings history.';
		}
	}

	$effect(() => {
		settings = data.settings ?? [];
		settingsLoadError = data.settingsLoadError;
	});

	onMount(() => {
		if (data.settingsLoadError !== null) void loadSettings();
		const b = onApiCacheUpdate(
			'/api/settings',
			() => {
				void loadSettings();
			},
			{ prefix: true }
		);
		return () => {
			b();
		};
	});

	async function loadSettings(): Promise<void> {
		settingsLoadError = null;
		try {
			const r = await getSettings();
			settings = r.settings;
		} catch (err) {
			settingsLoadError = err instanceof ApiError ? err.message : String(err);
		}
	}

	async function handleSettingSave(
		key: string,
		value: SettingValue,
		changeReason?: string
	): Promise<void> {
		const previous = settings.find((s) => s.key === key);
		if (!previous) return;
		if (previous.kind === 'bool')
			settings = settings.map((s) => (s.key === key ? { ...s, value } : s));
		settingRowLoading = { ...settingRowLoading, [key]: true };
		settingRowError = { ...settingRowError, [key]: null };
		try {
			const saved = await patchSetting(key, value, changeReason);
			settings = settings.map((s) => (s.key === key ? { ...s, ...saved } : s));
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

	const byKey = $derived(new Map(settings.map((s) => [s.key, s])));
	function settingStr(key: string): string {
		const s = byKey.get(key);
		return typeof s?.value === 'string' ? s.value : '';
	}
	// A stored model outside the list keeps showing as the current one instead of vanishing.
	const storedModel = $derived(settingStr('embedding.model'));
	const modelChoices = $derived<WorkerModel[]>(
		storedModel === '' || MODELS.some((m) => m.id === storedModel)
			? MODELS
			: [
					{
						id: storedModel,
						description:
							'Currently stored. It is not a 768-dimension model, so it does not fit the embedding columns.'
					},
					...MODELS
				]
	);
	function settingBool(key: string): boolean {
		return byKey.get(key)?.value === true;
	}
</script>

<svelte:head><title>Embedding — Settings — mike</title></svelte:head>

<SettingsPage
	title="Embedding"
	current="/settings/embedding"
	loadErrors={[settingsLoadError === null ? null : `Settings: ${settingsLoadError}`]}
	onRetry={() => {
		void loadSettings();
	}}
>
	{#snippet lede()}
		Embeds session turns so memory recall and similarity search can find them.
	{/snippet}
	<PageAlarms home="embedding" />

	<WorkerCard
		name="embedding"
		label="Embedding"
		description="Embeds session turns so earlier decisions can be recalled into later sessions."
		enabled={settingBool('embedding.enabled')}
		onToggle={toggleEmbedding}
		disabled={settingRowLoading['embedding.enabled'] ?? false}
		error={settingRowError['embedding.enabled'] ?? settingRowError['embedding.model'] ?? null}
		applies="next_cycle"
		accent="info"
		models={modelChoices}
		selectedModelId={storedModel}
		onSelectModel={(id) => void handleSettingSave('embedding.model', id)}
	>
		{#if disabling}
			<form
				class="flex flex-col gap-3"
				onsubmit={(event) => {
					event.preventDefault();
					void disableEmbedding();
				}}
			>
				<p>
					Disabling embedding pauses semantic indexing and retrieval for every project. Record why
					before saving.
				</p>
				<Field
					label="Reason for disabling embedding"
					bind:value={disableReason}
					required
					disabled={settingRowLoading['embedding.enabled'] ?? false}
				/>
				<div class="flex gap-2">
					<Button
						type="submit"
						disabled={!disableReason.trim() || settingRowLoading['embedding.enabled']}
						>Disable embedding</Button
					>
					<Button
						type="button"
						variant="outline"
						disabled={settingRowLoading['embedding.enabled'] ?? false}
						onclick={() => {
							disabling = false;
							disableReason = '';
						}}>Cancel</Button
					>
				</div>
			</form>
		{/if}
		{@render cfg(['embedding.url', 'embedding.chunk_max_chars'])}
	</WorkerCard>
	{#if disableAuditMessage}<p role="status">{disableAuditMessage}</p>{/if}
</SettingsPage>

{#snippet cfg(keys: string[])}
	<div class="st-cfg-list">
		{#each keys as key (key)}
			{@const s = byKey.get(key)}
			{#if s}
				<SettingRow
					setting={s}
					loading={settingRowLoading[key] ?? false}
					error={settingRowError[key] ?? null}
					onSave={(k, v) => void handleSettingSave(k, v)}
				/>
			{/if}
		{/each}
	</div>
{/snippet}

<style>
	.st-cfg-list {
		display: flex;
		flex-direction: column;
		border: 1px solid var(--border-default);
		border-radius: 6px;
		overflow: hidden;
		background: var(--surface-card);
	}
</style>
