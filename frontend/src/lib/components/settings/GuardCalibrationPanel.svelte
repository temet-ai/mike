<script lang="ts">
	import { onMount } from 'svelte';
	import { candidateRows, getCalibration } from '$lib/api/crossSession';
	import type { CalibrationResponse } from '$lib/api/crossSession';

	let data = $state<CalibrationResponse | null>(null);
	let loadError = $state<string | null>(null);

	onMount(() => {
		getCalibration()
			.then((d) => {
				data = d;
				loadError = null;
			})
			.catch((e: unknown) => {
				loadError = e instanceof Error ? e.message : String(e);
			});
	});
</script>

<section class="flex flex-col gap-2" data-testid="guard-calibration">
	<h2 class="text-sm font-semibold text-[var(--text-strong)]">Guard calibration</h2>
	{#if loadError}
		<p class="text-micro text-[var(--text-3)]" role="alert">
			Could not load calibration: {loadError}
		</p>
	{:else if data}
		<p class="text-micro text-[var(--text-3)]" data-calibration-modes>
			block {data.snapshot.settings.input_mode} · thresholds {data.snapshot.settings.threshold_mode} ·
			rollup
			{data.snapshot.rollup ? `#${data.snapshot.rollup.id}` : 'none yet'}
		</p>
		<ul class="flex flex-col gap-1">
			{#each candidateRows(data) as row, i (i)}
				<li
					class="text-micro text-[var(--text-2)]"
					data-calibration-row={`${row.question}:${row.scope_kind}`}
				>
					{row.question} ({row.scope_kind}): confirmed {row.k} of {row.n}{row.ci_low !== null &&
					row.ci_high !== null
						? ` (95% CI ${row.ci_low.toFixed(2)}-${row.ci_high.toFixed(2)})`
						: ''}
				</li>
			{/each}
		</ul>
		{#each data.snapshot.thresholds as t (t.id)}
			<p class="text-micro text-[var(--text-2)]" data-threshold-row={t.id}>
				{t.question}
				{t.scope}: threshold {t.applied_threshold} since {t.computed_at}
			</p>
		{/each}
		{#if data.block_if_live}
			<pre class="whitespace-pre-wrap text-micro text-[var(--text-2)]" data-calibration-block>{data
					.block_if_live.text}</pre>
		{/if}
	{/if}
</section>
