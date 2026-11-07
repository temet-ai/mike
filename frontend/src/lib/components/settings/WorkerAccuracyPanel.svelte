<script lang="ts">
	import type { EvalRateDto, EvalRollupDto } from '$lib/api/evalMetrics';
	import { formatShare } from '$lib/api/evalMetrics';

	let {
		role,
		rates,
		mix = [],
		rollup = null,
		error = null
	}: {
		role: 'turn_judge' | 'session_judge';
		rates: EvalRateDto[];
		mix?: EvalRateDto[];
		rollup?: EvalRollupDto | null;
		error?: string | null;
	} = $props();

	interface SliceRow {
		key: string;
		group: string;
		klass: string;
		tier: string;
		split: string;
		source: string;
		precision: EvalRateDto | null;
		recall: EvalRateDto | null;
	}

	function group(rate: EvalRateDto): string {
		if (rate.dimension === 'writer_role') {
			return rate.dimension_value === 'abort_path'
				? 'stream aborted (no judge)'
				: `writer: ${rate.dimension_value ?? 'none'}`;
		}
		if (rate.bundle_label?.startsWith('legacy:')) {
			const [, model, rubric] = rate.bundle_label.split(':');
			return `${model} · rubric ${rubric}`;
		}
		return rate.bundle_label ?? 'all writers';
	}

	let rows = $derived.by(() => {
		const byKey = new Map<string, SliceRow>();
		for (const rate of rates.filter((r) => r.role === role && r.window_days === null)) {
			const key = [
				group(rate),
				rate.class,
				rate.label_tier,
				rate.label_split,
				rate.label_source
			].join('|');
			const row = byKey.get(key) ?? {
				key,
				group: group(rate),
				klass: rate.class,
				tier: rate.label_tier ?? '—',
				split: rate.label_split ?? '—',
				source: rate.label_source ?? '—',
				precision: null,
				recall: null
			};
			if (rate.metric === 'precision') row.precision = rate;
			if (rate.metric === 'recall') row.recall = rate;
			byKey.set(key, row);
		}
		return [...byKey.values()];
	});

	let current = $derived(mix.filter((r) => r.window_days === 7 && r.population === 'real_turns'));
	let band = $derived(mix.find((r) => r.population === 'replied_real_turns') ?? null);

	function baselineOf(klass: string): EvalRateDto | null {
		return mix.find((r) => r.window_days === 28 && r.class === klass) ?? null;
	}

	function cell(rate: EvalRateDto | null): string {
		if (!rate) return '—';
		const interval =
			rate.ci_low === null ? '' : ` [${formatShare(rate.ci_low)}, ${formatShare(rate.ci_high)}]`;
		const size = rate.weighted
			? `weighted, n_eff ≈ ${rate.n_effective?.toFixed(1) ?? '—'}`
			: `${rate.k}/${rate.n}`;
		const excluded = Number(rate.detail?.excluded_null_weight ?? 0);
		const note = excluded > 0 ? `, ${excluded} without weight excluded` : '';
		return `${formatShare(rate.value)} (${size}${note})${interval}`;
	}
</script>

<section class="accuracy" aria-label="Judge accuracy">
	{#if error}
		<p class="error" role="alert">Could not load accuracy: {error}</p>
	{:else if rows.length === 0}
		<p class="empty">
			No labelled turns for this judge yet. Labels come from the gold control on turns (operator),
			relayed corrections, and imported seed sets.
		</p>
	{:else}
		<table>
			<thead>
				<tr>
					<th>Writer or bundle</th>
					<th>Class</th>
					<th>Tier</th>
					<th>Split</th>
					<th>Source</th>
					<th>Precision</th>
					<th>Recall</th>
				</tr>
			</thead>
			<tbody>
				{#each rows as row (row.key)}
					<tr>
						<td>{row.group}</td>
						<td>{row.klass}</td>
						<td>{row.tier}</td>
						<td>{row.split}</td>
						<td>{row.source}</td>
						<td>{cell(row.precision)}</td>
						<td>{cell(row.recall)}</td>
					</tr>
				{/each}
			</tbody>
		</table>
	{/if}
	{#if role === 'turn_judge' && current.length > 0}
		<div class="mix" aria-label="Label mix">
			<h4>Stored label mix, last 7 days against the 28 days before</h4>
			{#each current as rate (rate.id)}
				<div class="mix-row">
					<span class="mix-class">{rate.class}</span>
					<span class="bar" style:width="{Math.round((rate.value ?? 0) * 100)}%"></span>
					<span
						>{formatShare(rate.value)} ({rate.k}/{rate.n}) against {formatShare(
							baselineOf(rate.class)?.value ?? null
						)}</span
					>
				</div>
			{/each}
			{#if band}
				<p class="band">
					Corrected on replied real turns: {formatShare(band.value)} ({band.k}/{band.n}), against
					the operator's 32-49% band.
				</p>
			{/if}
		</div>
	{/if}
	{#if rollup}<p class="rollup">Rolled up {rollup.for_day} ({rollup.sql_version})</p>{/if}
</section>

<style>
	.accuracy {
		display: flex;
		flex-direction: column;
		gap: var(--space-2);
		font-size: var(--text-xs);
	}
	table {
		border-collapse: collapse;
		width: 100%;
		font-family: var(--font-machine);
	}
	th,
	td {
		border: 1px solid var(--border-default);
		padding: var(--space-2);
		text-align: left;
		color: var(--text-2);
	}
	th {
		color: var(--text-strong);
	}
	.mix-row {
		display: grid;
		grid-template-columns: 8rem 1fr auto;
		gap: var(--space-2);
		align-items: center;
	}
	.bar {
		display: block;
		height: 0.5rem;
		background: var(--text-2);
	}
	h4 {
		margin: 0;
		color: var(--text-strong);
	}
</style>
