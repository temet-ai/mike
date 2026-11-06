<script lang="ts">
	import { formatRelative } from '$lib/time';

	export type ValidationState = 'pending' | 'pass' | 'correction' | null;

	interface Props {
		state: ValidationState;
		detail?: string | null;
		since?: string | null;
		now?: number;
	}

	let { state, detail = null, since = null, now = Date.now() }: Props = $props();
</script>

{#if state !== null}
	<div class="vsi-root" data-slot="validation-status-indicator" data-state={state}>
		{#if state === 'pending'}
			<div class="vsi-pending-line" aria-live="polite">
				<span class="vsi-dot" data-motion="wait wait-march">·</span><span
					class="vsi-dot"
					data-motion="wait wait-march">·</span
				><span class="vsi-dot" data-motion="wait wait-march">·</span>
				<span class="vsi-pending-text">this answer is being validated</span>
				{#if since}
					<span class="vsi-pending-since" title={since}>· {formatRelative(since, now)}</span>
				{/if}
			</div>
		{:else}
			<div
				class="vsi-badge"
				class:vsi-badge--pass={state === 'pass'}
				class:vsi-badge--correction={state === 'correction'}
			>
				<span class="vsi-badge-dot" aria-hidden="true"></span>
				<span class="vsi-badge-label">{state === 'pass' ? 'validated' : 'correction'}</span>
				{#if detail}
					<span class="vsi-badge-detail">{detail}</span>
				{/if}
			</div>
		{/if}
	</div>
{/if}

<style>
	.vsi-root {
		display: flex;
		align-items: center;
	}

	.vsi-pending-line {
		display: flex;
		align-items: center;
		gap: 2px;
		font-family: var(--font-mono, monospace);
		font-size: 10px;
		color: var(--status-warning-fg);
	}

	.vsi-pending-text {
		margin-left: 6px;
		color: var(--text-3);
	}

	.vsi-pending-since {
		color: var(--text-faint, var(--text-3));
	}

	.vsi-dot {
		opacity: 0.3;
	}

	.vsi-badge {
		display: inline-flex;
		align-items: center;
		gap: 6px;
		padding: 2px 7px;
		border-radius: 2px;
		border: 1px solid var(--border-default);
		background: var(--surface-2);
		font-family: var(--font-mono, monospace);
		font-size: 10px;
		color: var(--text-2);
	}

	.vsi-badge-dot {
		width: 7px;
		height: 7px;
		flex-shrink: 0;
		background: currentColor;
	}

	.vsi-badge-label {
		text-transform: uppercase;
		letter-spacing: 0.06em;
		font-weight: 600;
	}

	.vsi-badge-detail {
		color: var(--text-3);
		text-transform: none;
		font-weight: 400;
		letter-spacing: normal;
	}

	.vsi-badge--pass {
		color: var(--status-success-fg);
		background: var(--status-success-bg);
		border-color: var(--status-success-border);
	}

	.vsi-badge--correction {
		color: var(--status-danger-fg);
		background: var(--status-danger-bg);
		border-color: var(--status-danger-border);
	}
</style>
