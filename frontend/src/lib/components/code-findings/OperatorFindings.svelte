<script lang="ts">
	import { resolve } from '$app/paths';
	import { ApiError } from '$lib/api';
	import { acceptCodeFinding, type CodeFindingForOperator } from '$lib/api/codeFindings';
	import { Button } from '$lib/components/ui/button';
	import { formatRelative } from '$lib/time';

	interface Props {
		findings: CodeFindingForOperator[];
		now: number;
		onAccepted: () => void;
	}

	let { findings, now, onAccepted }: Props = $props();
	let busy = $state<number | null>(null);
	let error = $state<string | null>(null);

	async function accept(id: number): Promise<void> {
		busy = id;
		error = null;
		try {
			await acceptCodeFinding(id);
			onAccepted();
		} catch (err) {
			error = err instanceof ApiError ? err.message : String(err);
		} finally {
			busy = null;
		}
	}
</script>

{#if error}<p class="of-error" role="alert">{error}</p>{/if}
<ul class="of-list" aria-label="Waiting for you">
	{#each findings as finding (finding.id)}
		<li class="of-item">
			<div class="of-text">
				<span class="font-mono">{finding.path}:{finding.lines.join(', ')}</span>
				<span>{finding.rule_id} ({finding.severity}): {finding.message}</span>
				{#if finding.answer_reason}<span class="of-reason">
						Reason: {finding.answer_reason}
					</span>{/if}
				<span class="of-meta">
					{finding.answer === 'waiting'
						? finding.answer_ruling
						: finding.answer === 'refused'
							? 'Reason refused, then committed without a fix.'
							: 'Committed without a fix or an answer.'}
					{formatRelative(finding.since, now)}.
					{#if finding.session_id}
						<a
							class="of-link"
							href={resolve('/sessions/[sessionId]', { sessionId: finding.session_id })}
						>
							Session
						</a>
					{/if}
				</span>
			</div>
			{#if finding.answer === 'waiting'}
				<Button
					variant="ghost"
					size="sm"
					disabled={busy !== null}
					onclick={() => void accept(finding.id)}
				>
					Accept
				</Button>
			{/if}
		</li>
	{/each}
</ul>

<style>
	.of-list {
		margin: 0;
		padding: 0;
		list-style: none;
		display: flex;
		flex-direction: column;
		gap: var(--space-3);
	}
	.of-item {
		display: flex;
		justify-content: space-between;
		align-items: flex-start;
		gap: var(--space-3);
	}
	.of-text {
		display: flex;
		flex-direction: column;
		min-width: 0;
		overflow-wrap: anywhere;
	}
	.of-reason {
		color: var(--text-2);
	}
	.of-meta {
		color: var(--text-3);
	}
	.of-link {
		color: var(--interactive-active-fg);
	}
	.of-error {
		margin: 0;
		color: var(--status-danger-fg);
	}
</style>
