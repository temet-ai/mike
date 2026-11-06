<script lang="ts">
	import Tooltip from '$lib/components/shared/Tooltip.svelte';
	import Badge from '$lib/components/shared/Badge.svelte';

	interface EndFields {
		end_label?: string | null;
		end_source?: string | null;
		end_rule?: string | null;
		end_confidence?: string | null;
		end_evidence?: string | null;
		failure_tags?: string[];
	}

	let { end, sessionId = '' }: { end: EndFields; sessionId?: string } = $props();

	const TIPS: Record<string, string> = {
		done: 'The operator got what they asked for and stopped.',
		closed_in_frustration: 'The operator closed the session unhappy; its final turn counts as a correction.',
		cut_off_neutral: 'The session stopped for a reason outside the work, such as a limit or a restart.',
		fleet_seat: 'A fleet seat that ran its brief; its end says nothing about the operator.',
		unclear: 'The evidence does not say why the session ended.'
	};

	const label = $derived(end.end_label ?? null);
	const origin = $derived(end.end_rule ? `rule ${end.end_rule}` : (end.end_source ?? 'llm'));
	const tip = $derived(
		[label ? TIPS[label] : '', end.end_evidence ? `Evidence: ${end.end_evidence}` : '']
			.filter(Boolean)
			.join(' ')
	);
</script>

{#if label}
	<span class="inline-flex flex-wrap items-center gap-1" data-session-end={label}>
		<Tooltip text={tip}>
			<Badge
				variant={label === 'closed_in_frustration' ? 'chip-active' : 'chip-muted'}
				data-end-axis="label">{label.replaceAll('_', ' ')}</Badge
			>
		</Tooltip>
		<Badge variant="chip-muted" data-end-axis="origin">{origin}</Badge>
		{#if end.end_confidence}
			<Badge variant="chip-muted" data-end-axis="confidence">{end.end_confidence}</Badge>
		{/if}
		{#each end.failure_tags ?? [] as tag (tag)}
			<Badge variant="chip-muted" data-end-axis="tag">{tag.replaceAll('_', ' ')}</Badge>
		{/each}
		{#if sessionId}
			<!-- eslint-disable-next-line svelte/no-navigation-without-resolve -- an inspect API URL opened as JSON, not an app route -->
			<a
				class="text-micro text-[var(--text-faint)] hover:text-[var(--text-strong)]"
				href="/api/sessions/{sessionId}/verdicts"
				target="_blank"
				rel="noopener noreferrer">verdict history</a
			>
		{/if}
	</span>
{/if}
