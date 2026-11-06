<script lang="ts">
	import CompressionFlag from '$lib/components/compression/CompressionFlag.svelte';
	import { getContext } from 'svelte';
	import type { BodyPayload, RequestDetail } from '$lib/types';
	import { useLazyDelta, useLazyBody, useLazyManifest } from '$lib/stores/bodyFetcher.svelte';
	import { SESSION_DETAIL_STORE_KEY } from '$lib/context-keys';
	import type { SessionDetailStore } from '$lib/stores/cache/sessionDetailStore.svelte';
	import BodyViewer from './BodyViewer.svelte';
	import TurnDeltaHeader from './TurnDeltaHeader.svelte';
	import TransformationManifestCard from './TransformationManifestCard.svelte';
	import PromptCompositionDiff from '$lib/components/prompts/PromptCompositionDiff.svelte';
	import * as Tabs from '$lib/components/ui/tabs';

	let {
		detail,
		fill = false,
		initialTab = 'original'
	}: {
		detail: RequestDetail;
		fill?: boolean;
		initialTab?: 'original' | 'sent' | 'transformations' | 'diff';
	} = $props();

	const sessionDetailStore = getContext<SessionDetailStore | undefined>(SESSION_DETAIL_STORE_KEY);

	function predecessorIdFn(): number | null {
		if (!sessionDetailStore) return null;
		return sessionDetailStore.predecessorOf(detail.id);
	}

	const absentPayload: BodyPayload = {
		text: null,
		byte_size: 0,
		has_body: false
	};

	const body = useLazyDelta(() => detail, predecessorIdFn);
	const sentBody = useLazyBody(() => detail, 'request_sent');
	const manifest = useLazyManifest(() => detail);

	// svelte-ignore state_referenced_locally
	let activeTab = $state<'original' | 'sent' | 'transformations' | 'diff'>(initialTab);

	const originalFullBody = useLazyBody(() => (activeTab === 'diff' ? detail : null), 'request');

	const renderedPayload = $derived.by((): BodyPayload | null => {
		if (body.mode === 'full') return body.full;
		if (body.deltaBody !== null) return body.deltaBody;
		return body.full;
	});
</script>

<CompressionFlag target={{ kind: 'request', request_id: detail.id }} />

