<script lang="ts">
	import type { CodexIntegrationStatus, CodexRateLimitWindow } from '$lib/api/integrations';

	interface Props {
		status: CodexIntegrationStatus | null;
		error: string | null;
		unavailable: boolean;
		onRetry?: () => void;
	}
	let { status, error, unavailable, onRetry }: Props = $props();

	const compact = new Intl.NumberFormat(undefined, {
		notation: 'compact',
		maximumFractionDigits: 1
	});
	function windowLabel(window: CodexRateLimitWindow | null | undefined): string {
		if (!window) return 'Not reported';
		const remaining = Math.max(0, 100 - window.usedPercent);
		return `${remaining.toFixed(0)}% remaining`;
	}
	function resetLabel(window: CodexRateLimitWindow | null | undefined): string {
		if (!window?.resetsAt) return '';
		return `Resets ${new Date(window.resetsAt * 1000).toLocaleString()}`;
	}
</script>

<section id="codex" class="cip" aria-labelledby="cip-title">
	<header>
		<div>
			<h2 id="cip-title">Codex</h2>
			<p>Uses the Codex CLI's existing ChatGPT login. Mike never reads or stores its tokens.</p>
		</div>
		{#if status?.selected}<span class="provider-badge">Selected</span>{/if}
	</header>
	{#if unavailable}
		<p class="message">This view needs a newer prompt-assistant service.</p>
	{:else if error}
		<div class="error" role="alert">
			<span>Could not load Codex usage: {error}</span>{#if onRetry}<button
					onclick={() => onRetry?.()}>Retry</button
				>{/if}
		</div>
	{:else if status === null}
		<p class="message" aria-busy="true">Loading Codex account…</p>
	{:else if !status.installed}
		<p class="message">Codex CLI is not installed on this machine.</p>
	{:else if !status.logged_in}
		<p class="message">Codex is not signed in. Run <code>codex login</code> on this machine.</p>
	{:else}
		<div class="account">
			<div>
				<span class="eyebrow">Account</span><strong>{status.email ?? 'ChatGPT account'}</strong
				><small>{status.plan_type ?? status.account_type ?? 'ChatGPT'}</small>
			</div>
			<div>
				<span class="eyebrow">5-hour window</span><strong
					>{windowLabel(status.rate_limits?.rateLimits?.primary)}</strong
				><small>{resetLabel(status.rate_limits?.rateLimits?.primary)}</small>
			</div>
			<div>
				<span class="eyebrow">Weekly window</span><strong
					>{windowLabel(status.rate_limits?.rateLimits?.secondary)}</strong
				><small>{resetLabel(status.rate_limits?.rateLimits?.secondary)}</small>
			</div>
			<div>
				<span class="eyebrow">Lifetime tokens</span><strong
					>{status.usage?.summary?.lifetimeTokens == null
						? '—'
						: compact.format(status.usage.summary.lifetimeTokens)}</strong
				><small>Codex activity</small>
			</div>
		</div>
		{#if status.error}<p class="warning">
				Some usage details were unavailable: {status.error}
			</p>{/if}
	{/if}
</section>

<style>
	.cip {
		padding: 22px;
		border: 1px solid var(--border-subtle);
		background: var(--surface-card);
	}
	header {
		display: flex;
		justify-content: space-between;
		gap: 18px;
		align-items: flex-start;
	}
	h2 {
		margin: 0;
		color: var(--text-strong);
		font: 800 16px var(--font-display);
	}
	header p,
	.message,
	.warning {
		margin: 6px 0 0;
		color: var(--text-2);
		font-size: 12px;
		line-height: 1.5;
	}
	.provider-badge {
		padding: 4px 8px;
		color: var(--status-success-fg);
		border: 1px solid var(--status-success-border);
		font-size: 10px;
		font-weight: 800;
		text-transform: uppercase;
		letter-spacing: 0.08em;
	}
	.account {
		display: grid;
		grid-template-columns: repeat(4, 1fr);
		gap: 10px;
		margin-top: 18px;
	}
	.account > div {
		min-width: 0;
		padding: 12px;
		background: var(--surface-bg);
		border: 1px solid var(--border-subtle);
		display: flex;
		flex-direction: column;
		gap: 4px;
	}
	.eyebrow {
		color: var(--text-3);
		font-size: 9px;
		font-weight: 800;
		text-transform: uppercase;
		letter-spacing: 0.08em;
	}
	strong {
		color: var(--text-strong);
		font-size: 13px;
		overflow: hidden;
		text-overflow: ellipsis;
	}
	small {
		color: var(--text-3);
		font-size: 10px;
	}
	.error {
		display: flex;
		justify-content: space-between;
		align-items: center;
		gap: 14px;
		margin-top: 16px;
		color: var(--status-danger-fg);
		font-size: 12px;
	}
	.error button {
		border: 1px solid var(--border-subtle);
		background: var(--surface-bg);
		color: var(--text-1);
		padding: 6px 10px;
		cursor: pointer;
	}
	.warning {
		color: var(--status-warning-fg);
	}
	code {
		color: var(--text-1);
	}
	@media (max-width: 760px) {
		.account {
			grid-template-columns: 1fr 1fr;
		}
	}
</style>
