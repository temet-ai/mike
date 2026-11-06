<script lang="ts">
	import { RefreshCw, CircleCheck, CircleX, SquareTerminal } from '@lucide/svelte';
	import { getLaunchDoctor, LaunchApiError } from '$lib/api/launches';
	import type { DoctorReport } from '$lib/api/launches';
	import SectionCard from '$lib/components/detail/SectionCard.svelte';
	import { Button } from '$lib/components/ui/button';

	interface Props {
		backend: string;
	}

	const { backend }: Props = $props();

	let validating = $state(false);
	let report = $state<DoctorReport | null>(null);
	let error = $state<string | null>(null);

	async function runValidate(): Promise<void> {
		validating = true;
		error = null;
		try {
			report = await getLaunchDoctor();
		} catch (err) {
			error = err instanceof LaunchApiError ? err.message : String(err);
		} finally {
			validating = false;
		}
	}
</script>

<div data-reveal>
	<SectionCard
		title="Terminal launch backend"
		titleIcon={SquareTerminal}
		description="Live validation of the chosen backend — opens and closes a real scratch terminal surface, so it takes a few seconds."
		collapsible={false}
	>
		{#snippet headerActions()}
			<Button variant="outline" size="sm" disabled={validating} onclick={() => void runValidate()}>
				<RefreshCw size={13} aria-hidden="true" class={validating ? 'lbv-spin' : undefined} />
				{validating ? 'Validating...' : 'Validate'}
			</Button>
		{/snippet}

		<div class="lbv-body">
			<div class="lbv-row">
				<span class="lbv-row__k">Configured backend</span>
				<code class="lbv-row__v">{backend}</code>
			</div>
			{#if report && report.backend !== backend}
				<p class="lbv-mismatch" role="status">
					The settings catalogue shows <code>{backend}</code> because nothing has been written
					through <code>/settings</code> for this key yet, but the backend actually running right
					now — and the one just validated — is <code>{report.backend}</code> (set via
					<code>MIKE_LAUNCH_BACKEND</code> or a CLI flag). Editing the field above only takes effect
					after a restart; it will not change which backend the check above just ran against.
				</p>
			{/if}

			{#if error}
				<p class="lbv-error" role="alert">{error}</p>
			{/if}

			{#if validating}
				<p class="lbv-status" role="status" aria-live="polite">
					Validating {backend} — opening a scratch surface, running it, closing it.
				</p>
			{:else if report}
				<div class="lbv-report">
					<span
						class="lbv-report__verdict"
						class:lbv-report__verdict--pass={report.passed}
						class:lbv-report__verdict--fail={!report.passed}
					>
						{#if report.passed}
							<CircleCheck size={14} aria-hidden="true" />
							<span>All checks passed</span>
						{:else}
							<CircleX size={14} aria-hidden="true" />
							<span>One or more checks failed</span>
						{/if}
					</span>
					<ul class="lbv-checks" role="list">
						{#each report.checks as check (check.name)}
							<li class="lbv-check" data-passed={check.passed}>
								<span
									class="lbv-check__icon"
									class:lbv-check__icon--pass={check.passed}
									class:lbv-check__icon--fail={!check.passed}
								>
									{#if check.passed}
										<CircleCheck size={13} aria-hidden="true" />
									{:else}
										<CircleX size={13} aria-hidden="true" />
									{/if}
								</span>
								<div class="lbv-check__body">
									<span class="lbv-check__name">{check.name}</span>
									<p class="lbv-check__detail">{check.detail}</p>
									{#if !check.passed && check.remedy}
										<p class="lbv-check__remedy">
											<span class="lbv-check__remedy-lead">Fix:</span>
											{check.remedy}
										</p>
									{/if}
								</div>
							</li>
						{/each}
					</ul>
				</div>
			{:else}
				<p class="lbv-note">Not validated this session — click Validate to run a live check.</p>
			{/if}
		</div>
	</SectionCard>
</div>

<style>
	.lbv-body {
		display: flex;
		flex-direction: column;
		gap: 12px;
	}
	.lbv-row {
		display: flex;
		align-items: baseline;
		gap: 8px;
	}
	.lbv-row__k {
		font-size: 11px;
		font-weight: 600;
		text-transform: uppercase;
		letter-spacing: 0.05em;
		color: var(--text-3);
	}
	.lbv-row__v {
		font-family: var(--font-mono, monospace);
		font-size: 12px;
		color: var(--text-1);
		background: var(--surface-2);
		border: 1px solid var(--border-default);
		border-radius: var(--r-tol);
		padding: 2px 8px;
	}

	.lbv-note {
		margin: 0;
		font-size: 12px;
		color: var(--text-3);
	}
	.lbv-status {
		margin: 0;
		font-size: 12px;
		color: var(--status-info-fg);
	}
	.lbv-error {
		margin: 0;
		font-size: 12px;
		color: var(--status-danger-fg);
		overflow-wrap: break-word;
		word-break: break-word;
	}
	.lbv-mismatch {
		margin: 0;
		font-size: 12px;
		line-height: 1.5;
		color: var(--status-warning-fg);
		overflow-wrap: break-word;
		word-break: break-word;
	}
	.lbv-mismatch code {
		font-family: var(--font-mono, monospace);
	}

	:global(.lbv-spin) {
		animation: lbv-spin 0.9s linear infinite;
	}
	@keyframes lbv-spin {
		from {
			transform: rotate(0deg);
		}
		to {
			transform: rotate(360deg);
		}
	}

	.lbv-report {
		display: flex;
		flex-direction: column;
		gap: 10px;
	}
	.lbv-report__verdict {
		display: inline-flex;
		align-items: center;
		gap: 6px;
		align-self: flex-start;
		font-size: 12px;
		font-weight: 600;
	}
	.lbv-report__verdict--pass {
		color: var(--status-success-fg);
	}
	.lbv-report__verdict--fail {
		color: var(--status-danger-fg);
	}

	.lbv-checks {
		list-style: none;
		margin: 0;
		padding: 0;
		display: flex;
		flex-direction: column;
		border: 1px solid var(--border-subtle);
		border-radius: var(--r-tol);
		overflow: hidden;
	}
	.lbv-check {
		display: flex;
		align-items: flex-start;
		gap: 10px;
		padding: 8px 10px;
		border-bottom: 1px solid var(--border-subtle);
	}
	.lbv-check:last-child {
		border-bottom: none;
	}
	.lbv-check__icon {
		flex-shrink: 0;
		margin-top: 1px;
	}
	.lbv-check__icon--pass {
		color: var(--status-success-fg);
	}
	.lbv-check__icon--fail {
		color: var(--status-danger-fg);
	}
	.lbv-check__body {
		display: flex;
		flex-direction: column;
		gap: 2px;
		min-width: 0;
	}
	.lbv-check__name {
		font-family: var(--font-mono, monospace);
		font-size: 12px;
		font-weight: 600;
		color: var(--text-strong);
	}
	.lbv-check__detail {
		margin: 0;
		font-size: 12px;
		color: var(--text-2);
		line-height: 1.5;
		overflow-wrap: break-word;
		word-break: break-word;
	}
	.lbv-check__remedy {
		margin: 0;
		font-size: 12px;
		color: var(--status-warning-fg);
		line-height: 1.5;
		overflow-wrap: break-word;
		word-break: break-word;
	}
	.lbv-check__remedy-lead {
		font-weight: 600;
	}
</style>
