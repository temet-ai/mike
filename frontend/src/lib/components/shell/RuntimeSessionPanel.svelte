<script lang="ts">
	import { onDestroy, untrack } from 'svelte';
	import {
		getRuntimeCaptureBody,
		getRuntimeCaptures,
		getSession,
		runtimeCaptureBodyUrl
	} from '$lib/api';
	import { invalidateApiCache } from '$lib/stores/cache/apiCache.svelte';
	import type {
		RuntimeCaptureDirection,
		RuntimeCaptureExchange,
		RuntimeCapturePage,
		RuntimeItem,
		RuntimeItemPage,
		RuntimeSessionObservation
	} from '$lib/types';
	import SectionCard from '$lib/components/detail/SectionCard.svelte';
	import CodeBlock from '$lib/components/message-view/CodeBlock.svelte';
	import { Button } from '$lib/components/ui/button';

	interface Props {
		sessionId: string;
		observation: RuntimeSessionObservation | null;
		initialItems: RuntimeItemPage | null;
	}

	let { sessionId, observation, initialItems }: Props = $props();
	let loadedSession = $state<string | null>(null);
	let items = $state<RuntimeItemPage | null>(null);
	let refreshed = $state<RuntimeSessionObservation | null>(null);
	let loading = $state(false);
	let error = $state<string | null>(null);
	let requestedOffset = $state(0);
	let captures = $state<RuntimeCapturePage | null>(null);
	let captureOffset = $state(0);
	let captureRequestedOffset = $state(0);
	let captureLoading = $state(false);
	let captureError = $state<string | null>(null);
	let captureRequest = 0;
	let metadataId = $state<string | null>(null);
	let selectedBody = $state<{ exchangeId: string; direction: RuntimeCaptureDirection } | null>(
		null
	);
	let body = $state<{ text: string | null; byteSize: number; complete: boolean } | null>(null);
	let bodyLoading = $state(false);
	let bodyError = $state<string | null>(null);
	let bodyRequest: AbortController | null = null;
	const directions: Array<[RuntimeCaptureDirection, string]> = [
		['request', 'Request'],
		['sent', 'Sent request'],
		['response', 'Response']
	];
	const observed = $derived(refreshed ?? observation);

	$effect(() => {
		const id = sessionId;
		const firstPage = initialItems;
		untrack(() => {
			if (loadedSession === id) return;
			loadedSession = id;
			items = firstPage;
			refreshed = null;
			error = null;
			loading = false;
			captures = null;
			captureLoading = false;
			captureOffset = 0;
			void loadCaptures(0);
		});
	});

	onDestroy(() => {
		captureRequest += 1;
		clearBody();
	});

	function value(value: string | number | null | undefined): string {
		return value == null ? 'Unknown' : typeof value === 'number' ? value.toLocaleString() : value;
	}

	const facts = $derived<Array<[string, string]>>(
		observed
			? [
					['Runtime', observed.runtime],
					['Lifecycle', value(observed.lifecycle)],
					['Mail delivery', value(observed.control)],
					['Source', observed.source],
					['Source version', value(observed.source_version)],
					['Fidelity', observed.fidelity],
					['Coverage', observed.completeness],
					['Observed at', observed.observed_at],
					['Requested model', value(observed.requested_model)],
					['Observed model', value(observed.observed_model)],
					['Usage scope', value(observed.usage_scope)],
					['Usage basis', value(observed.usage_basis)],
					['Input tokens', value(observed.input_tokens)],
					['Cached input tokens', value(observed.cached_input_tokens)],
					['Output tokens', value(observed.output_tokens)],
					['Reasoning output tokens', value(observed.reasoning_output_tokens)],
					['Total tokens', value(observed.total_tokens)],
					['Context window', value(observed.context_window)],
					['Model calls', value(observed.model_call_count)],
					['Reported cost', 'Unknown']
				]
			: []
	);

	function itemFacts(item: RuntimeItem): Array<[string, string]> {
		return [
			['Item', item.item_id],
			['Turn', value(item.turn_id)],
			['Status', value(item.status)],
			['Success', item.success === null ? 'Unknown' : String(item.success)],
			['Exit code', value(item.exit_code)],
			['First seen', item.first_seen_at],
			['Observed at', item.observed_at],
			['Source', item.source],
			['Fidelity', item.fidelity],
			['Coverage', item.completeness],
			['Content', item.content_availability]
		];
	}

	async function loadItems(offset: number, refresh = false): Promise<void> {
		if (loading) return;
		const id = sessionId;
		requestedOffset = offset;
		loading = true;
		error = null;
		try {
			if (refresh) await invalidateApiCache(`/api/sessions/${encodeURIComponent(id)}`);
			const detail = await getSession(id, {
				offset,
				limit: Math.max(1, Math.min(items?.limit ?? 50, 100)),
				order: 'desc'
			});
			if (sessionId !== id) return;
			if (!detail.runtime_items) throw new Error('Runtime activity is unavailable.');
			items = detail.runtime_items;
			refreshed = detail.observation ?? null;
		} catch (cause) {
			if (sessionId === id) error = cause instanceof Error ? cause.message : String(cause);
		} finally {
			if (sessionId === id) loading = false;
		}
	}

	function captureTitle(exchange: RuntimeCaptureExchange): string {
		return (
			[exchange.metadata.method, exchange.metadata.path]
				.filter((part): part is string => typeof part === 'string')
				.join(' ') || 'Captured exchange'
		);
	}

	function captureStatus(status: RuntimeCaptureExchange['status']): string {
		return {
			pending: 'In progress',
			complete: 'Complete',
			error: 'Error',
			aborted: 'Interrupted',
			refused: 'Unavailable'
		}[status];
	}

	function clearBody(): void {
		bodyRequest?.abort();
		bodyRequest = null;
		selectedBody = null;
		body = null;
		bodyLoading = false;
		bodyError = null;
	}

	async function loadCaptures(offset: number): Promise<void> {
		if (captureLoading) return;
		const id = sessionId;
		const request = ++captureRequest;
		captureRequestedOffset = offset;
		captureLoading = true;
		captureError = null;
		metadataId = null;
		clearBody();
		try {
			const page = await getRuntimeCaptures(id, offset);
			if (request !== captureRequest || sessionId !== id) return;
			captures = page;
			captureOffset = offset;
		} catch (cause) {
			if (request === captureRequest && sessionId === id) {
				captureError = cause instanceof Error ? cause.message : String(cause);
			}
		} finally {
			if (request === captureRequest && sessionId === id) captureLoading = false;
		}
	}

	async function loadBody(
		exchange: RuntimeCaptureExchange,
		direction: RuntimeCaptureDirection
	): Promise<void> {
		clearBody();
		const id = sessionId;
		const request = new AbortController();
		bodyRequest = request;
		selectedBody = { exchangeId: exchange.id, direction };
		bodyLoading = true;
		try {
			const result = await getRuntimeCaptureBody(id, exchange.id, direction, request.signal);
			if (bodyRequest !== request || sessionId !== id) return;
			let text: string | null = null;
			try {
				text = new TextDecoder('utf-8', { fatal: true, ignoreBOM: true }).decode(result.bytes);
			} catch {
				// Non-text bytes are available unchanged from the download endpoint.
			}
			body = { text, byteSize: result.bytes.byteLength, complete: result.complete };
		} catch (cause) {
			if (bodyRequest === request && sessionId === id) {
				bodyError = cause instanceof Error ? cause.message : String(cause);
			}
		} finally {
			if (bodyRequest === request && sessionId === id) bodyLoading = false;
		}
	}
