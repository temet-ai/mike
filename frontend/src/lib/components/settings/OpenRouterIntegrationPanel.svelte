<script lang="ts">
	import { resolve } from '$app/paths';
	import { formatUsd } from '$lib/format';
	import { ExternalLink } from '@lucide/svelte';
	import { setOpenRouterKey } from '$lib/api/steering';
	import type { OpenRouterUsageDto } from '$lib/api/usage';
	import type { SteererDto } from '$lib/types';

	interface Props {
		usage: OpenRouterUsageDto | null;
		usageError: string | null;
		usageUnavailable: boolean;
		onRetryUsage?: () => void;
		steerers: SteererDto[] | null;
		steerersError: string | null;
		steerersUnavailable: boolean;
		onRetrySteerers?: () => void;
	}

	const {
		usage,
		usageError,
		usageUnavailable,
		onRetryUsage,
		steerers,
		steerersError,
		steerersUnavailable,
		onRetrySteerers
	}: Props = $props();

	let keyInput = $state('');
	let applying = $state(false);
	let keyResult = $state<{ ok: boolean; message: string } | null>(null);

	async function applyKey(event: SubmitEvent): Promise<void> {
		event.preventDefault();
		const key = keyInput.trim();
		if (key.length === 0 || applying) return;
		applying = true;
		try {
			const result = await setOpenRouterKey(key);
			if (result.valid) {
				keyResult = {
					ok: true,
					message: result.label
						? `Key verified — now using ${result.label}.`
						: 'Key verified — now in use.'
				};
				keyInput = '';
				onRetryUsage?.();
			} else {
				keyResult = {
					ok: false,
					message: result.error
						? `Not applied — ${result.error}`
						: 'Not applied — this key could not be verified.'
				};
			}
		} catch (err) {
			keyResult = {
				ok: false,
				message: `Not applied — ${err instanceof Error ? err.message : 'could not reach the server.'}`
			};
		} finally {
			applying = false;
		}
	}

	interface StatTile {
		label: string;
		value: string;
		hint?: string;
	}

	const supportingStats = $derived<StatTile[]>(
		usage && usage.configured
			? [
					{
						label: 'This month',
						value: formatUsd(usage.usage_monthly),
						hint:
							usage.limit !== null
								? `of ${formatUsd(usage.limit)} limit (${formatUsd(usage.limit_remaining)} left, resets ${usage.limit_reset ?? 'unknown'})`
								: 'no monthly limit set'
					},
					{ label: 'Last 7 days', value: formatUsd(usage.usage_weekly) },
					{ label: 'Today', value: formatUsd(usage.usage_daily) },
					{ label: 'Lifetime', value: formatUsd(usage.usage_total) }
				]
			: []
	);

	interface ModelConsumer {
		display_name: string;
		enabled: boolean;
	}

	interface ModelGroup {
		model: string;
		consumers: ModelConsumer[];
	}

	const modelGroups = $derived<ModelGroup[]>(
		steerers === null
			? []
			: Object.values(
					steerers.reduce<Record<string, ModelGroup>>((acc, s) => {
						const group = (acc[s.model] ??= { model: s.model, consumers: [] });
						group.consumers.push({ display_name: s.display_name, enabled: s.enabled });
						return acc;
					}, {})
				).sort((a, b) => a.model.localeCompare(b.model))
	);
</script>

