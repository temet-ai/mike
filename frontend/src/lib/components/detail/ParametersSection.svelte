<script lang="ts">
	import type { HTMLAttributes } from 'svelte/elements';
	import { cn, type WithElementRef } from '$lib/utils';
	import type { RequestDetail } from '$lib/types';
	import { formatCount } from '$lib/time';
	import SectionCard from './SectionCard.svelte';
	import Tooltip from '../shared/Tooltip.svelte';

	interface Props extends WithElementRef<HTMLAttributes<HTMLDivElement>> {
		detail: RequestDetail;
	}

	let { ref = $bindable(null), class: className, detail, ...restProps }: Props = $props();

	const thinkingLabel = $derived(
		detail.params.thinking_mode ?? (detail.params.thinking_enabled ? 'enabled' : 'disabled')
	);

	type ParamHelp = { text: string; slug: string };
	const PARAM_HELP: Record<string, ParamHelp> = {
		max_tokens: {
			text: 'Maximum output tokens the model is allowed to generate',
			slug: 'request-parameters'
		},
		temperature: {
			text: 'Sampling temperature (0.0 deterministic — 1.0 creative)',
			slug: 'request-parameters'
		},
		thinking: {
			text: 'Extended thinking mode (off, on, or adaptive)',
			slug: 'thinking-mode'
		},
		effort: {
			text: 'How much effort the model was asked to spend (low to max), including a change made earlier in the conversation',
			slug: 'request-parameters'
		},
		streaming: {
			text: 'Whether the response is delivered as SSE chunks (true) or one JSON body (false)',
			slug: 'streaming-vs-non-streaming'
		},
		tool_loop: {
			text: 'This request is part of a tool-use loop — the assistant is responding to a tool result',
			slug: 'tool-loop'
		},
		response_status: {
			text: 'HTTP status returned upstream — 200 success, 4xx client error, 5xx server error',
			slug: 'http-status'
		}
	};

	const rows = $derived<Array<[string, string]>>([
		['max_tokens', detail.params.max_tokens !== null ? formatCount(detail.params.max_tokens) : '—'],
		['temperature', detail.params.temperature !== null ? String(detail.params.temperature) : '—'],
		['thinking', thinkingLabel],
		['effort', detail.params.effort ?? '—'],
		['streaming', detail.is_streaming ? 'true' : 'false'],
		['tool_loop', detail.is_tool_loop ? 'true' : 'false'],
		['response_status', detail.response_status !== null ? String(detail.response_status) : '—']
	]);
</script>

<div
	bind:this={ref}
	data-slot="parameters-section"
	class={cn('contents', className)}
	{...restProps}
>
	<SectionCard
		title="Parameters"
		ariaLabel="Request parameters"
		collapsible
		sectionKey="detail:parameters"
	>
		<dl
			data-slot="parameters-grid"
			class="grid grid-cols-1 gap-x-4 gap-y-3 text-xs sm:grid-cols-2 lg:grid-cols-3"
		>
			{#each rows as [label, value] (label)}
				<div data-slot="parameters-row">
					<dt class="uppercase tracking-wider text-[var(--text-3)]">
						{#if PARAM_HELP[label]}
							<Tooltip text={PARAM_HELP[label].text} slug={PARAM_HELP[label].slug}>
								<span>{label}</span>
							</Tooltip>
						{:else}
							{label}
						{/if}
					</dt>
					<dd class="mt-0.5 font-mono text-[var(--text-strong)]">{value}</dd>
				</div>
			{/each}
		</dl>
	</SectionCard>
</div>