</script>

{#snippet factRows(rows: Array<[string, string]>, label?: string)}
	<dl
		aria-label={label}
		class="grid grid-cols-1 gap-x-4 gap-y-3 text-xs sm:grid-cols-2 lg:grid-cols-3"
	>
		{#each rows as [name, fact] (name)}
			<div>
				<dt class="uppercase tracking-wider text-[var(--text-3)]">{name}</dt>
				<dd class="mt-0.5 break-words font-mono text-[var(--text-strong)]">{fact}</dd>
			</div>
		{/each}
	</dl>
{/snippet}

<div class="space-y-3" data-slot="runtime-session">
	{#if observed}
		<SectionCard
			title="Runtime observation"
			description="Activity metadata is independent of diagnostic wire capture; missing measurements remain unknown."
		>
			{#snippet headerActions()}
				<Button
					variant="outline"
					size="sm"
					disabled={loading}
					onclick={() => void loadItems(0, true)}
				>
					Refresh runtime activity
				</Button>
			{/snippet}
			{@render factRows(facts, 'Runtime observation facts')}
		</SectionCard>
	{/if}
	{#if observed || items}
		<SectionCard
			title="Runtime activity"
			description="Complete metadata rows for observed turns, tools and commands; content availability is shown for each item."
		>
			{#snippet headerActions()}
				{#if items}
					<span class="text-xs text-[var(--text-3)]" aria-live="polite">
						Items {items.items.length ? items.offset + 1 : 0}–{items.offset + items.items.length}
					</span>
					<Button
						variant="outline"
						size="sm"
						disabled={loading || items.offset === 0}
						onclick={() => void loadItems(Math.max(0, items!.offset - items!.limit))}
					>
						Previous activity page
					</Button>
					<Button
						variant="outline"
						size="sm"
						disabled={loading || !items.has_more}
						onclick={() => void loadItems(items!.offset + items!.limit)}
					>
						Next activity page
					</Button>
				{/if}
			{/snippet}
			{#if error}
				<p role="alert" class="text-xs text-[var(--status-danger-fg)]">{error}</p>
				<Button
					variant="outline"
					size="sm"
					disabled={loading}
					onclick={() => void loadItems(requestedOffset)}
				>
					Retry runtime activity
				</Button>
			{/if}
			{#if items === null}
				<p class="text-xs text-[var(--text-3)]">Runtime activity is unavailable.</p>
			{:else if items.items.length === 0}
				<p class="text-xs text-[var(--text-3)]">No runtime activity items on this page.</p>
			{:else}
				<ol class="space-y-3" aria-label="Runtime activity items" start={items.offset + 1}>
					{#each items.items as item (item.item_id)}
						<li class="border-t border-[var(--border-subtle)] pt-3">
							<h4 class="mb-2 break-words font-mono text-xs text-[var(--text-strong)]">
								{item.kind} · {item.name ?? item.item_id}
							</h4>
							{@render factRows(itemFacts(item))}
						</li>
					{/each}
				</ol>
			{/if}
		</SectionCard>
	{/if}
	<SectionCard
		title="Captured exchanges"
		description="Open request and response bodies or download their complete captured bytes."
	>
		{#snippet headerActions()}
			<Button
				variant="outline"
				size="sm"
				disabled={captureLoading}
				onclick={() => void loadCaptures(0)}
			>
				Refresh captures
			</Button>
			{#if captures}
				<span class="text-xs text-[var(--text-3)]" aria-live="polite">
					Exchanges {captures.exchanges.length ? captureOffset + 1 : 0}–{captureOffset +
						captures.exchanges.length}
				</span>
				<Button
					variant="outline"
					size="sm"
					disabled={captureLoading || captureOffset === 0}
					onclick={() => void loadCaptures(Math.max(0, captureOffset - 20))}
				>
					Previous capture page
				</Button>
				<Button
					variant="outline"
					size="sm"
					disabled={captureLoading || captures.exchanges.length < 20}
					onclick={() => void loadCaptures(captures!.next_offset)}
				>
					Next capture page
				</Button>
			{/if}
		{/snippet}
		{#if captureError}
			<p role="alert" class="break-words text-xs text-[var(--status-danger-fg)]">{captureError}</p>
			<Button
				variant="outline"
				size="sm"
				disabled={captureLoading}
				onclick={() => void loadCaptures(captureRequestedOffset)}
			>
				Retry captures
			</Button>
		{:else if captures === null}
			<p role="status" class="text-xs text-[var(--text-3)]">Loading captured exchanges…</p>
		{/if}
		{#if captures?.exchanges.length === 0}
			<p class="text-xs text-[var(--text-3)]">No captured exchanges on this page.</p>
		{:else if captures}
			<ol class="space-y-3" aria-label="Captured exchanges" start={captureOffset + 1}>
				{#each captures.exchanges as exchange (exchange.id)}
					{@const selected = selectedBody?.exchangeId === exchange.id ? selectedBody : null}
					<li class="space-y-2 border-t border-[var(--border-subtle)] pt-3">
						<h4 class="break-words font-mono text-xs text-[var(--text-strong)]">
							{captureTitle(exchange)}
						</h4>
						{@render factRows([
							['Created', exchange.created_at],
							['Status', captureStatus(exchange.status)],
							[
								'Response status',
								value(
									typeof exchange.metadata.response_status === 'number'
										? exchange.metadata.response_status
										: null
								)
							]
						])}
						<div class="flex flex-wrap gap-2">
							{#each directions as [direction, label] (direction)}
								<Button
									variant="outline"
									size="sm"
									disabled={direction === 'response' && exchange.response_document_id === null}
									aria-expanded={selected?.direction === direction}
									aria-controls={selected?.direction === direction
										? `capture-body-${exchange.id}`
										: undefined}
									onclick={() =>
										selected?.direction === direction
											? clearBody()
											: void loadBody(exchange, direction)}
								>
									{selected?.direction === direction ? 'Hide' : 'View'}
									{label.toLowerCase()}
								</Button>
							{/each}
							<Button
								variant="outline"
								size="sm"
								aria-expanded={metadataId === exchange.id}
								aria-controls={metadataId === exchange.id
									? `capture-metadata-${exchange.id}`
									: undefined}
								onclick={() => (metadataId = metadataId === exchange.id ? null : exchange.id)}
							>
								{metadataId === exchange.id ? 'Hide' : 'View'} exchange details
							</Button>
						</div>
						{#if metadataId === exchange.id}
							<div
								id={`capture-metadata-${exchange.id}`}
								role="region"
								aria-label="Exchange details"
							>
								<CodeBlock lang="json" code={JSON.stringify(exchange, null, 2)} />
							</div>
						{/if}
						{#if selected}
							<div
								id={`capture-body-${exchange.id}`}
								role="region"
								aria-label="Captured body"
								aria-busy={bodyLoading}
								class="space-y-2"
							>
								{#if bodyLoading}
									<p role="status" class="text-xs text-[var(--text-3)]">Loading captured body…</p>
								{:else if bodyError}
									<p role="alert" class="break-words text-xs text-[var(--status-danger-fg)]">
										{bodyError}
									</p>
									<Button
										variant="outline"
										size="sm"
										onclick={() => void loadBody(exchange, selected.direction)}
									>
										Retry body
									</Button>
								{:else if body}
									<div class="flex flex-wrap items-center gap-2 text-xs text-[var(--text-3)]">
										<span>{body.byteSize.toLocaleString()} bytes</span>
										<Button
											variant="outline"
											size="sm"
											href={runtimeCaptureBodyUrl(sessionId, exchange.id, selected.direction)}
											download={`${exchange.id}-${selected.direction}.${body.text === null ? 'bin' : 'txt'}`}
										>
											Download {directions
												.find(([direction]) => direction === selected.direction)?.[1]
												.toLowerCase()}
										</Button>
									</div>
									{#if !body.complete}
										<p role="status" class="text-xs text-[var(--text-3)]">
											{exchange.status === 'pending'
												? 'Capture is still in progress. Refresh to see newer bytes.'
												: 'Capture was interrupted. These are the bytes received before it stopped.'}
										</p>
									{/if}
									{#if body.text !== null}
										<CodeBlock lang="text" code={body.text} />
									{:else}
										<p class="text-xs text-[var(--text-3)]">
											This body is binary. Download it to read the complete captured bytes.
										</p>
									{/if}
								{/if}
							</div>
						{/if}
					</li>
				{/each}
			</ol>
		{/if}
	</SectionCard>
</div>
