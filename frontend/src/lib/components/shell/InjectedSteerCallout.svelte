<script lang="ts">
	import { buildSteeringBlock } from '$lib/steering';

	interface Props {
		payloadText: string;
	}

	let { payloadText }: Props = $props();

	const envelope = $derived(buildSteeringBlock(payloadText));
</script>

<div class="isc-steer" data-slot="injected-steer-callout">
	<p class="isc-steer__nudge-line">
		<span class="isc-steer__nudge-tag">Authored nudge</span>
		<span class="isc-steer__nudge-text">{payloadText}</span>
	</p>
	<pre class="isc-steer__pre">{envelope}</pre>
</div>

<style>
	.isc-steer {
		display: flex;
		flex-direction: column;
		gap: 8px;
		padding: 10px;
		border: 1px solid var(--status-steering-border);
		border-radius: 2px;
		background: var(--status-steering-bg);
	}

	.isc-steer__nudge-line {
		margin: 0;
		display: flex;
		flex-wrap: wrap;
		align-items: baseline;
		gap: 6px;
		font-family: var(--font-mono, monospace);
	}

	.isc-steer__nudge-tag {
		font-size: 10px;
		font-weight: 600;
		text-transform: uppercase;
		letter-spacing: 0.06em;
		color: var(--status-steering-fg);
		flex-shrink: 0;
	}

	.isc-steer__nudge-text {
		font-size: 13px;
		font-weight: 600;
		color: var(--status-steering-fg);
		white-space: pre-wrap;
		overflow-wrap: break-word;
		word-break: break-word;
	}

	.isc-steer__pre {
		margin: 0;
		max-height: 300px;
		overflow-y: auto;
		padding: 10px;
		border: 1px solid var(--border-subtle);
		border-radius: 2px;
		background: var(--surface-2);
		font-family: var(--font-mono, monospace);
		font-size: 12px;
		color: var(--text-2);
		line-height: 1.5;
		white-space: pre-wrap;
		overflow-wrap: break-word;
		word-break: break-word;
	}
</style>
