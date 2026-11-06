<script lang="ts">
	let { label = 'Response streaming' }: { label?: string } = $props();
</script>

<span class="si-orbit" role="status" aria-label={label}>
	<span class="si-ring si-ring--outer" aria-hidden="true"></span>
	<span class="si-ring si-ring--mid" aria-hidden="true"></span>
	<span class="si-ring si-ring--inner" aria-hidden="true"></span>
</span>

<style>
	@keyframes si-spin-outer {
		0% {
			transform: rotate(0deg);
		}
		16.7% {
			transform: rotate(360deg);
		}
		100% {
			transform: rotate(360deg);
		}
	}

	@keyframes si-spin-mid {
		0% {
			transform: rotate(0deg);
		}
		16.7% {
			transform: rotate(-540deg);
		}
		100% {
			transform: rotate(-540deg);
		}
	}

	@keyframes si-spin-inner {
		0% {
			transform: rotate(0deg);
		}
		16.7% {
			transform: rotate(720deg);
		}
		100% {
			transform: rotate(720deg);
		}
	}

	.si-orbit {
		position: relative;
		display: inline-flex;
		align-items: center;
		justify-content: center;
		width: 16px;
		height: 16px;
		flex-shrink: 0;
	}

	.si-ring {
		position: absolute;
		border-radius: 50%;
		border-style: solid;
		border-width: 1px;
		will-change: transform;
	}

	.si-ring--outer {
		width: 16px;
		height: 16px;
		border-color: var(--status-warning-fg) var(--status-warning-fg) transparent
			var(--status-warning-fg);
	}
	.si-ring--mid {
		width: 11px;
		height: 11px;
		border-color: var(--status-warning-border) var(--status-warning-border) transparent
			var(--status-warning-border);
	}
	.si-ring--inner {
		width: 6px;
		height: 6px;
		border-color: var(--border-subtle) var(--border-subtle) transparent var(--border-subtle);
	}

	.si-ring--outer {
		animation: si-spin-outer 12s linear 0s infinite;
	}
	.si-ring--mid {
		animation: si-spin-mid 12s linear 0.1s infinite;
	}
	.si-ring--inner {
		animation: si-spin-inner 12s linear 0.2s infinite;
	}
</style>