<section id="openrouter" class="ori-section" aria-labelledby="ori-title">
	<div class="ori-card" data-augmented-ui="tl-clip br-clip border">
		<div class="ori-card-header">
			<h2 class="ori-title" id="ori-title">OpenRouter</h2>
			<!-- eslint-disable-next-line svelte/no-navigation-without-resolve -- external OpenRouter dashboard URL, not an app route -->
			<a
				class="ori-topup"
				data-augmented-ui="tr-clip bl-clip border"
				href="https://openrouter.ai/settings/credits"
				target="_blank"
				rel="noopener noreferrer"
			>
				Top up credits
				<ExternalLink size={11} aria-hidden="true" />
			</a>
		</div>
		<p class="ori-desc">The paid account that powers steering verdicts.</p>

		<form class="ori-keyform" onsubmit={applyKey} aria-busy={applying}>
			<label class="ori-keyform-label" for="ori-key-input">OpenRouter API key</label>
			<div class="ori-keyform-row">
				<input
					id="ori-key-input"
					class="ori-keyform-input"
					type="password"
					placeholder="sk-or-v1-…"
					autocomplete="off"
					spellcheck="false"
					disabled={applying}
					aria-describedby="ori-key-result"
					bind:value={keyInput}
				/>
				<button
					type="submit"
					class="ori-keyform-submit"
					data-augmented-ui="tr-clip bl-clip border"
					disabled={applying || keyInput.trim().length === 0}
				>
					Apply key
				</button>
			</div>
			<p
				id="ori-key-result"
				class="ori-keyform-result"
				class:ori-keyform-result--ok={keyResult?.ok === true}
				class:ori-keyform-result--fail={keyResult?.ok === false}
				aria-live="polite"
			>
				{keyResult?.message ?? ''}
			</p>
		</form>

		<div class="ori-balance">
			{#if usageUnavailable}
				<p class="ori-plain-msg">This view needs a newer version than the one currently running.</p>
			{:else if usageError !== null}
				<div class="ori-error" role="alert" data-augmented-ui="tl-clip br-clip border">
					<p class="ori-error-msg">Could not load OpenRouter usage: {usageError}</p>
					{#if onRetryUsage}
						<span class="ori-retry-backing"
							><button
								type="button"
								class="ori-retry"
								data-augmented-ui="tr-clip bl-clip border"
								onclick={() => onRetryUsage?.()}>Retry</button
							></span
						>
					{/if}
				</div>
			{:else if usage === null}
				<p class="ori-plain-msg" aria-busy="true">Loading OpenRouter usage…</p>
			{:else if !usage.configured}
				<div class="ori-empty">
					<p class="ori-empty-msg">
						No OpenRouter key configured yet. Steering runs without a spend account until one is
						set.
					</p>
					<a href={resolve('/settings/steering')} class="ori-empty-link"
						>Set it up in Steering settings</a
					>
				</div>
			{:else if usage.credits_unavailable}
				<div class="ori-stats" data-testid="openrouter-usage-stats">
					<p class="ori-balance-unavailable-msg">
						Balance isn't available for this key type. OpenRouter only reports account balance to
						management keys — use one of those to see credits here.
					</p>
					{#if usage.label}
						<div class="ori-meta">
							<span class="ori-meta-item">Key <code class="ori-key">{usage.label}</code></span>
						</div>
					{/if}
				</div>
			{:else}
				<div class="ori-stats" data-testid="openrouter-usage-stats">
					<div class="ori-headline">
						<span class="ori-headline-label">Credits remaining</span>
						<span class="ori-headline-value">{formatUsd(usage.credits_remaining)}</span>
						<span class="ori-headline-sub"
							>of {formatUsd(usage.credits_total)} total · {formatUsd(usage.credits_used)} used</span
						>
					</div>
					<div class="ori-tiles">
						{#each supportingStats as tile (tile.label)}
							<div class="ori-tile">
								<span class="ori-tile-label">{tile.label}</span>
								<span class="ori-tile-value">{tile.value}</span>
								{#if tile.hint}<span class="ori-tile-hint">{tile.hint}</span>{/if}
							</div>
						{/each}
					</div>
					<div class="ori-meta">
						{#if usage.label}<span class="ori-meta-item"
								>Key <code class="ori-key">{usage.label}</code></span
							>{/if}
						{#if usage.is_free_tier}<span class="ori-badge ori-badge--free">Free tier</span>{/if}
					</div>
				</div>
			{/if}
		</div>

		<div class="ori-models" aria-labelledby="ori-models-title">
			<h3 class="ori-models-title" id="ori-models-title">Models in use</h3>
			<p class="ori-models-desc">
				Every model the fleet's steerers are configured to call, and which steerers use it.
			</p>

			{#if steerersUnavailable}
				<p class="ori-plain-msg">This view needs a newer version than the one currently running.</p>
			{:else if steerersError !== null}
				<div class="ori-error" role="alert" data-augmented-ui="tl-clip br-clip border">
					<p class="ori-error-msg">Could not load steerer models: {steerersError}</p>
					{#if onRetrySteerers}
						<span class="ori-retry-backing"
							><button
								type="button"
								class="ori-retry"
								data-augmented-ui="tr-clip bl-clip border"
								onclick={() => onRetrySteerers?.()}>Retry</button
							></span
						>
					{/if}
				</div>
			{:else if steerers === null}
				<p class="ori-plain-msg" aria-busy="true">Loading steerer models…</p>
			{:else if modelGroups.length === 0}
				<p class="ori-empty-msg">No steerers registered yet.</p>
			{:else}
				<div class="ori-model-list" data-testid="steerer-models-list">
					{#each modelGroups as group (group.model)}
						<div class="ori-model-row">
							{#if group.model === ''}
								<span class="ori-model-id ori-model-id--unknown">(no model set)</span>
							{:else}
								<code class="ori-model-id">{group.model}</code>
							{/if}
							<div class="ori-model-consumers">
								{#each group.consumers as consumer, i (consumer.display_name + '-' + i)}
									<span class="ori-model-chip" class:ori-model-chip--disabled={!consumer.enabled}>
										{consumer.display_name}{#if !consumer.enabled}<span class="ori-model-chip-off">
												(disabled)</span
											>{/if}
									</span>
								{/each}
							</div>
						</div>
					{/each}
				</div>
			{/if}
		</div>
	</div>
</section>

<style>
	.ori-section {
		display: flex;
		flex-direction: column;
	}
	.ori-card {
		--edge-tl: 8px;
		--edge-br: 10px;
		--aug-tl: var(--edge-tl);
		--aug-br: var(--edge-br);
		--aug-border-all: 1px;
		--aug-border-bg: linear-gradient(
			135deg,
			var(--status-info-fg) 0%,
			var(--status-info-border) 35%,
			var(--border-default) 70%
		);
		display: flex;
		flex-direction: column;
		background: var(--surface-card);
		overflow: visible;
	}

	.ori-card-header {
		display: flex;
		align-items: center;
		gap: 10px;
		padding: 16px 16px 0;
	}
	.ori-title {
		font-family: var(--font-display);
		font-size: 13px;
		font-weight: 600;
		text-transform: uppercase;
		letter-spacing: 0.06em;
		color: var(--text-3);
		margin: 0;
	}
	.ori-topup {
		--edge: 6px;
		--aug-tr: var(--edge);
		--aug-bl: var(--edge);
		--aug-tl: 0;
		--aug-br: 0;
		--aug-border-all: 1px;
		--aug-border-bg: var(--interactive-active-border);
		display: inline-flex;
		align-items: center;
		gap: 5px;
		margin-left: auto;
		padding: 5px 12px;
		background: var(--surface-2);
		color: var(--text-1);
		font-family: inherit;
		font-size: 11px;
		font-weight: 600;
		text-decoration: none;
		transition: color 100ms ease-out;
	}
	.ori-topup:hover {
		color: var(--text-strong);
	}
	.ori-topup:focus-visible {
		outline: 2px solid var(--interactive-focus-ring);
		outline-offset: 2px;
	}

	.ori-desc {
		margin: 6px 0 0;
		padding: 0 16px;
		font-size: 12px;
		color: var(--text-3);
		line-height: 1.55;
	}

	.ori-keyform {
		display: flex;
		flex-direction: column;
		gap: 6px;
		padding: 14px 16px;
	}
	.ori-keyform-label {
		font-size: 11px;
		color: var(--text-3);
	}
	.ori-keyform-row {
		display: flex;
		gap: 8px;
	}
	.ori-keyform-input {
		flex: 1 1 auto;
		min-width: 0;
		padding: 5px 8px;
		border: 1px solid var(--border-default);
		background: var(--surface-bg);
		color: var(--text-1);
		border-radius: 4px;
		font-family: var(--font-mono, monospace);
		font-size: 12px;
		transition: border-color 100ms ease-out;
	}
	.ori-keyform-input:hover:not(:disabled) {
		border-color: var(--text-3);
	}
	.ori-keyform-input:focus-visible {
		outline: 2px solid var(--interactive-focus-ring);
		outline-offset: 2px;
		border-color: var(--interactive-focus-ring);
	}
	.ori-keyform-input:disabled {
		cursor: not-allowed;
		opacity: 0.6;
	}
	.ori-keyform-submit {
		--edge: 6px;
		--aug-tr: var(--edge);
		--aug-bl: var(--edge);
		--aug-tl: 0;
		--aug-br: 0;
		--aug-border-all: 1px;
		--aug-border-bg: var(--interactive-active-border);
		flex-shrink: 0;
		padding: 5px 14px;
		background: var(--surface-2);
		color: var(--text-1);
		font-family: inherit;
		font-size: 12px;
		font-weight: 600;
		cursor: pointer;
		transition: color 100ms ease-out;
	}
	.ori-keyform-submit:hover:not(:disabled) {
		color: var(--text-strong);
	}
	.ori-keyform-submit:focus-visible {
		outline: 2px solid var(--interactive-focus-ring);
		outline-offset: 2px;
	}
	.ori-keyform-submit:disabled {
		opacity: 0.45;
		cursor: not-allowed;
	}
	.ori-keyform-result {
		margin: 0;
		min-height: 15px;
		font-size: 12px;
		line-height: 1.5;
		color: var(--text-3);
		overflow-wrap: break-word;
		word-break: break-word;
	}
	.ori-keyform-result--ok {
		color: var(--status-success-fg);
	}
	.ori-keyform-result--fail {
		color: var(--status-danger-fg);
	}

	.ori-balance {
		padding: 14px 16px;
		border-top: 1px solid var(--border-subtle);
	}
	.ori-plain-msg {
		margin: 0;
		font-size: 12px;
		color: var(--text-3);
		line-height: 1.55;
	}

	.ori-error {
		--edge: 10px;
		--aug-tl: var(--edge);
		--aug-br: var(--edge);
		--aug-border-all: 1px;
		--aug-border-bg: linear-gradient(
			135deg,
			var(--status-danger-fg) 0%,
			var(--status-danger-border) 35%,
			var(--border-default) 70%
		);
		display: flex;
		flex-direction: column;
		gap: 12px;
		padding: calc(var(--edge) + 6px) calc(var(--edge) + 10px);
		background: var(--status-danger-bg);
	}
	.ori-error-msg {
		margin: 0;
		font-size: 13px;
		color: var(--status-danger-fg);
		line-height: 1.55;
		overflow-wrap: break-word;
		word-break: break-word;
	}
	.ori-retry-backing {
		align-self: flex-start;
		display: inline-flex;
		background: var(--status-danger-bg);
	}
	.ori-retry {
		--edge: 6px;
		--aug-tr: var(--edge);
		--aug-bl: var(--edge);
		--aug-tl: 0;
		--aug-br: 0;
		--aug-border-all: 1px;
		--aug-border-bg: var(--border-default);
		padding: 6px 14px;
		background: var(--surface-2);
		color: var(--text-1);
		font-family: inherit;
		font-size: 12px;
		cursor: pointer;
		transition: color 100ms ease-out;
	}
	.ori-retry:hover {
		color: var(--text-strong);
		--aug-border-bg: var(--interactive-active-border);
	}

	.ori-empty {
		display: flex;
		flex-direction: column;
		gap: 8px;
		align-items: flex-start;
	}
	.ori-empty-msg {
		margin: 0;
		font-size: 13px;
		color: var(--text-2);
		line-height: 1.6;
		overflow-wrap: break-word;
		word-break: break-word;
	}
	.ori-empty-link {
		font-size: 12px;
		color: var(--text-3);
		text-decoration: underline;
		text-underline-offset: 2px;
		transition: color 0.15s;
	}
	.ori-empty-link:hover {
		color: var(--text-1);
	}
	.ori-empty-link:focus-visible {
		outline: 2px solid var(--interactive-focus-ring);
		outline-offset: 2px;
		border-radius: 2px;
	}

	.ori-stats {
		display: flex;
		flex-direction: column;
		gap: 16px;
	}
	.ori-balance-unavailable-msg {
		margin: 0;
		font-size: 13px;
		color: var(--text-2);
		line-height: 1.6;
		overflow-wrap: break-word;
		word-break: break-word;
	}

	.ori-headline {
		display: flex;
		flex-direction: column;
		gap: 2px;
	}
	.ori-headline-label {
		font-size: 11px;
		text-transform: uppercase;
		letter-spacing: 0.06em;
		color: var(--text-3);
	}
	.ori-headline-value {
		font-family: var(--font-display);
		font-size: 30px;
		font-weight: 700;
		color: var(--text-strong);
		font-variant-numeric: tabular-nums;
		letter-spacing: -0.01em;
	}
	.ori-headline-sub {
		font-size: 12px;
		color: var(--text-3);
	}

	.ori-tiles {
		display: grid;
		grid-template-columns: repeat(auto-fit, minmax(120px, 1fr));
		gap: 12px;
		padding-top: 12px;
		border-top: 1px solid var(--border-subtle);
	}
	.ori-tile {
		display: flex;
		flex-direction: column;
		gap: 2px;
		min-width: 0;
	}
	.ori-tile-label {
		font-size: 10px;
		text-transform: uppercase;
		letter-spacing: 0.06em;
		color: var(--text-3);
	}
	.ori-tile-value {
		font-size: 15px;
		font-weight: 600;
		color: var(--text-1);
		font-variant-numeric: tabular-nums;
	}
	.ori-tile-hint {
		font-size: 11px;
		color: var(--text-3);
		overflow-wrap: break-word;
		word-break: break-word;
	}

	.ori-meta {
		display: flex;
		align-items: center;
		gap: 10px;
		flex-wrap: wrap;
		padding-top: 12px;
		border-top: 1px solid var(--border-subtle);
	}
	.ori-meta-item {
		font-size: 11px;
		color: var(--text-3);
		display: inline-flex;
		align-items: center;
		gap: 5px;
	}
	.ori-key {
		font-family: var(--font-mono, monospace);
		font-size: 11px;
		color: var(--text-2);
		background: var(--surface-2);
		border: 1px solid var(--border-subtle);
		border-radius: 4px;
		padding: 1px 6px;
		word-break: break-all;
	}
	.ori-badge {
		display: inline-flex;
		align-items: center;
		padding: 2px 7px;
		border-radius: 4px;
		font-size: 10px;
		font-weight: 600;
		text-transform: uppercase;
		letter-spacing: 0.05em;
	}
	.ori-badge--free {
		background: var(--status-info-bg);
		color: var(--status-info-fg);
		border: 1px solid var(--status-info-border);
	}

	.ori-models {
		padding: 14px 16px 16px;
		border-top: 1px solid var(--border-subtle);
		display: flex;
		flex-direction: column;
		gap: 8px;
	}
	.ori-models-title {
		font-family: var(--font-display);
		font-size: 11px;
		font-weight: 600;
		text-transform: uppercase;
		letter-spacing: 0.06em;
		color: var(--text-3);
		margin: 0;
	}
	.ori-models-desc {
		margin: 0;
		font-size: 12px;
		color: var(--text-3);
		line-height: 1.55;
	}

	.ori-model-list {
		display: flex;
		flex-direction: column;
		border: 1px solid var(--border-subtle);
		border-radius: 6px;
	}
	.ori-model-row {
		display: flex;
		flex-wrap: wrap;
		align-items: flex-start;
		gap: 10px 14px;
		padding: 10px 14px;
		border-bottom: 1px solid var(--border-subtle);
	}
	.ori-model-row:last-child {
		border-bottom: none;
	}
	.ori-model-id {
		font-family: var(--font-mono, monospace);
		font-size: 12px;
		color: var(--text-1);
		background: var(--surface-2);
		border: 1px solid var(--border-subtle);
		border-radius: 4px;
		padding: 2px 8px;
		overflow-wrap: anywhere;
		word-break: break-word;
		flex: 0 1 260px;
	}
	.ori-model-id--unknown {
		font-family: inherit;
		font-style: italic;
		color: var(--text-3);
	}
	.ori-model-consumers {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: 6px;
		flex: 1 1 200px;
		min-width: 0;
	}
	.ori-model-chip {
		display: inline-flex;
		align-items: center;
		padding: 2px 8px;
		border-radius: 4px;
		font-size: 11px;
		color: var(--text-1);
		background: var(--surface-2);
		border: 1px solid var(--border-subtle);
		overflow-wrap: anywhere;
		word-break: break-word;
	}
	.ori-model-chip--disabled {
		color: var(--text-3);
	}
	.ori-model-chip-off {
		color: var(--text-3);
		font-style: italic;
	}
</style>
