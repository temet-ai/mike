<script lang="ts">
	import Tooltip from '$lib/components/shared/Tooltip.svelte';

	interface Props {
		registered: number;
		observed: number;
		unregistered: number;
		drifted: number;
		windowLabel: string;
	}

	let { registered, observed, unregistered, drifted, windowLabel }: Props = $props();

	interface KpiCell {
		label: string;
		value: number;
		help: string;
		accent: 'none' | 'green' | 'amber' | 'red';
	}

	const cells = $derived<KpiCell[]>([
		{
			label: 'Registered',
			value: registered,
			help: 'Classes with a registered composition — counts whether or not they had traffic in the selected window.',
			accent: registered > 0 ? 'green' : 'none'
		},
		{
			label: `Shapes observed (${windowLabel})`,
			value: observed,
			help: 'Distinct (lane, persona) classes with at least one request in the selected window.',
			accent: 'none'
		},
		{
			label: `Unregistered (${windowLabel})`,
			value: unregistered,
			help: 'Classes observed in the selected window with no matching registered composition.',
			accent: unregistered > 0 ? 'amber' : 'green'
		},
		{
			label: `Drifted (${windowLabel})`,
			value: drifted,
			help: 'Registered classes whose most recent in-window observation no longer matches the stored composition.',
			accent: drifted > 0 ? 'red' : 'green'
		}
	]);
</script>

<div
	class="grid grid-cols-1 gap-4 bg-[var(--surface-card)] p-4 sm:grid-cols-2 lg:grid-cols-4"
	data-slot="prompt-kpi-strip"
>
	{#each cells as cell, i (cell.label)}
		<div
			class="kpi-cell rounded-md border border-[var(--border-default)] bg-[var(--surface-card)] p-[18px]"
			class:kpi-green={cell.accent === 'green'}
			class:kpi-amber={cell.accent === 'amber'}
			class:kpi-red={cell.accent === 'red'}
			style="animation-delay: {i * 0.05}s"
		>
			<p class="text-label uppercase tracking-kpi text-[var(--text-3)]">
				<Tooltip text={cell.help}>{cell.label}</Tooltip>
			</p>
			<p class="mt-2 text-3xl text-[var(--text-strong)] kpi-value">
				{cell.value.toLocaleString('en-US')}
			</p>
		</div>
	{/each}
</div>

<style>
	.kpi-cell {
		box-shadow: inset 0 1px 0 var(--surface-shine);
		animation: rise 0.4s ease both;
	}
	.kpi-value {
		font-family: var(--font-display, 'Syne Variable', sans-serif);
		font-weight: 700;
		letter-spacing: -0.02em;
	}
	.kpi-green .kpi-value {
		color: var(--status-success-fg);
	}
	.kpi-green {
		border-color: var(--status-success-border);
	}
	.kpi-amber .kpi-value {
		color: var(--status-warning-fg);
	}
	.kpi-amber {
		border-color: var(--status-warning-border);
	}
	.kpi-red .kpi-value {
		color: var(--status-danger-fg);
	}
	.kpi-red {
		border-color: var(--status-danger-border);
	}

	@keyframes rise {
		from {
			opacity: 0;
			transform: translateY(6px);
		}
		to {
			opacity: 1;
			transform: translateY(0);
		}
	}
</style>
