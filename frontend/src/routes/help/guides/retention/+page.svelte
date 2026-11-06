<script lang="ts">
	import GuideLayout from '$lib/components/help/GuideLayout.svelte';
	import { resolve } from '$app/paths';

	const toc = [
		{ slug: 'why-it-exists', label: 'Why retention exists' },
		{ slug: 'global-default', label: 'Global default' },
		{ slug: 'per-project', label: 'Per-project override' },
		{ slug: 'per-session', label: 'Per-session override' },
		{ slug: 'pinning', label: 'Pinning' },
		{ slug: 'precedence', label: 'Precedence summary' }
	];
</script>

<GuideLayout
	title="Keep the data you care about (TTL & pinning)"
	summary="Capture is unbounded by default; eviction is what keeps the working set finite. TTL is configurable at three levels (global, project, session). Pinning is the override — a pinned session is exempt from eviction entirely."
	audience="operator"
	duration="3 min"
	{toc}
>
	<h2 id="why-it-exists">Why retention exists</h2>
	<p>
		Every captured <a href={resolve('/help/glossary#request')}>request</a> stores the full request
		body, the full response (or accumulated SSE stream), and all the metadata around it. Without
		<a href={resolve('/help/glossary#ttl')}>TTL</a>, the database grows indefinitely. With TTL, old
		<a href={resolve('/help/glossary#session')}>sessions</a> become eligible for eviction the next time
		the sweep runs, and the working set stays useful.
	</p>

	<h2 id="global-default">Global default</h2>
	<p>
		Sessions are <strong>not deleted by default</strong>. Retention is controlled by two boot-time
		settings, not by
		<a href={resolve('/settings')}>/settings</a> — they're read once when the process starts, the same
		as the listen ports or the database URL, so there is nothing for a live settings page to hot-reload.
	</p>
	<div class="guide-table-frame" data-augmented-ui="tl-clip br-clip border">
		<table class="guide-table">
			<thead>
				<tr>
					<th>Env var (CLI flag)</th>
					<th>What it does</th>
				</tr>
			</thead>
			<tbody>
				<tr>
					<td><code>MIKE_SWEEP_AT_STARTUP</code> (<code>--sweep-at-startup</code>)</td>
					<td
						>Opts in to deleting expired sessions. Defaults to <code>false</code> — off means nothing
						is ever deleted.</td
					>
				</tr>
				<tr>
					<td><code>MIKE_DEFAULT_TTL</code> (<code>--default-ttl</code>)</td>
					<td
						>How old an unpinned, unoverridden session has to be before the sweep deletes it.
						Defaults to <code>"365d"</code>.</td
					>
				</tr>
			</tbody>
		</table>
	</div>
	<p>
		Set them as environment variables or CLI flags before starting the proxy, then confirm what
		actually took effect with <code>mike print-config</code>. When enabled, the sweep runs
		<strong>once per process start</strong> — after crash recovery, before binding the listeners. There
		is no continuous timer; restart to re-run it.
	</p>
	<div class="callout" data-augmented-ui="tl-clip br-clip border">
		<span class="callout-title">A second TTL knob feeds the pin/TTL API</span>
		<span>
			<code>default_ttl</code> (a humantime string like <code>"365d"</code>) is what the startup
			sweep itself uses. A separate boot value,
			<code>default_ttl_secs</code>, feeds the "effective TTL" the per-session and per-project TTL
			API report back to you. The two are independent settings — keep them consistent or the API's
			reported effective TTL and the sweep's actual behavior will disagree.
		</span>
	</div>

	<h2 id="per-project">Per-project override</h2>
	<p>
		<a href={resolve('/help/glossary#project')}>Projects</a> can override the global default, and
		the API for it already exists —
		<code>PATCH /api/projects/&#123;path&#125;/ttl</code> — but there is no control for it in the SPA
		yet. Set it directly:
	</p>
	<p>
		Use the actual inspect address from <a href={resolve('/help/guides/getting-started')}
			>Getting started</a
		>
		and your URL-encoded project path. Send <code>&#123;"ttl_secs": 604800&#125;</code>
		as the JSON body to keep seven days. Confirm the target project before changing retention.
	</p>
	<p>
		Setting <code>ttl_secs</code> to <code>null</code> clears the override (the project inherits the global
		default again).
	</p>

	<h2 id="per-session">Per-session override</h2>
	<p>
		Same pattern at session granularity — <code>PATCH /api/sessions/&#123;id&#125;/ttl</code>, same
		<code>&#123;"ttl_secs": &lt;seconds&gt; | null&#125;</code> body, also not yet exposed as a control
		in the SPA. A per-session override beats the project override beats the global default.
	</p>

	<h2 id="pinning">Pinning</h2>
	<p>
		<a href={resolve('/help/glossary#project-pin')}>Pinning</a> is the escape hatch: pinned sessions
		(and every session under a pinned project) are
		<strong>exempt from TTL eviction entirely</strong>, regardless of any TTL setting. They stay
		until you explicitly unpin or delete them.
	</p>
	<p>
		Use pinning for the project you're actively investigating — an opted-in sweep won't silently
		delete data you're mid-analysis on. Click the pin icon next to a session or project name to
		toggle.
	</p>
	<div class="callout" data-augmented-ui="tl-clip br-clip border">
		<span class="callout-title">Pin first, query second</span>
		<span>
			If you're about to spend an hour digging through a session, pin it before you start. There's
			no recovery from an evicted session — the capture is gone.
		</span>
	</div>

	<h2 id="precedence">Precedence summary</h2>
	<ol>
		<li>
			<strong>Pinned</strong> (session or its project) → never evicts.
		</li>
		<li>
			<strong>Per-session TTL</strong> override → wins over project + global.
		</li>
		<li>
			<strong>Per-project TTL</strong> override → wins over global.
		</li>
		<li>
			<strong>Global default</strong> → applies when nothing above it does.
		</li>
	</ol>
</GuideLayout>

<style>
	.guide-table-frame {
		--edge: 8px;
		--aug-tl: var(--edge);
		--aug-br: var(--edge);
		--aug-border-all: 1px;
		--aug-border-bg: linear-gradient(
			135deg,
			var(--status-warning-fg) 0%,
			var(--status-warning-border) 45%,
			var(--border-default) 80%
		);
		padding: calc(var(--edge) + 4px);
		background: var(--surface-card);
	}
	.guide-table {
		width: 100%;
		table-layout: fixed;
		border-collapse: collapse;
		font-size: 0.8125rem;
		color: var(--text-1);
	}
	.guide-table th {
		text-align: left;
		padding: 7px 12px;
		font-size: 0.6875rem;
		font-weight: 600;
		text-transform: uppercase;
		letter-spacing: 0.05em;
		color: var(--text-3);
		background: var(--surface-2);
		border-bottom: 1px solid var(--border-subtle);
	}
	.guide-table td {
		padding: 9px 12px;
		border-bottom: 1px solid var(--border-subtle);
		line-height: 1.55;
		vertical-align: top;
	}
	.guide-table td:first-child {
		width: 40%;
		overflow-wrap: break-word;
		color: var(--text-strong);
	}
	.guide-table tr:last-child td {
		border-bottom: none;
	}
</style>
