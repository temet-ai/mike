<script lang="ts">
	interface Props {
		label: string;
		value: string | number;
		accentToken?: string;
	}

	let { label, value, accentToken = '--signal-active' }: Props = $props();
</script>

<div
	class="instrument instrument-tick instrument-readout"
	style="--instrument-accent: var({accentToken});"
>
	<span class="machine instrument-readout__label">{label}</span>
	<span class="instrument-readout__value">{value}</span>
</div>

<style>
	.instrument-readout {
		display: flex;
		flex-direction: column;
		gap: 2px;
		padding-left: var(--space-3, 12px);
	}
	.instrument-readout__label {
		font-size: 9px;
		color: var(--text-3);
	}
	.instrument-readout__value {
		font-family: var(--font-display);
		font-weight: 700;
		font-size: 20px;
		line-height: 1;
		color: var(--text-1);
	}
</style>
