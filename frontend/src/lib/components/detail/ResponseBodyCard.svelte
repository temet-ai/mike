<script lang="ts">
	import CompressionFlag from '$lib/components/compression/CompressionFlag.svelte';
	import type { BodyPayload, RequestDetail } from '$lib/types';
	import { useLazyBody } from '$lib/stores/bodyFetcher.svelte';
	import { isAbortedRequest, isNonAbortedStreamingError } from '$lib/status';
	import BodyViewer from './BodyViewer.svelte';
	import ToolCallCard from '$lib/components/tool-calls/ToolCallCard.svelte';

	let { detail, fill = false }: { detail: RequestDetail; fill?: boolean } = $props();

	const isPending = $derived(detail.status === 'Pending');
	const isAborted = $derived(isAbortedRequest(detail.status));
	const isStreamingError = $derived(isNonAbortedStreamingError(detail.status));
	const streamingErrorMsg = $derived(
		isStreamingError && typeof detail.status === 'object' && 'Error' in detail.status
			? (detail.status as { Error: string }).Error
			: ''
	);
	const partialText = $derived(detail.accumulated_text ?? '');

	const hasRecoveredContent = $derived(
		(detail.accumulated_text ?? '').length > 0 ||
			(detail.accumulated_thinking ?? '').length > 0 ||
			detail.tool_calls.length > 0
	);

	function parseToolInput(inputJson: string): unknown {
		try {
			return JSON.parse(inputJson);
		} catch {
			return inputJson;
		}
	}

	const absentPayload = $derived<BodyPayload>({
		text: null,
		byte_size: 0,
		has_body: false
	});

	const body = useLazyBody(() => detail, 'response');
</script>

<CompressionFlag target={{ kind: 'response', request_id: detail.id }} />

{#if isPending}
	<div class="flex flex-col gap-2">
		<span
			data-slot="response-state"
			data-state="pending"
			class="inline-flex w-fit items-center gap-1.5 rounded-[2px] border border-[var(--status-warning-border)] bg-[var(--status-warning-bg)] px-2 py-0.5 text-label uppercase tracking-wider text-[var(--status-warning-fg)]"
		>
			Response in progress
		</span>
		{#if partialText !== ''}
			<pre
				class="max-h-96 overflow-auto whitespace-pre-wrap font-mono text-xs text-[var(--text-strong)]">{partialText}</pre>
		{:else}
			<p class="text-sm italic text-[var(--text-3)]">
				Request sent — awaiting upstream response. No streamed chunks received yet.
			</p>
		{/if}
	</div>
{:else if isAborted}
	<div class="flex flex-col gap-2">
		<span
			data-slot="response-state"
			data-state="aborted"
			class="inline-flex w-fit items-center gap-1.5 rounded-[2px] border border-[var(--status-danger-border)] bg-[var(--status-danger-bg)] px-2 py-0.5 text-label uppercase tracking-wider text-[var(--status-danger-fg)]"
		>
			Aborted
		</span>
		{#if partialText !== ''}
			<p class="text-xs text-[var(--text-3)]">
				Upstream connection dropped mid-stream. Showing the partial response that arrived before the
				abort:
			</p>
			<pre
				class="max-h-96 overflow-auto whitespace-pre-wrap font-mono text-xs text-[var(--text-strong)]">{partialText}</pre>
		{:else}
			<p class="text-sm italic text-[var(--text-3)]">
				Stream aborted before any response chunks arrived — no response content was captured.
			</p>
		{/if}
	</div>
{:else if isStreamingError}
	<div class="flex flex-col gap-2">
		<span
			data-slot="response-state"
			data-state="stream-error"
			class="inline-flex w-fit items-center gap-1.5 rounded-[2px] border border-[var(--status-danger-border)] bg-[var(--status-danger-bg)] px-2 py-0.5 text-label uppercase tracking-wider text-[var(--status-danger-fg)]"
		>
			Stream error
		</span>
		{#if streamingErrorMsg}
			<p class="font-mono text-xs text-[var(--status-danger-fg)]">{streamingErrorMsg}</p>
		{/if}
		{#if partialText !== ''}
			<p class="text-xs text-[var(--text-3)]">Partial content captured before the error:</p>
			<pre
				class="max-h-96 overflow-auto whitespace-pre-wrap font-mono text-xs text-[var(--text-strong)]">{partialText}</pre>
		{:else}
			<p class="text-sm italic text-[var(--text-3)]">
				No response content was captured before the stream error.
			</p>
		{/if}
	</div>
{:else if detail.is_streaming && detail.status === 'Streaming'}
	{#if (detail.accumulated_text ?? '') === ''}
		<p class="text-sm italic text-[var(--text-3)]">Receiving stream…</p>
	{:else}
		<pre
			class="max-h-96 overflow-auto whitespace-pre-wrap font-mono text-xs text-[var(--text-strong)]">{detail.accumulated_text ??
				''}</pre>
	{/if}
{:else if detail.is_streaming && detail.response_body_size === 0 && hasRecoveredContent}
	<div class="flex flex-col gap-4">
		{#if (detail.accumulated_text ?? '').length > 0}
			<section>
				<h3 class="section-label">Accumulated text</h3>
				<div class="code">
					<div class="bar"><span class="lang">text</span></div>
					<pre>{detail.accumulated_text}</pre>
				</div>
			</section>
		{/if}

		{#if (detail.accumulated_thinking ?? '').length > 0}
			<section>
				<h3 class="section-label">Accumulated thinking</h3>
				<div class="code">
					<div class="bar"><span class="lang">thinking</span></div>
					<pre>{detail.accumulated_thinking}</pre>
				</div>
			</section>
		{/if}

		{#if detail.tool_calls.length > 0}
			<section>
				<h3 class="section-label">Tool calls ({detail.tool_calls.length})</h3>
				<ul class="flex flex-col gap-2 list-none p-0 m-0">
					{#each detail.tool_calls as call, idx (idx)}
						<li>
							{#if !call.complete}
								<p class="text-xs italic text-[var(--text-3)]">{call.name} (incomplete)</p>
							{/if}
							<ToolCallCard name={call.name} input={parseToolInput(call.input_json)} />
						</li>
					{/each}
				</ul>
			</section>
		{/if}
	</div>
{:else if detail.response_body_size === 0}
	<BodyViewer payload={absentPayload} label="response" {fill} />
{:else if body.text !== null}
	<BodyViewer payload={body.text} label="response" {fill} />
{:else if body.error !== null}
	<div
		class="flex items-center justify-between gap-3 rounded-[2px] border border-[var(--status-danger-border)] bg-[var(--status-danger-bg)] px-3 py-2 text-xs text-[var(--status-danger-fg)]"
	>
		<span>Failed to load response body: {body.error}</span>
		<button
			type="button"
			class="rounded-[2px] border border-[var(--status-danger-border)] px-2 py-0.5 text-label uppercase tracking-wider text-[var(--status-danger-fg)] hover:border-[var(--status-danger-fg)] hover:bg-[var(--status-danger-bg)]"
			onclick={body.retry}>Retry</button
		>
	</div>
{:else if body.loading}
	<p class="text-xs italic text-[var(--text-3)]">Loading body…</p>
{/if}