<Tabs.Root bind:value={activeTab} class="flex min-h-0 flex-1 flex-col gap-2">
	<Tabs.List data-slot="request-body-tabs-list">
		<Tabs.Trigger value="original">Original</Tabs.Trigger>
		<Tabs.Trigger value="sent">Sent</Tabs.Trigger>
		<Tabs.Trigger value="transformations">Transformations</Tabs.Trigger>
		<Tabs.Trigger value="diff">Diff</Tabs.Trigger>
	</Tabs.List>

	<Tabs.Content value="original" data-slot="request-body-tab-original">
		{#if detail.request_body_size === 0}
			<BodyViewer payload={absentPayload} label="request" {fill} />
		{:else}
			{@const payloadNow = renderedPayload}
			<TurnDeltaHeader
				mode={body.mode}
				delta={body.delta}
				fullBytes={body.fullBytes}
				deltaBytes={body.deltaBody?.byte_size ?? null}
				loading={body.loading}
				onViewFull={body.viewFull}
				onViewDelta={body.viewDelta}
			/>
			{#if renderedPayload?.reconstructed}
				<div class="rebuilt-strip" data-augmented-ui="tl-clip br-clip border">
					<span class="rebuilt-strip__label">
						Rebuilt from the stored envelope — same content, byte-for-byte differences in message
						key order and spacing
					</span>
				</div>
			{/if}
			{#if body.mode === 'delta' && body.removed.length > 0}
				<div class="removed-strip" data-augmented-ui="tl-clip br-clip border">
					<span class="removed-strip__label">
						{body.removed.length}
						{body.removed.length === 1 ? 'message' : 'messages'} removed this turn
						{#if body.removed[0]?.removed_reason}
							· {body.removed[0].removed_reason}
						{/if}
					</span>
					{#each body.removed as row, i (`${row.message_id}:${i}`)}
						<details class="removed-strip__row">
							<summary class="removed-strip__toggle">
								{row.role} · {row.byte_size.toLocaleString('en-US')} B
							</summary>
							<BodyViewer
								payload={{
									text: JSON.stringify(row.body, null, 2),
									byte_size: row.byte_size,
									has_body: true
								}}
								label="request"
							/>
						</details>
					{/each}
				</div>
			{/if}
			{#if payloadNow !== null}
				<BodyViewer payload={payloadNow} label="request" {fill} />
			{:else if body.error !== null}
				<div
					class="flex items-center justify-between gap-3 rounded-[2px] border border-[var(--status-danger-border)] bg-[var(--status-danger-bg)] px-3 py-2 text-xs text-[var(--status-danger-fg)]"
				>
					<span>Failed to load request body: {body.error}</span>
					<button
						type="button"
						class="rounded-[2px] border border-[var(--status-danger-border)] px-2 py-0.5 text-label uppercase tracking-wider text-[var(--status-danger-fg)] hover:border-[var(--status-danger-fg)] hover:bg-[var(--status-danger-bg)]"
						onclick={body.retry}>Retry</button
					>
				</div>
			{:else if body.loading}
				<p class="text-xs italic text-[var(--text-3)]">Loading body…</p>
			{/if}
		{/if}
	</Tabs.Content>

	<Tabs.Content value="sent" data-slot="request-body-tab-sent">
		{#if detail.request_body_size === 0}
			<BodyViewer payload={absentPayload} label="request" {fill} />
		{:else if sentBody.text !== null}
			<BodyViewer payload={sentBody.text} label="request" {fill} />
		{:else if sentBody.error !== null}
			<div
				class="flex items-center justify-between gap-3 rounded-[2px] border border-[var(--status-danger-border)] bg-[var(--status-danger-bg)] px-3 py-2 text-xs text-[var(--status-danger-fg)]"
			>
				<span>Failed to load sent body: {sentBody.error}</span>
				<button
					type="button"
					class="rounded-[2px] border border-[var(--status-danger-border)] px-2 py-0.5 text-label uppercase tracking-wider text-[var(--status-danger-fg)] hover:border-[var(--status-danger-fg)] hover:bg-[var(--status-danger-bg)]"
					onclick={sentBody.retry}>Retry</button
				>
			</div>
		{:else if sentBody.loading}
			<p class="text-xs italic text-[var(--text-3)]">Loading sent body…</p>
		{/if}
	</Tabs.Content>

	<Tabs.Content value="transformations" data-slot="request-body-tab-transformations">
		<TransformationManifestCard
			payload={manifest.payload}
			loading={manifest.loading}
			error={manifest.error}
			onRetry={manifest.retry}
		/>
	</Tabs.Content>

	<Tabs.Content value="diff" data-slot="request-body-tab-diff">
		<PromptCompositionDiff
			composition={originalFullBody.text?.text ?? null}
			observed={sentBody.text?.text ?? null}
			loading={originalFullBody.loading || sentBody.loading}
			error={originalFullBody.error ?? sentBody.error}
			leftLabel="Original"
			rightLabel="Sent"
			splitPattern={/\n/}
			loadingText="Loading full bodies to diff…"
			errorPrefix="Could not load sent body"
			emptyText="No original or sent body to compare."
		/>
	</Tabs.Content>
</Tabs.Root>

<style>
	.rebuilt-strip {
		--edge: 4px;
		--aug-tl: var(--edge);
		--aug-br: var(--edge);
		--aug-border-all: 1px;
		--aug-border-bg: var(--status-muted-border);
		display: flex;
		flex-direction: column;
		gap: 6px;
		margin-bottom: 8px;
		padding: 8px 10px;
		background: var(--status-muted-bg);
	}

	.rebuilt-strip__label {
		font-size: 10px;
		text-transform: uppercase;
		letter-spacing: 0.04em;
		color: var(--status-muted-fg);
		font-weight: 600;
	}

	.removed-strip {
		--edge: 4px;
		--aug-tl: var(--edge);
		--aug-br: var(--edge);
		--aug-border-all: 1px;
		--aug-border-bg: var(--status-warning-border);
		display: flex;
		flex-direction: column;
		gap: 6px;
		margin-bottom: 8px;
		padding: 8px 10px;
		background: var(--status-warning-bg);
	}

	.removed-strip__label {
		font-size: 10px;
		text-transform: uppercase;
		letter-spacing: 0.04em;
		color: var(--status-warning-fg);
		font-weight: 600;
	}

	.removed-strip__row {
		border-top: 1px solid var(--border-subtle);
		padding-top: 6px;
	}

	.removed-strip__toggle {
		cursor: pointer;
		font-size: 11px;
		color: var(--text-2);
		font-family: var(--font-mono, monospace);
	}

	.removed-strip__toggle:hover {
		color: var(--text-strong);
	}
</style>
