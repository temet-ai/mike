<script lang="ts">
	import { ChevronsUpDown, Maximize2, Minimize2 } from '@lucide/svelte';
	import { formatBytes } from '$lib/format';
	import type { DeltaShape, MessageDelta } from '$lib/components/message-view/messageWalker';

	interface Props {
		mode: 'delta' | 'full';
		delta: MessageDelta | null;
		fullBytes: number | null;
		deltaBytes: number | null;
		loading: boolean;
		onViewFull: () => void;
		onViewDelta: () => void;
	}

	let { mode, delta, fullBytes, deltaBytes, loading, onViewFull, onViewDelta }: Props = $props();

	const shapeLabel: Record<DeltaShape, string> = {
		'tail-append': 'Turn delta',
		'first-turn': 'First turn — full body shown',
		compaction: 'Conversation reset (compaction)',
		'lineage-broken': 'Lineage break — full body shown'
	};

	const shapeForHeader = $derived<DeltaShape>(delta?.shape ?? 'tail-append');
	const isDeltaMode = $derived(mode === 'delta');

	const byteLabel = $derived.by((): string => {
		if (isDeltaMode && shapeForHeader === 'tail-append' && deltaBytes !== null) {
			return `${formatBytes(deltaBytes)} new`;
		}
		if (fullBytes !== null) {
			return formatBytes(fullBytes);
		}
		return '';
	});

	const flags = $derived.by((): Array<{ label: string; key: string }> => {
		if (!delta) return [];
		const out: Array<{ label: string; key: string }> = [];
		if (delta.systemChanged) out.push({ label: 'system', key: 'system' });
		if (delta.toolsChanged) out.push({ label: 'tools', key: 'tools' });
		if (delta.modelChanged) out.push({ label: 'model', key: 'model' });
		if (delta.paramsChanged) out.push({ label: 'params', key: 'params' });
		return out;
	});
</script>

<header class="turn-delta-header" data-augmented-ui="tl-clip tr-clip border">
	<div class="turn-delta-header__main">
		<span class="turn-delta-header__shape">
			{#if isDeltaMode}
				{shapeLabel[shapeForHeader]}
			{:else}
				Full request body
			{/if}
		</span>
		{#if byteLabel.length > 0}
			<span class="turn-delta-header__bytes">· {byteLabel}</span>
		{/if}
		{#each flags as flag (flag.key)}
			<span class="turn-delta-header__flag-backing">
				<span
					class="turn-delta-header__flag"
					data-augmented-ui="tl-clip br-clip border"
					title="{flag.label} changed this turn"
				>
					{flag.label}
				</span>
			</span>
		{/each}
	</div>

	<div class="turn-delta-header__actions">
		{#if isDeltaMode}
			{#if shapeForHeader === 'tail-append' || shapeForHeader === 'first-turn'}
				<span class="turn-delta-header__btn-backing">
					<button
						type="button"
						class="turn-delta-header__btn"
						data-motion="hover"
						data-augmented-ui="tl-clip br-clip border"
						disabled={loading}
						onclick={onViewFull}
						aria-label="View full request body"
					>
						<Maximize2 class="size-3" aria-hidden="true" />
						<span>View full request</span>
					</button>
				</span>
			{:else}
				<span class="turn-delta-header__btn-backing">
					<button
						type="button"
						class="turn-delta-header__btn"
						data-motion="hover"
						data-augmented-ui="tl-clip br-clip border"
						disabled={loading}
						onclick={onViewFull}
						aria-label="View full request body"
					>
						<ChevronsUpDown class="size-3" aria-hidden="true" />
						<span>View full request</span>
					</button>
				</span>
			{/if}
		{:else}
			<span class="turn-delta-header__btn-backing">
				<button
					type="button"
					class="turn-delta-header__btn"
					data-augmented-ui="tl-clip br-clip border"
					disabled={loading}
					onclick={onViewDelta}
					aria-label="Back to delta view"
				>
					<Minimize2 class="size-3" aria-hidden="true" />
					<span>Back to delta</span>
				</button>
			</span>
		{/if}
	</div>
</header>

<style>
	.turn-delta-header {
		--edge: 4px;
		--aug-tl: var(--edge);
		--aug-tr: var(--edge);
		--aug-border-all: 1px;
		--aug-border-bg: var(--border-default);
		display: flex;
		justify-content: space-between;
		align-items: center;
		gap: 12px;
		padding: calc(var(--edge) + 4px) 10px;
		background: transparent;
		font-size: 11px;
		color: var(--text-2);
		min-height: 32px;
	}

	.turn-delta-header__main {
		display: flex;
		align-items: center;
		gap: 8px;
		flex-wrap: wrap;
	}

	.turn-delta-header__shape {
		text-transform: uppercase;
		letter-spacing: 0.04em;
		font-size: 10px;
		color: var(--text-3);
	}

	.turn-delta-header__bytes {
		font-family: var(--font-mono);
		color: var(--text-strong);
		font-variant-numeric: tabular-nums;
	}

	.turn-delta-header__flag-backing {
		display: inline-flex;
		flex-shrink: 0;
		background: var(--surface-card);
	}
	.turn-delta-header__flag {
		--edge: 2px;
		--aug-tl: var(--edge);
		--aug-br: var(--edge);
		--aug-border-all: 1px;
		--aug-border-bg: var(--status-warning-border);
		display: inline-block;
		padding: calc(var(--edge) - 1px) calc(var(--edge) + 4px);
		font-size: 10px;
		text-transform: uppercase;
		letter-spacing: 0.05em;
		font-weight: 600;
		color: var(--status-warning-fg);
	}

	.turn-delta-header__actions {
		display: flex;
		align-items: center;
		gap: 6px;
	}

	.turn-delta-header__btn-backing {
		display: inline-flex;
		flex-shrink: 0;
		background: var(--surface-card);
	}
	.turn-delta-header__btn {
		--edge: 2px;
		--aug-tl: var(--edge);
		--aug-br: var(--edge);
		--aug-border-all: 1px;
		--aug-border-bg: var(--border-default);
		display: inline-flex;
		align-items: center;
		gap: 4px;
		padding: calc(var(--edge) + 1px) calc(var(--edge) + 6px);
		font-size: 10px;
		text-transform: uppercase;
		letter-spacing: 0.04em;
		color: var(--text-2);
		background: transparent;
		cursor: pointer;
	}

	.turn-delta-header__btn:hover:not(:disabled) {
		color: var(--text-strong);
		--aug-border-bg: var(--interactive-active-border);
	}

	.turn-delta-header__btn:focus-visible {
		outline: none;
		box-shadow: 0 0 0 2px var(--interactive-focus-ring);
	}

	.turn-delta-header__btn:disabled {
		opacity: 0.5;
		cursor: not-allowed;
	}
</style>
