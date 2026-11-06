<script lang="ts">
	interface Props {
		values: number[];
		stroke: string;
		kpi?: boolean;
	}

	const { values, stroke, kpi = false }: Props = $props();

	const points = $derived.by(() => {
		if (values.length === 0) return '';
		if (values.length === 1) return '0,10 60,10';
		const min = Math.min(...values);
		const max = Math.max(...values);
		const range = max - min || 1;
		return values
			.map((v, i) => {
				const x = ((i / (values.length - 1)) * 60).toFixed(2);
				const y = (20 - ((v - min) / range) * 20).toFixed(2);
				return `${x},${y}`;
			})
			.join(' ');
	});
</script>

{#if values.length > 0}
	<svg
		class="lp-sparkline {kpi ? 'lp-sparkline--kpi' : ''}"
		viewBox="0 0 60 20"
		aria-hidden="true"
		role="presentation"
	>
		<polyline {points} fill="none" {stroke} stroke-width="1.5" vector-effect="non-scaling-stroke" />
	</svg>
{/if}

<style>
	.lp-sparkline {
		width: 60px;
		height: 20px;
		flex-shrink: 0;
	}

	.lp-sparkline--kpi {
		width: 60px;
		height: 16px;
		margin-top: 4px;
	}
</style>
