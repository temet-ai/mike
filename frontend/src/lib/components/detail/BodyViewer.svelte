<script lang="ts">
	import { onMount } from 'svelte';
	import type { BodyPayload } from '$lib/types';
	import MessageContentRenderer, {
		renderBody
	} from '../message-view/MessageContentRenderer.svelte';
	import Tooltip from '$lib/components/shared/Tooltip.svelte';

	let {
		payload,
		label,
		fill = false
	}: {
		payload: BodyPayload;
		label: 'request' | 'response' | 'system';
		fill?: boolean;
	} = $props();

	function prettyPrint(raw: string): { text: string; ok: boolean; parsed: unknown } {
		try {
			const parsed: unknown = JSON.parse(raw);
			return { text: JSON.stringify(parsed, null, 2), ok: true, parsed };
		} catch {
			return { text: raw, ok: false, parsed: null };
		}
	}

	function formatBytes(n: number): string {
		if (n < 1024) return `${n.toLocaleString('en-US')} B`;
		if (n < 1024 * 1024) return `${(n / 1024).toFixed(1)} KB`;
		return `${(n / (1024 * 1024)).toFixed(1)} MB`;
	}

	const body = $derived(payload.text);
	const formatted = $derived(body === null ? null : prettyPrint(body));
	const rendered = $derived(formatted?.text ?? null);
	const parseOk = $derived(formatted?.ok ?? false);
	const renderedBody = $derived(
		formatted?.ok ? renderBody(formatted.parsed) : { shape: 'unknown' as const }
	);
	const hasRenderedView = $derived(renderedBody.shape !== 'unknown');
	let activeView = $state<'rendered' | 'raw'>('rendered');
	const effectiveView = $derived(hasRenderedView ? activeView : 'raw');
	const sizeLabel = $derived(formatBytes(payload.byte_size));
	const absentLabel = $derived(
		label === 'request'
			? 'No request body captured'
			: label === 'system'
				? 'No system prompt captured'
				: 'No response body captured'
	);
	const copyTitle = $derived(
		label === 'request'
			? 'Copy raw request body'
			: label === 'system'
				? 'Copy raw system JSON'
				: 'Copy raw response body'
	);

	let clipboardSupported = $state(false);
	let copied = $state(false);
	let copyTimer: ReturnType<typeof setTimeout> | null = null;

	onMount(() => {
		clipboardSupported = typeof navigator !== 'undefined' && !!navigator.clipboard?.writeText;
		return () => {
			if (copyTimer !== null) clearTimeout(copyTimer);
		};
	});

	async function handleCopy(): Promise<void> {
		if (body === null) return;
		try {
			await navigator.clipboard.writeText(body);
			copied = true;
			if (copyTimer !== null) clearTimeout(copyTimer);
			copyTimer = setTimeout(() => {
				copied = false;
				copyTimer = null;
			}, 1500);
		} catch {}
	}
</script>

{#if !payload.has_body}
	<p class="text-sm italic text-[var(--text-faint)]">{absentLabel}</p>
{:else if rendered === null}
	<p class="text-sm italic text-[var(--text-faint)]">Body present but not UTF-8 decodable</p>
{:else}
	<div class="mb-2 flex flex-wrap items-center justify-between gap-3">
		<div class="flex flex-wrap items-center gap-x-3 gap-y-1 text-label text-[var(--text-3)]">
			<Tooltip text="Raw byte count (UTF-8). Pre-decode size — accurate for binary content.">
				<span class="font-mono tabular-nums text-[var(--text-2)]">{sizeLabel}</span>
			</Tooltip>
			{#if !parseOk}
				<span class="text-[var(--status-warning-fg)]">(raw — not valid JSON)</span>
			{/if}
		</div>
		<div class="flex items-center gap-2">
			{#if hasRenderedView}
				<div
					class="inline-flex items-center rounded-[2px] border border-[var(--border-default)] bg-[var(--surface-bg)] text-label"
					role="tablist"
					aria-label="Body view"
				>
					<Tooltip text="Pretty-printed view with Claude message blocks expanded.">
						{#snippet child({ props })}
							<button
								{...props}
								type="button"
								role="tab"
								aria-selected={effectiveView === 'rendered'}
								class="rounded-l-[2px] px-2 py-0.5 uppercase tracking-wider {effectiveView ===
								'rendered'
									? 'bg-[var(--surface-2)] text-[var(--text-strong)]'
									: 'text-[var(--text-2)] hover:text-[var(--text-strong)]'}"
								onclick={() => (activeView = 'rendered')}>Rendered</button
							>
						{/snippet}
					</Tooltip>
					<Tooltip text="Raw bytes captured (one per line).">
						{#snippet child({ props })}
							<button
								{...props}
								type="button"
								role="tab"
								aria-selected={effectiveView === 'raw'}
								class="rounded-r-[2px] px-2 py-0.5 uppercase tracking-wider {effectiveView === 'raw'
									? 'bg-[var(--surface-2)] text-[var(--text-strong)]'
									: 'text-[var(--text-2)] hover:text-[var(--text-strong)]'}"
								onclick={() => (activeView = 'raw')}>Raw JSON</button
							>
						{/snippet}
					</Tooltip>
				</div>
			{/if}
			{#if clipboardSupported && body !== null}
				<button
					type="button"
					class="rounded-[2px] border border-[var(--border-default)] px-2 py-0.5 text-label uppercase tracking-wider text-[var(--text-1)] hover:border-[var(--text-3)] hover:bg-[var(--surface-2)] hover:text-[var(--text-strong)]"
					title={copyTitle}
					onclick={handleCopy}>{copied ? 'Copied' : 'Copy'}</button
				>
			{/if}
		</div>
	</div>
	{#if effectiveView === 'rendered' && renderedBody.shape !== 'unknown'}
		<!-- svelte-ignore a11y_no_noninteractive_tabindex -->
		<div
			role="region"
			tabindex="0"
			aria-label="{label} body"
			class="{fill
				? ''
				: 'max-h-[600px]'} overflow-auto rounded-[2px] border border-[var(--border-default)] bg-[var(--surface-bg)] p-3 text-xs text-[var(--text-strong)]"
		>
			{#if renderedBody.shape === 'request'}
				<div class="flex flex-col gap-4">
					{#each renderedBody.messages as message, i (i)}
						<div
							class="flex flex-col gap-2"
							class:border-t={i > 0}
							class:border-[var(--border-subtle)]={i > 0}
							class:pt-4={i > 0}
						>
							<span
								class="inline-block self-start rounded-[2px] border border-[var(--border-subtle)] bg-[var(--surface-2)] px-2 py-0.5 text-caption font-semibold uppercase tracking-wider text-[var(--text-1)]"
								>{message.role}</span
							>
							{#if message.blocks.length === 0}
								<p class="text-[var(--text-faint)] italic">(empty)</p>
							{:else}
								<MessageContentRenderer blocks={message.blocks} />
							{/if}
						</div>
					{/each}
				</div>
			{:else}
				<MessageContentRenderer blocks={renderedBody.blocks} />
			{/if}
		</div>
	{:else}
		<!-- svelte-ignore a11y_no_noninteractive_tabindex -->
		<pre
			role="region"
			tabindex="0"
			aria-label="{label} raw JSON"
			class="{fill
				? ''
				: 'max-h-[600px]'} overflow-auto whitespace-pre-wrap break-words rounded-[2px] border border-[var(--border-default)] bg-[var(--surface-bg)] p-3 font-mono text-xs text-[var(--text-strong)]">{rendered}</pre>
	{/if}
{/if}
