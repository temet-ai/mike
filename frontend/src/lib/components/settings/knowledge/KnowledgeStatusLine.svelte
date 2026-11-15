<script lang="ts">
	interface Props {
		text: string;
		failed?: boolean;
	}

	const { text, failed = false }: Props = $props();
</script>

<p class="kk-status" class:kk-status--failed={failed} role="status" aria-live="polite">{text}</p>

<style>
	.kk-status {
		margin: 0;
		min-block-size: 1.55em;
		padding: var(--space-1) calc(var(--edge-md) + var(--space-2));
		font-family: var(--font-machine);
		font-size: var(--text-xs);
		color: var(--text-3);
		overflow-wrap: anywhere;
	}
	.kk-status--failed {
		color: var(--status-danger-fg);
	}
</style>
