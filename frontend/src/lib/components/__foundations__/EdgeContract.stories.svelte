<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { userEvent, expect } from 'storybook/test';

	const { Story } = defineMeta({
		title: 'Foundations/Edge Contract',
		tags: ['autodocs']
	});

	const SCALE = ['--edge-sm', '--edge-md', '--edge-lg'] as const;
</script>

<script lang="ts">
	import { onMount } from 'svelte';

	let edge = $state(22);

	let resolved = $state<Record<string, string>>({});

	onMount(() => {
		const probe = document.createElement('div');
		probe.setAttribute('data-design', 'funk');
		document.body.appendChild(probe);
		const cs = getComputedStyle(probe);
		resolved = Object.fromEntries(SCALE.map((t) => [t, cs.getPropertyValue(t).trim()]));
		probe.remove();
	});
</script>

<Story
	name="EdgeContract"
	play={async ({ canvas }) => {
		const readout = canvas.getByTestId('edge-readout');
		await expect(readout).toHaveTextContent('--edge: 22px');
		await userEvent.click(canvas.getByRole('button', { name: /grow the cut/i }));
		await expect(readout).toHaveTextContent('--edge: 34px');
		await userEvent.click(canvas.getByRole('button', { name: /grow the cut/i }));
		await expect(readout).toHaveTextContent('--edge: 22px');
	}}
>
	{#snippet template()}
		<div class="fk-plate" data-design="funk">
			<p class="fk-plate-title machine">Edge contract</p>

			<div class="fk-pair">
				<div>
					<div class="machine fk-verdict fk-verdict--bad">Violation</div>
					<div class="edge" style="--edge: {edge}px; padding: 4px 6px;">
						<div class="fk-line">
							Trace 0x91C — 412ms, 2,847 tokens written to buffer
							<span class="fk-chip fk-chip--loose">WARN</span>
						</div>
					</div>
					<p class="fk-cap">padding hardcoded — text falls inside the cut · translucent label</p>
				</div>

				<div>
					<div class="machine fk-verdict fk-verdict--good">Contract</div>
					<div class="edge" style="--edge: {edge}px;">
						<div class="fk-line">
							Trace 0x91C — 412ms, 2,847 tokens written to buffer
							<span class="fk-chip">WARN</span>
						</div>
					</div>
					<p class="fk-cap">pad = calc(edge + 4px) · opaque chip token</p>
				</div>
			</div>

			<div class="fk-controls">
				<button type="button" class="fk-btn" onclick={() => (edge = edge === 22 ? 34 : 22)}>
					Grow the cut
				</button>
				<span class="fk-readout" data-testid="edge-readout">
					--edge: {edge}px — both sides follow the same variable; only one keeps its content
				</span>
			</div>

			<div class="fk-scale">
				{#each SCALE as token (token)}
					<div class="edge fk-scale-cell" style="--edge: var({token});">
						<span class="machine fk-scale-lbl">{token} · {resolved[token] || '—'}</span>
					</div>
				{/each}
				<div
					class="edge edge-4 fk-scale-cell"
					style="--edge: var(--edge-md); --edge-line: var(--cyan-border);"
				>
					<span class="machine fk-scale-lbl fk-scale-lbl--cyan">edge-4</span>
				</div>
			</div>
		</div>
	{/snippet}
</Story>

<style>
	.fk-plate {
		display: flex;
		flex-direction: column;
		gap: var(--space-3);
		padding: var(--space-6);
		background: var(--bg);
		max-width: 860px;
	}
	.fk-plate-title {
		margin: 0 0 var(--space-2);
		font-size: var(--text-sm);
		color: var(--text-2);
	}
	.fk-pair {
		display: grid;
		grid-template-columns: 1fr 1fr;
		gap: var(--space-4);
		align-items: start;
	}
	.fk-verdict {
		margin-bottom: var(--space-2);
		font-size: var(--text-caption);
	}
	.fk-verdict--bad {
		color: var(--red);
	}
	.fk-verdict--good {
		color: var(--green);
	}
	.fk-line {
		font-family: var(--font-mono);
		font-size: var(--text-xs);
		line-height: var(--leading-body);
		color: var(--text-1);
	}
	.fk-chip {
		padding: 1px 6px;
		font-size: var(--text-caption);
		background: var(--chip-yellow);
		border: var(--hairline) solid var(--yellow-border);
		color: var(--yellow);
	}
	.fk-chip--loose {
		background: var(--yellow-dim);
		border: none;
	}
	.fk-controls {
		display: flex;
		align-items: center;
		gap: var(--space-3);
		margin-top: var(--space-2);
	}
	.fk-btn {
		display: inline-flex;
		align-items: center;
		height: var(--control-h);
		padding: 0 var(--space-3);
		font-family: var(--font-mono);
		font-size: var(--text-xs);
		color: var(--text-1);
		background: var(--surface-2);
		border: var(--hairline) solid var(--line);
		cursor: pointer;
		transition:
			background-color var(--dur-1) var(--ease-snap),
			border-color var(--dur-1) var(--ease-snap);
	}
	.fk-btn:hover {
		background: var(--surface-3);
		border-color: var(--line-strong);
	}
	.fk-btn:focus-visible {
		outline: 2px solid var(--focus);
		outline-offset: 2px;
	}
	.fk-readout {
		font-family: var(--font-mono);
		font-size: var(--text-caption);
		color: var(--text-3);
	}
	.fk-scale {
		display: flex;
		gap: var(--space-3);
		margin-top: var(--space-4);
	}
	.fk-scale-cell {
		flex: 1;
	}
	.fk-scale-lbl {
		font-size: var(--text-caption);
		color: var(--text-2);
	}
	.fk-scale-lbl--cyan {
		color: var(--cyan);
	}
	.fk-cap {
		margin: var(--space-2) 0 0;
		font-family: var(--font-mono);
		font-size: var(--text-caption);
		line-height: var(--leading-body);
		color: var(--text-3);
	}
</style>
