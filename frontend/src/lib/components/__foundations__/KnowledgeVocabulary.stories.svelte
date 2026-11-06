<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';

	const { Story } = defineMeta({
		title: 'Foundations/Knowledge Vocabulary',
		tags: ['autodocs']
	});

	interface Claim {
		text: string;
		provenance: 'derived' | 'operator';
		confidence: number | null;
		stage: 'raw' | 'daily' | 'long_term' | 'archived';
		sessions: number;
		projects: number;
		at: string;
	}

	function claims(): Claim[] {
		return [
			{
				text: 'A read probe is not a health check: SELECT 1 and pool acquire both pass against a full-disk Postgres while every write fails. Probe with an allocation.',
				provenance: 'operator',
				confidence: null,
				stage: 'long_term',
				sessions: 4,
				projects: 2,
				at: '2026-07-02'
			},
			{
				text: 'Sessions that end without a following prompt never close, so their final turn is never embedded. The conclusion of the work is the part that goes missing.',
				provenance: 'derived',
				confidence: 0.82,
				stage: 'daily',
				sessions: 3,
				projects: 1,
				at: '2026-08-14'
			},
			{
				text: 'Piping a gate through tail reports tail exit status and hides a failing gate.',
				provenance: 'derived',
				confidence: 0.41,
				stage: 'raw',
				sessions: 1,
				projects: 1,
				at: '2026-08-19'
			}
		];
	}

	const STAGE_LABEL: Record<Claim['stage'], string> = {
		raw: 'RAW',
		daily: 'DAILY',
		long_term: 'LONG TERM',
		archived: 'ARCHIVED'
	};

	interface Neighbour {
		title: string;
		score: number;
		distance: number | null;
		path: 'embedding' | 'fts';
	}

	function neighbours(): Neighbour[] {
		return [
			{
				title: 'Embedding pipeline audit — corpus written, never read',
				score: 0.68,
				distance: 0.68,
				path: 'embedding'
			},
			{ title: 'Stale-turn reaper design', score: 0.61, distance: 0.61, path: 'embedding' },
			{
				title: 'Recall endpoint returns prompt for an assistant-chunk match',
				score: 0.6,
				distance: 0.6,
				path: 'embedding'
			},
			{
				title: 'Consolidation ladder never receives raw claims',
				score: 0.34,
				distance: null,
				path: 'fts'
			}
		];
	}
</script>

