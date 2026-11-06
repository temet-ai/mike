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

	const MODELS: WorkerModel[] = [
		{
			id: 'nomic-embed-text-v1.5',
			released: 'Feb 2024',
			pricing: 'free · local',
			context: '768 dim · 8K ctx',
			description:
				"Nomic's open embedding model, run locally via Ollama/Infinity. 768-dimension vectors, 8K-token input. The default here — no API cost and solid general-purpose retrieval quality."
		},
		{
			id: 'openai/text-embedding-3-small',
			released: 'Jan 2024',
			pricing: '$0.02 / 1M',
			context: '1536 dim · 8K ctx',
			description:
				"OpenAI's small embedding model. Cheap and fast at 1536 dimensions; a strong hosted default when you want better quality than local models without paying for the large tier."
		},
		{
			id: 'openai/text-embedding-3-large',
			released: 'Jan 2024',
			pricing: '$0.13 / 1M',
			context: '3072 dim · 8K ctx',
			description:
				"OpenAI's large embedding model. 3072-dimension vectors give the best quality of the OpenAI tier at ~6× the price of 3-small — worth it when recall accuracy matters more than cost."
		},
		{
			id: 'voyage-3',
			released: 'Sep 2024',
			pricing: '$0.06 / 1M',
			context: '1024 dim · 32K ctx',
			description:
				"Voyage AI's general embedding model. 1024 dimensions with a large 32K-token context; strong on long-document and code retrieval, a mid-priced hosted alternative to OpenAI."
		},
		{
			id: 'BAAI/bge-m3',
			released: 'Jan 2024',
			pricing: 'free · local',
			context: '1024 dim · 8K ctx',
			description:
				"BAAI's open multilingual embedding model. Runs locally, 1024 dimensions, strong cross-lingual retrieval — a good open option when your sessions span multiple languages."
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
		models={MODELS}
		selectedModelId={settingStr('embedding.model')}
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
