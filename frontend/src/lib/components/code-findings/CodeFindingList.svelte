<script lang="ts">
	import { resolve } from '$app/paths';
	import type { CodeFinding } from '$lib/api/codeFindings';
	import { StatusMark } from '$lib/components/ui/status-mark';
	import { formatRelative } from '$lib/time';
	import { groupFindings, outcomeMark, severityMark } from './codeFindingsFormat';

	interface Props {
		findings: CodeFinding[];
		sessionId: string;
		now: number;
	}

	let { findings, sessionId, now }: Props = $props();

	const groups = $derived(groupFindings(findings));

	function requestHref(finding: CodeFinding, requestId: number): string {
		const session = finding.session_id ?? sessionId;
		return resolve(
			`sessions/${encodeURIComponent(session)}/requests/${requestId}` as `sessions/${string}/requests/${string}`
		);
	}
</script>

<div class="cfl" data-slot="code-finding-list">
	{#each groups as group (group.key)}
		{@const severity = severityMark(group.severity)}
		<section
			class="cfl-group"
			data-slot="code-finding-group"
			aria-label="{group.rule_id} in {group.path}"
		>
			<div class="cfl-head">
				<div class="cfl-head-line">
					<span class="cfl-path">{group.path}</span>
					<span class="cfl-rule">
						<span class="cfl-rule-id">{group.rule_id}</span>
						<StatusMark icon={severity.icon} label={severity.label} tone={severity.tone} />
					</span>
				</div>
				<p class="cfl-message">{group.message}</p>
			</div>
			{#each group.findings as finding (finding.id)}
				{@const outcome = outcomeMark(finding.outcome)}
				<div class="cfl-row" data-slot="code-finding-row" data-motion="hover">
					<span class="cfl-pos">{finding.line}:{finding.col}</span>
					<div class="cfl-content">
						<pre class="cfl-code">{finding.matched}</pre>
						{#if finding.fix !== null}
							<div class="cfl-fix">
								<span class="machine cfl-fix-label">Fix</span>
								<pre class="cfl-code">{finding.fix}</pre>
							</div>
						{/if}
						<span class="cfl-delivery">
							{#if finding.delivered_via === 'request'}
								told {formatRelative(finding.delivered_at, now)}
							{:else if finding.delivered_via === 'push'}
								pushed {formatRelative(finding.delivered_at, now)}
							{:else}
								not told
							{/if}
							{#if finding.delivered_via !== null && finding.delivered_request_id !== null}
								<a class="cfl-link" href={requestHref(finding, finding.delivered_request_id)}
									>request #{finding.delivered_request_id}</a
								>
							{/if}
						</span>
					</div>
					<div class="cfl-outcome">
						<StatusMark icon={outcome.icon} label={outcome.label} tone={outcome.tone} />
						{#if finding.outcome !== 'open' && finding.outcome_at}
							<span class="cfl-outcome-at">{formatRelative(finding.outcome_at, now)}</span>
						{/if}
					</div>
				</div>
			{/each}
		</section>
	{/each}
</div>

<style>
	.cfl {
		display: flex;
		flex-direction: column;
	}
	.cfl-group {
		padding-block: var(--space-3);
		border-top: var(--hairline) solid var(--border-subtle);
	}
	.cfl-group:first-child {
		border-top: 0;
		padding-top: 0;
	}
	.cfl-head {
		display: flex;
		flex-direction: column;
		gap: var(--space-1);
		padding-bottom: var(--space-2);
	}
	.cfl-head-line {
		display: flex;
		flex-wrap: wrap;
		align-items: baseline;
		justify-content: space-between;
		gap: var(--space-2) var(--space-4);
	}
	.cfl-path {
		font-family: var(--font-mono, monospace);
		font-size: var(--text-sm);
		color: var(--text-1);
		overflow-wrap: anywhere;
		min-width: 0;
	}
	.cfl-rule {
		display: inline-flex;
		flex-wrap: wrap;
		align-items: center;
		gap: var(--space-3);
	}
	.cfl-rule-id {
		font-family: var(--font-mono, monospace);
		font-size: var(--text-sm);
		color: var(--text-2);
		overflow-wrap: anywhere;
	}
	.cfl-message {
		margin: 0;
		font-size: var(--text-sm);
		color: var(--text-2);
	}
	.cfl-row {
		display: grid;
		grid-template-columns: 8ch minmax(0, 1fr) minmax(15ch, max-content);
		gap: var(--space-3);
		padding-block: var(--space-2);
	}
	.cfl-row:hover {
		background: var(--surface-2);
	}
	.cfl-pos {
		font-family: var(--font-mono, monospace);
		font-size: var(--text-caption);
		font-variant-numeric: tabular-nums;
		color: var(--text-3);
	}
	.cfl-content {
		display: flex;
		flex-direction: column;
		gap: var(--space-2);
		min-width: 0;
	}
	.cfl-code {
		margin: 0;
		padding: var(--space-2);
		border-radius: var(--r-tol);
		background: var(--surface-2);
		font-family: var(--font-mono, monospace);
		font-size: var(--text-sm);
		color: var(--text-1);
		white-space: pre-wrap;
		overflow-wrap: anywhere;
	}
	.cfl-row:hover .cfl-code {
		background: var(--surface-3);
	}
	.cfl-fix {
		display: flex;
		flex-direction: column;
		gap: var(--space-1);
	}
	.cfl-fix-label {
		font-size: var(--text-caption);
		color: var(--text-3);
	}
	.cfl-delivery {
		display: inline-flex;
		flex-wrap: wrap;
		gap: var(--space-2);
		font-family: var(--font-mono, monospace);
		font-size: var(--text-caption);
		color: var(--text-3);
	}
	.cfl-link {
		color: var(--interactive-active-fg);
		text-decoration: none;
	}
	.cfl-link:hover {
		text-decoration: underline;
	}
	.cfl-link:focus-visible {
		outline: 2px solid var(--interactive-focus-ring);
		outline-offset: 2px;
	}
	.cfl-outcome {
		display: flex;
		flex-direction: column;
		gap: var(--space-1);
		align-items: flex-start;
	}
	.cfl-outcome-at {
		font-family: var(--font-mono, monospace);
		font-size: var(--text-caption);
		color: var(--text-3);
	}
</style>
