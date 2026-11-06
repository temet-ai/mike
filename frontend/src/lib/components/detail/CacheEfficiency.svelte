<script lang="ts">
	import type { HTMLAttributes } from 'svelte/elements';
	import { cn, type WithElementRef } from '$lib/utils';
	import type { TokenUsage } from '$lib/types';
	import { formatCount } from '$lib/time';
	import { cachePercent } from '$lib/format';
	import Tooltip from '../shared/Tooltip.svelte';

	interface LegendHelp {
		text: string;
		slug: string;
	}
	const LEGEND_HELP: Record<string, LegendHelp> = {
		'Cache read': {
			text: 'Tokens served from prompt cache (10× cheaper than input).',
			slug: 'prompt-cache'
		},
		'Cache create': {
			text: 'Tokens written to prompt cache (1.25× input rate).',
			slug: 'prompt-cache'
		},
		'Uncached input': {
			text: 'Input tokens billed at full rate (no cache hit).',
			slug: 'prompt-cache'
		},
		Output: {
			text: 'Output tokens — separate column from input-side cache.',
			slug: 'usage-tokens'
		}
	};

	interface Props extends WithElementRef<HTMLAttributes<HTMLDivElement>> {
		usage: TokenUsage | null;
	}

	let { ref = $bindable(null), class: className, usage, ...restProps }: Props = $props();

	const CIRCUMFERENCE = 314.16;

	const cachePct = $derived(cachePercent(usage));

	const ringColor = $derived.by(() => {
		if (cachePct === null) return 'var(--text-3)';
		if (cachePct >= 80) return 'var(--status-success-fg)';
		if (cachePct >= 40) return 'var(--status-warning-fg)';
		return 'var(--text-2)';
	});

	const strokeOffset = $derived(
		cachePct === null ? CIRCUMFERENCE : CIRCUMFERENCE * (1 - cachePct / 100)
	);

	const ariaLabel = $derived(
		cachePct === null ? 'Cache efficiency: no usage recorded' : `Cache hit rate ${cachePct}%`
	);

	interface LegendRow {
		label: string;
		color: string;
		count: number;
		pct: number | null;
		showPct: boolean;
	}

	const legendRows = $derived.by((): LegendRow[] => {
		if (usage === null) return [];
		const input = usage.input_tokens ?? 0;
		const output = usage.output_tokens ?? 0;
		const cacheRead = usage.cache_read_input_tokens ?? 0;
		const cacheCreate = usage.cache_creation_input_tokens ?? 0;
		const inputTotal = input + cacheRead + cacheCreate;
		const pctOf = (n: number): number | null =>
			inputTotal <= 0 ? null : Math.round((n / inputTotal) * 100);
		return [
			{
				label: 'Cache read',
				color: 'var(--data-teal-fg)',
				count: cacheRead,
				pct: pctOf(cacheRead),
				showPct: true
			},
			{
				label: 'Cache create',
				color: 'var(--status-info-fg)',
				count: cacheCreate,
				pct: pctOf(cacheCreate),
				showPct: true
			},
			{
				label: 'Uncached input',
				color: 'var(--text-3)',
				count: input,
				pct: pctOf(input),
				showPct: true
			},
			{
				label: 'Output',
				color: 'var(--text-2)',
				count: output,
				pct: null,
				showPct: false
			}
		];
	});
</script>

<div
	bind:this={ref}
	data-slot="cache-efficiency"
	class={cn('flex flex-col gap-3', className)}
	{...restProps}
>
	{#if usage === null}
		<div class="flex items-center justify-center py-6 text-xs text-[var(--text-3)]">
			No usage recorded
		</div>
	{:else}
		<div class="flex items-start gap-4">
			<div class="relative shrink-0" style="width:96px;height:96px">
				<svg
					data-slot="cache-ring"
					viewBox="0 0 120 120"
					width="96"
					height="96"
					role="img"
					aria-label={ariaLabel}
				>
					<circle cx="60" cy="60" r="50" fill="none" stroke="var(--surface-2)" stroke-width="10" />
					{#if cachePct !== null}
						<circle
							cx="60"
							cy="60"
							r="50"
							fill="none"
							stroke={ringColor}
							stroke-width="10"
							stroke-linecap="round"
							stroke-dasharray={CIRCUMFERENCE}
							stroke-dashoffset={strokeOffset}
							transform="rotate(-90 60 60)"
						/>
					{/if}
				</svg>
				<div class="pointer-events-none absolute inset-0 flex flex-col items-center justify-center">
					<div
						class={cn(
							'font-semibold leading-none',
							cachePct === null ? 'text-[var(--text-3)]' : 'text-[var(--text-1)]'
						)}
						style:font-size="var(--text-xl)"
					>
						{cachePct === null ? '—' : `${cachePct}%`}
					</div>
					<Tooltip
						text="Share of input-side tokens served from prompt cache. Higher is cheaper."
						slug="cache-efficiency"
					>
						<div class="mt-1 leading-none text-[var(--text-2)]" style:font-size="10px">
							hit rate
						</div>
					</Tooltip>
				</div>
			</div>

			<ul data-slot="cache-legend" class="flex min-w-0 flex-1 flex-col gap-1.5">
				{#each legendRows as row (row.label)}
					{@const help = LEGEND_HELP[row.label]}
					<li class="flex items-center gap-2 text-label">
						<span
							class="inline-block shrink-0 rounded-[2px]"
							style:width="5px"
							style:height="5px"
							style:background-color={row.color}
							aria-hidden="true"
						></span>
						<span class="flex-1 whitespace-pre-wrap break-words text-[var(--text-2)]">
							{#if help}
								<Tooltip text={help.text} slug={help.slug}>
									{row.label}
								</Tooltip>
							{:else}
								{row.label}
							{/if}
						</span>
						<span class="font-mono tabular-nums text-[var(--text-1)]">{formatCount(row.count)}</span
						>
						{#if row.showPct}
							<span class="w-8 text-right font-mono tabular-nums text-[var(--text-3)]"
								>{row.pct === null ? '—' : `${row.pct}%`}</span
							>
						{:else}
							<span class="w-8" aria-hidden="true"></span>
						{/if}
					</li>
				{/each}
			</ul>
		</div>
	{/if}
</div>
