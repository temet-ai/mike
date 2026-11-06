<script lang="ts">
	import { ArrowRight } from '@lucide/svelte';
	import { formatBytes } from '$lib/format';
	import { formatCount } from '$lib/time';
	import type {
		ManifestOutcome,
		ManifestStage,
		ManifestTarget,
		TransformationManifestPayload
	} from '$lib/types';

	let {
		payload,
		loading = false,
		error = null,
		onRetry
	}: {
		payload: TransformationManifestPayload | null;
		loading?: boolean;
		error?: string | null;
		onRetry?: () => void;
	} = $props();

	const manifest = $derived(payload?.manifest ?? null);
	const hasManifest = $derived(payload?.has_manifest ?? false);
	const bodyChanged = $derived(
		manifest !== null && manifest.original_sha256 !== manifest.sent_sha256
	);

	const OUTCOME_BADGE: Record<ManifestOutcome, { cls: string; label: string }> = {
		applied: { cls: 'badge-active', label: 'Applied' },
		unchanged: { cls: 'badge-analyzed', label: 'Unchanged' },
		skipped: { cls: 'badge-muted', label: 'Skipped' },
		failed_closed: { cls: 'badge-warning', label: 'Failed closed' }
	};

	function targetKey(target: ManifestTarget, i: number): string {
		return `${target.content_kind}:${target.json_path ?? i}`;
	}

	function hasRange(stage: ManifestStage): boolean {
		return stage.targets.some(
			(t) =>
				t.input_range.start !== t.input_range.end || t.output_range.start !== t.output_range.end
		);
	}
</script>

