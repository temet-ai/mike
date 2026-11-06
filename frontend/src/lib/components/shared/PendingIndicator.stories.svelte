<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import StatusPill from './StatusPill.svelte';

	const { Story } = defineMeta({
		title: 'Shared/PendingIndicator',
		component: StatusPill,
		tags: ['autodocs'],
		parameters: { layout: 'padded' }
	});
</script>

<Story name="PendingIndicator">
	{#snippet template()}
		<div class="pi-stage">
			<div class="pi-concept-header">
				PendingIndicator — arc family, pending state
				<span class="pi-concept-sub">
					1 dashed ring · 4s burst / 20s hold — vs streaming's solid arcs / 2s burst / 10s hold
				</span>
			</div>

			<div class="pi-section-label">Inline (status-row context, 16px)</div>
			<div class="pi-band-mockup">
				<span class="pi-band-toggle">#41 · Turn 2</span>
				<span class="pi-band-label">claude-opus-4</span>

				<span class="pi-pill" role="status" aria-label="Request pending — awaiting response">
					<span class="pi-arc" aria-hidden="true"></span>
					<span class="pi-label">wait</span>
				</span>

				<span class="pi-band-time">0s ago</span>
			</div>

			<div class="pi-section-label" style="margin-top: 20px;">
				Side-by-side: pending (left) vs streaming (right) — must be distinguishable at a glance
			</div>
			<div class="pi-compare-row">
				<span class="pi-pill" role="status" aria-label="Pending">
					<span class="pi-arc" aria-hidden="true"></span>
					<span class="pi-label">wait</span>
				</span>

				<span class="pi-streaming-pill" role="status" aria-label="Streaming">
					<span class="pi-streaming-orbit">
						<span class="pi-s-arc pi-s-arc--outer" aria-hidden="true"></span>
						<span class="pi-s-arc pi-s-arc--mid" aria-hidden="true"></span>
						<span class="pi-s-arc pi-s-arc--inner" aria-hidden="true"></span>
					</span>
					<span class="pi-label">recv</span>
				</span>
			</div>

			<div class="pi-note">
				Pending: 1 dashed ring, 4s burst / 20s hold — "waiting quietly, nothing flowing yet".<br />
				Streaming: solid arcs, 2s burst / 10s hold — "receiving, data arriving in chunks".<br />
				Dashed border matches status-pill-pending convention (app.css:1278).<br />
				Keyframe audit: rotate() only — zero opacity, zero scale, zero position changes.<br />
				No rotate3d() — avoids flicker from rings going edge-on at 16px.
			</div>
		</div>
	{/snippet}
</Story>

<style>
	.pi-stage {
		padding: 20px;
		background: var(--surface-bg);
		min-width: 400px;
	}
	.pi-concept-header {
		font-family: 'JetBrains Mono', monospace;
		font-size: 0.75rem;
		font-weight: 600;
		color: var(--text-1);
		margin-bottom: 20px;
		letter-spacing: 0.02em;
	}
	.pi-concept-sub {
		display: block;
		font-weight: 400;
		color: var(--text-3);
		margin-top: 3px;
		font-size: 0.6875rem;
	}
	.pi-section-label {
		font-family: 'JetBrains Mono', monospace;
		font-size: 0.625rem;
		color: var(--text-3);
		margin-bottom: 8px;
		letter-spacing: 0.04em;
		text-transform: uppercase;
	}
	.pi-band-mockup {
		display: flex;
		align-items: center;
		gap: 8px;
		padding: 6px 10px;
		background: var(--surface-card);
		border: 1px solid var(--border-default);
		border-radius: 4px;
	}
	.pi-band-toggle {
		font-family: 'JetBrains Mono', monospace;
		font-size: 0.6875rem;
		color: var(--text-2);
		font-weight: 500;
	}
	.pi-band-label {
		font-family: 'JetBrains Mono', monospace;
		font-size: 0.625rem;
		color: var(--text-3);
		flex: 1;
	}
	.pi-band-time {
		font-family: 'JetBrains Mono', monospace;
		font-size: 0.625rem;
		color: var(--text-3);
	}
	.pi-compare-row {
		display: flex;
		align-items: center;
		gap: 16px;
		padding: 10px;
		background: var(--surface-card);
		border: 1px solid var(--border-default);
		border-radius: 4px;
	}
	.pi-note {
		font-family: 'JetBrains Mono', monospace;
		font-size: 0.625rem;
		color: var(--text-3);
		margin-top: 16px;
		line-height: 1.7;
	}

	@keyframes pi-spin {
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

	.pi-pill {
		display: inline-flex;
		align-items: center;
		gap: 6px;
		padding: 2px 7px;
		border-radius: 4px;
		background: var(--status-warning-bg);
		border: 1px dashed var(--status-warning-border);
	}
	.pi-label {
		font-family: 'JetBrains Mono', monospace;
		font-size: 0.6875rem;
		font-weight: 500;
		color: var(--text-3);
		letter-spacing: 0.02em;
	}

	.pi-arc {
		display: block;
		width: 16px;
		height: 16px;
		border-radius: 50%;
		border: 1.5px dashed var(--status-warning-fg);
		flex-shrink: 0;
	}

	.pi-arc {
		animation: pi-spin 24s linear 0s infinite;
	}

	@keyframes pi-s-burst {
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

	.pi-streaming-pill {
		display: inline-flex;
		align-items: center;
		gap: 6px;
		padding: 2px 7px;
		border-radius: 4px;
		background: var(--status-warning-bg);
		border: 1px solid var(--status-warning-border);
	}
	.pi-streaming-orbit {
		position: relative;
		display: flex;
		align-items: center;
		justify-content: center;
		width: 16px;
		height: 16px;
		flex-shrink: 0;
	}
	.pi-s-arc {
		position: absolute;
		border-radius: 50%;
	}
	.pi-s-arc--outer {
		width: 16px;
		height: 16px;
		border: 1.5px solid var(--status-warning-fg);
		border-top-color: transparent;
	}
	.pi-s-arc--mid {
		width: 10px;
		height: 10px;
		border: 1px solid var(--status-warning-border);
		border-bottom-color: transparent;
	}
	.pi-s-arc--inner {
		width: 5px;
		height: 5px;
		border: 1px solid var(--border-subtle);
		border-right-color: transparent;
	}

	.pi-s-arc--outer {
		animation: pi-s-burst 12s linear 0s infinite;
	}
	.pi-s-arc--mid {
		animation: pi-s-burst 12s linear 0.4s infinite;
	}
	.pi-s-arc--inner {
		animation: pi-s-burst 12s linear 0.8s infinite;
	}
</style>