<Story name="KnowledgeVocabulary">
	{#snippet template()}
		{@const rows = claims()}
		{@const near = neighbours()}
		{@const top = Math.max(...near.map((n) => n.score))}

		<div class="kv-plate" data-design="funk">
			<p class="machine kv-sec">Claim — provenance, maturity, recurrence</p>
			<p class="kv-note">
				The claim sentence is the primary object. The rail carries provenance and, for derived
				claims only, confidence. The ladder carries maturity. The count is the card's one
				display-face element and is suppressed below two.
			</p>

			<div class="kv-stack">
				{#each rows as claim (claim.text)}
					<article class="edge kv-claim">
						<span
							class="kv-rail"
							class:kv-rail--operator={claim.provenance === 'operator'}
							style="--conf: {claim.confidence ?? 1}"
							aria-hidden="true"
						></span>

						<div class="kv-body">
							<p class="kv-text">{claim.text}</p>
							<div class="kv-meta">
								<span class="kv-ladder" data-stage={claim.stage} aria-hidden="true">
									<span></span><span></span><span></span>
								</span>
								<span class="machine">{STAGE_LABEL[claim.stage]}</span>
								<span class="kv-sep" aria-hidden="true">·</span>
								<span class="machine">
									{#if claim.provenance === 'operator'}
										OPERATOR
									{:else}
										DERIVED · {claim.confidence?.toFixed(2)}
									{/if}
								</span>
								<span class="kv-sep" aria-hidden="true">·</span>
								<span class="kv-time">{claim.at}</span>
							</div>
						</div>

						{#if claim.sessions >= 2}
							<div class="kv-kpi">
								<span class="kv-kpi-n">{claim.sessions}</span>
								<span class="machine kv-kpi-l">
									{claim.projects >= 2 ? `sessions · ${claim.projects} projects` : 'sessions'}
								</span>
							</div>
						{/if}
					</article>
				{/each}
			</div>

			<p class="machine kv-sec">Maturity ladder</p>
			<p class="kv-note">
				Fills grow in count and brighten in value: --text-3, --text-2, --text-1. No signal colour —
				green means complete, and a long-term claim is not a finished task. Archived empties the
				ladder and dims the claim; it is never hidden.
			</p>
			<div class="kv-ladders">
				{#each ['raw', 'daily', 'long_term', 'archived'] as const as stage (stage)}
					<div class="kv-ladder-demo">
						<span class="kv-ladder" data-stage={stage} aria-hidden="true">
							<span></span><span></span><span></span>
						</span>
						<span class="machine kv-dim">{STAGE_LABEL[stage]}</span>
					</div>
				{/each}
			</div>

			<p class="machine kv-sec">Supersedes / refines — history, on a spine</p>
			<p class="kv-note">
				Current truth at the top at full weight; each prior claim beneath it, indented past a 1px
				spine, dimmed, ladder emptied. Reading top-down is reading backwards in time. Nothing is
				deleted.
			</p>
			<article class="edge kv-thread">
				<p class="kv-text">
					The capture DB is dialled directly over the LAN at 192.168.1.20:5440; a Windows
					portproxy rule pins the distro IP, so a WSL restart breaks the link.
				</p>
				<div class="kv-meta">
					<span class="kv-ladder" data-stage="long_term" aria-hidden="true">
						<span></span><span></span><span></span>
					</span>
					<span class="machine">LONG TERM</span>
					<span class="kv-sep" aria-hidden="true">·</span>
					<span class="kv-time">2026-08-20</span>
				</div>

				<div class="kv-history">
					<div class="kv-prior">
						<span class="machine kv-dim">Superseded 2026-08-20</span>
						<p class="kv-text kv-text--prior">
							The capture DB is reached through an SSH tunnel to omit; bulk pool timeouts mean
							the tunnel died and needs re-establishing.
						</p>
					</div>
					<div class="kv-prior">
						<span class="machine kv-dim">Refined from</span>
						<p class="kv-text kv-text--prior">
							The capture DB is remote and occasionally unreachable.
						</p>
					</div>
				</div>
			</article>

			<p class="machine kv-sec">Contradicts — an open question, not a fault</p>
			<p class="kv-note">
				Neither claim is subordinate, because the system has not decided. Yellow is FUNK's warning
				signal — unsafe to act on either until adjudicated — and it matches relationStatusTone()'s
				existing contested → warning mapping. Red would say something is broken; the system noticing
				a disagreement is it working.
			</p>
			<article class="edge edge-yellow kv-contra">
				<div class="kv-contra-head">
					<span class="badge-nx badge-warning">Contradicts</span>
					<span class="machine kv-dim">2 sessions · adjudication pending</span>
				</div>
				<div class="kv-compare">
					<div class="kv-cell">
						<p class="kv-text">
							Injection selects newest-first at project scope, so a long-term fact and a raw claim
							compete on recency alone.
						</p>
						<span class="machine kv-dim">Derived · 0.77 · 2026-08-11</span>
					</div>
					<div class="kv-cell">
						<p class="kv-text">
							Injection is stage-weighted; a long-term fact outranks a raw claim regardless of age.
						</p>
						<span class="machine kv-dim">Derived · 0.64 · 2026-08-18</span>
					</div>
				</div>
			</article>

			<p class="machine kv-sec">Similarity — rank, falloff, and the exact value</p>
			<p class="kv-note">
				Bars are normalised to the top hit, not to 1.0: cosine values cluster in a narrow band, so a
				0..1 scale renders every row the same length and shows nothing. The exact value sits in the
				meta line for anyone auditing. Where the lexical retriever answered there is no distance, so
				the bar is absent and the value is an em-dash — absence and zero are different.
			</p>
			<ul class="kv-near" role="list">
				{#each near as n (n.title)}
					<li class="kv-near-row">
						<span class="kv-near-title">{n.title}</span>
						<span class="kv-bar" aria-hidden="true">
							{#if n.distance !== null}
								<span class="kv-bar-fill" style="--fill: {n.score / top}"></span>
							{/if}
						</span>
						<span class="machine kv-dim kv-near-meta">
							{n.path === 'embedding' ? `semantic · ${n.distance?.toFixed(2)}` : 'lexical · —'}
						</span>
					</li>
				{/each}
			</ul>

			<p class="machine kv-sec">Fused result — the session is the answer, the turn is the proof</p>
			<p class="kv-note">
				One frame holds both levels, so the turns are visibly inside their session rather than
				beside it. Built on .mech-group: the group draws the frame, members draw only the seams. The
				top result shows its evidence without a click.
			</p>
			<div class="edge mech-group kv-fused">
				<div class="kv-session">
					<span class="kv-session-title">Embedding system audit — corpus written, never read</span>
					<span class="machine kv-dim kv-session-meta">temet-proxy · 2026-08-20 · 4 turns</span>
				</div>
				<div class="kv-turn">
					<span class="badge-nx badge-muted kv-turn-tag">assistant</span>
					<span class="kv-turn-text"
						>The single tool that queries the vector index has never been called: <mark
							class="kv-mark">recall_similar_turns</mark
						> shows 0 uses across 80,822 captured tool calls.</span
					>
				</div>
				<div class="kv-turn">
					<span class="badge-nx badge-muted kv-turn-tag">prompt</span>
					<span class="kv-turn-text"
						>check whether anything actually reads the <mark class="kv-mark">embeddings</mark> we generate</span
					>
				</div>
			</div>
		</div>
	{/snippet}
</Story>

<style>
	.kv-plate {
		display: flex;
		flex-direction: column;
		gap: var(--space-3);
		padding: var(--space-6);
		background: var(--surface-bg);
		font-family: var(--font-mono);
		max-width: 860px;
	}

	.kv-sec {
		margin-top: var(--space-5);
		font-size: var(--text-xs);
		color: var(--text-2);
	}
	.kv-sec:first-child {
		margin-top: 0;
	}
	.kv-note {
		margin: 0 0 var(--space-2);
		font-size: var(--text-sm);
		line-height: var(--leading-body);
		color: var(--text-3);
	}
	.kv-dim {
		color: var(--text-3);
	}
	.kv-sep {
		color: var(--text-3);
	}

	.kv-stack {
		display: flex;
		flex-direction: column;
		gap: var(--space-2);
	}
	.kv-claim {
		--edge: var(--edge-sm);
		display: grid;
		grid-template-columns: 2px minmax(0, 1fr) auto;
		gap: var(--space-3);
		align-items: start;
	}
	.kv-rail {
		align-self: stretch;
		min-height: var(--space-6);
		background: linear-gradient(
			to bottom,
			var(--text-3) calc(var(--conf, 1) * 100%),
			var(--line) 0
		);
	}
	.kv-rail--operator {
		background: var(--text-1);
	}
	.kv-body {
		min-width: 0;
	}
	.kv-text {
		margin: 0;
		font-size: var(--text-md);
		line-height: var(--leading-body);
		color: var(--text-1);
		white-space: pre-wrap;
		overflow-wrap: anywhere;
	}
	.kv-meta {
		display: flex;
		align-items: center;
		flex-wrap: wrap;
		gap: var(--space-2);
		margin-top: var(--space-2);
		font-size: var(--text-xs);
		color: var(--text-2);
	}
	.kv-time {
		font-size: var(--text-xs);
		color: var(--text-3);
	}

	.kv-ladder {
		display: inline-flex;
		gap: 2px;
		flex: none;
	}
	.kv-ladder > span {
		width: 8px;
		height: 8px;
		border: var(--hairline) solid var(--line);
		background: transparent;
		transition: background-color var(--dur-2) var(--ease-snap);
	}
	.kv-ladder[data-stage='raw'] > span:nth-child(1) {
		background: var(--text-3);
	}
	.kv-ladder[data-stage='daily'] > span:nth-child(-n + 2) {
		background: var(--text-2);
	}
	.kv-ladder[data-stage='long_term'] > span {
		background: var(--text-1);
	}
	.kv-ladders {
		display: flex;
		flex-wrap: wrap;
		gap: var(--space-5);
	}
	.kv-ladder-demo {
		display: flex;
		align-items: center;
		gap: var(--space-2);
		font-size: var(--text-xs);
	}

	.kv-kpi {
		display: flex;
		flex-direction: column;
		align-items: flex-end;
		flex: none;
		text-align: right;
	}
	.kv-kpi-n {
		font-family: var(--font-display);
		font-weight: var(--weight-display);
		font-size: var(--text-xl);
		line-height: var(--leading-tight);
		color: var(--text-1);
		font-variant-numeric: tabular-nums;
	}
	.kv-kpi-l {
		font-size: var(--text-xs);
		color: var(--text-3);
	}

	.kv-thread {
		--edge: var(--edge-sm);
	}
	.kv-history {
		display: flex;
		flex-direction: column;
		gap: var(--space-3);
		margin-top: var(--space-3);
		padding-left: var(--space-3);
		border-left: var(--hairline) solid var(--line);
	}
	.kv-prior {
		display: flex;
		flex-direction: column;
		gap: var(--space-1);
		font-size: var(--text-xs);
	}
	.kv-text--prior {
		font-size: var(--text-sm);
		color: var(--text-3);
	}

	.kv-contra {
		--edge: var(--edge-sm);
	}
	.kv-contra-head {
		display: flex;
		align-items: center;
		gap: var(--space-2);
		margin-bottom: var(--space-3);
		font-size: var(--text-xs);
	}
	.kv-compare {
		display: grid;
		grid-template-columns: 1fr 1fr;
		gap: var(--hairline);
		background: var(--line);
	}
	.kv-cell {
		display: flex;
		flex-direction: column;
		gap: var(--space-2);
		padding: var(--space-3);
		background: var(--surface-2);
		font-size: var(--text-xs);
	}

	.kv-near {
		display: flex;
		flex-direction: column;
		gap: var(--space-2);
		margin: 0;
		padding: 0;
		list-style: none;
	}
	.kv-near-row {
		display: grid;
		grid-template-columns: minmax(0, 1fr) 140px auto;
		align-items: center;
		gap: var(--space-3);
	}
	.kv-near-title {
		font-size: var(--text-sm);
		color: var(--text-1);
		overflow-wrap: anywhere;
	}
	.kv-bar {
		display: block;
		height: 2px;
		background: var(--surface-3);
	}
	.kv-bar-fill {
		display: block;
		height: 100%;
		width: calc(var(--fill, 0) * 100%);
		background: var(--line-strong);
	}
	.kv-near-meta {
		font-size: var(--text-xs);
		white-space: nowrap;
	}

	.kv-fused {
		--edge: var(--edge-sm);
	}
	.kv-session {
		display: flex;
		align-items: baseline;
		flex-wrap: wrap;
		gap: var(--space-2);
		padding: var(--space-3) var(--space-4);
	}
	.kv-session-title {
		font-size: var(--text-md);
		color: var(--text-1);
	}
	.kv-session-meta {
		margin-left: auto;
		font-size: var(--text-xs);
	}
	.kv-turn {
		display: flex;
		align-items: flex-start;
		gap: var(--space-2);
		padding: var(--space-3) var(--space-4) var(--space-3) var(--space-6);
		background: var(--surface-2);
	}
	.kv-turn-tag {
		flex: none;
		margin-top: 1px;
	}
	.kv-turn-text {
		min-width: 0;
		font-size: var(--text-sm);
		line-height: var(--leading-body);
		color: var(--text-2);
		white-space: pre-wrap;
		overflow-wrap: anywhere;
	}
	.kv-mark {
		background: var(--chip-cyan);
		color: var(--cyan);
		padding: 0 2px;
	}
</style>