<div data-slot="transformation-manifest-card" class="flex flex-col gap-3">
	{#if error !== null}
		<div
			class="flex items-center justify-between gap-3 rounded-[2px] border border-[var(--status-danger-border)] bg-[var(--status-danger-bg)] px-3 py-2 text-xs text-[var(--status-danger-fg)]"
		>
			<span>Failed to load transformation manifest: {error}</span>
			{#if onRetry}
				<button
					type="button"
					class="rounded-[2px] border border-[var(--status-danger-border)] px-2 py-0.5 text-label uppercase tracking-wider text-[var(--status-danger-fg)] hover:border-[var(--status-danger-fg)] hover:bg-[var(--status-danger-bg)]"
					onclick={onRetry}>Retry</button
				>
			{/if}
		</div>
	{:else if loading}
		<p class="text-sm italic text-[var(--text-3)]">Loading transformation manifest…</p>
	{:else if !hasManifest}
		<p class="text-sm italic text-[var(--text-faint)]">
			No transformation manifest recorded for this request — it either predates manifest capture, or
			its body reached Anthropic unchanged.
		</p>
	{:else if manifest !== null}
		<header class="flex flex-wrap items-center justify-between gap-2 px-1">
			<div class="flex flex-wrap items-center gap-x-2 gap-y-1 text-label text-[var(--text-3)]">
				<span>schema v{manifest.schema_version}</span>
				<span class="text-[var(--text-faint)]" aria-hidden="true">·</span>
				<span class="font-mono break-all text-[var(--text-2)]">{manifest.original_sha256}</span>
				<ArrowRight class="size-3 shrink-0 text-[var(--text-faint)]" aria-hidden="true" />
				<span class="font-mono break-all text-[var(--text-2)]">{manifest.sent_sha256}</span>
			</div>
			{#if bodyChanged}
				<span class="badge-nx badge-warning">Body changed</span>
			{:else}
				<span class="badge-nx badge-active">Unchanged</span>
			{/if}
		</header>

		{#if manifest.stages.length === 0}
			<p class="text-sm italic text-[var(--text-3)]">
				No transformation stages ran — the body reached Anthropic byte-identical.
			</p>
		{:else}
			<ol class="flex flex-col gap-2" data-slot="transformation-manifest-stages">
				{#each manifest.stages as stage (stage.ordinal)}
					{@const badge = OUTCOME_BADGE[stage.outcome]}
					<li class="tmc-stage flex flex-col gap-2" data-augmented-ui="tl-clip br-clip border">
						<div class="flex flex-wrap items-center justify-between gap-2">
							<div class="flex items-center gap-2">
								<span class="font-mono text-label text-[var(--text-3)]">#{stage.ordinal}</span>
								<span class="font-mono text-sm text-[var(--text-strong)]">{stage.stage}</span>
							</div>
							<span class="badge-nx {badge.cls}">{badge.label}</span>
						</div>
						<div
							class="flex flex-wrap items-center gap-x-3 gap-y-1 text-label text-[var(--text-3)]"
						>
							<span>{stage.policy_id} v{stage.policy_version}</span>
							<span aria-hidden="true">·</span>
							<span class="font-mono break-all">{stage.input_sha256}</span>
							<ArrowRight class="size-3 shrink-0 text-[var(--text-faint)]" aria-hidden="true" />
							<span class="font-mono break-all">{stage.output_sha256}</span>
						</div>
						<p class="text-xs text-[var(--text-2)]">
							<span class="uppercase tracking-wider text-[var(--text-3)]">{stage.reason_code}</span>
							{#if stage.detail}
								<span> — {stage.detail}</span>
							{/if}
						</p>

						{#if stage.targets.length > 0}
							<ul
								class="flex flex-col gap-1.5 border-t border-[var(--border-subtle)] pt-2"
								data-slot="transformation-manifest-targets"
							>
								{#each stage.targets as target, i (targetKey(target, i))}
									{@const targetBadge = OUTCOME_BADGE[target.outcome]}
									<li
										class="flex flex-col gap-1 rounded-[2px] border border-[var(--border-subtle)] bg-[var(--surface-2)] p-2"
									>
										<div class="flex flex-wrap items-center justify-between gap-2">
											<span class="font-mono text-xs text-[var(--text-1)]"
												>{target.content_kind}</span
											>
											<span class="badge-nx {targetBadge.cls}">{targetBadge.label}</span>
										</div>
										<div
											class="flex flex-wrap items-center gap-x-3 gap-y-1 text-label text-[var(--text-3)]"
										>
											{#if target.producer}
												<span>{target.producer}</span>
											{/if}
											{#if target.tool_use_id}
												<span class="font-mono break-all">{target.tool_use_id}</span>
											{/if}
											{#if target.json_path}
												<span class="font-mono break-all">{target.json_path}</span>
											{/if}
										</div>
										<div
											class="flex flex-wrap items-center gap-x-3 gap-y-1 text-label text-[var(--text-2)]"
										>
											<span
												>{formatBytes(target.original_bytes)} → {formatBytes(
													target.sent_bytes
												)}</span
											>
											{#if target.original_tokens !== null}
												<span
													>{formatCount(target.original_tokens)} → {target.sent_tokens !== null
														? formatCount(target.sent_tokens)
														: '—'} tok</span
												>
											{/if}
										</div>
										{#if hasRange(stage) && (target.input_range.start !== target.input_range.end || target.output_range.start !== target.output_range.end)}
											<div class="font-mono text-label text-[var(--text-faint)]">
												in [{target.input_range.start}, {target.input_range.end}) → out [{target
													.output_range.start}, {target.output_range.end})
											</div>
										{/if}
										<p class="text-label text-[var(--text-2)]">
											<span class="uppercase tracking-wider text-[var(--text-3)]"
												>{target.reason_code}</span
											>
											{#if target.detail}
												<span> — {target.detail}</span>
											{/if}
										</p>
										{#if target.recovery_ref}
											<p class="text-label text-[var(--text-faint)]">
												recovery: <span class="font-mono break-all">{target.recovery_ref}</span>
											</p>
										{/if}
									</li>
								{/each}
							</ul>
						{/if}
					</li>
				{/each}
			</ol>
		{/if}
	{/if}
</div>

<style>
	.tmc-stage {
		--edge: 4px;
		--aug-tl: var(--edge);
		--aug-br: var(--edge);
		--aug-border-all: 1px;
		--aug-border-bg: var(--border-subtle);
		background: var(--surface-bg);
		padding: calc(var(--edge) + 8px);
	}
</style>
