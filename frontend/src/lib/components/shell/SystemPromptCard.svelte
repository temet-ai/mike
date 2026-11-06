<script lang="ts">
	import { getContext } from 'svelte';
	import { resolve } from '$app/paths';
	import { Braces, FileText, Filter } from '@lucide/svelte';
	import { COLLAPSE_STORE_KEY } from '$lib/context-keys';
	import type { CollapseStore } from '$lib/stores/collapseState.svelte';
	import { formatTokens } from '$lib/components/message-view/parseSystemPrompt';
	import {
		classifySystemBlocks,
		type SystemBlockInput,
		type ClassifiedSystemBlock
	} from './systemBlockKinds';
	import * as Dialog from '$lib/components/ui/dialog';
	import BodyViewer from '$lib/components/detail/BodyViewer.svelte';
	import Tooltip from '$lib/components/shared/Tooltip.svelte';
	import PromptCompositionDiff from '$lib/components/prompts/PromptCompositionDiff.svelte';
	import type { BodyPayload } from '$lib/types';
	import DisclosureMarker from '$lib/components/shared/DisclosureMarker.svelte';

	interface Props {
		blocks: SystemBlockInput[];
		sessionId: string;
		systemRaw?: unknown[] | null;
		requestId?: number | null;
		sentBlocks?: string[] | null;
	}

	let {
		blocks,
		systemRaw = null,
		sessionId,
		requestId = null,
		sentBlocks = null
	}: Props = $props();

	const collapseStore = getContext<CollapseStore | undefined>(COLLAPSE_STORE_KEY);
	let localOpen = $state<Record<number, boolean>>({});

	const classified = $derived(classifySystemBlocks(blocks ?? []));

	function isAltered(block: ClassifiedSystemBlock): boolean {
		const sent = sentBlocks?.[block.index];
		return sent !== undefined && sent !== block.text;
	}

	const alteredCount = $derived(classified.filter((b) => isAltered(b)).length);

	function toJson(value: unknown): string {
		try {
			return JSON.stringify(value, null, 2);
		} catch {
			return String(value);
		}
	}

	function collapseKey(index: number): string {
		return `session:${sessionId}:sysblock:${index}`;
	}

	function isOpen(index: number): boolean {
		if (collapseStore) {
			return collapseStore.isExpanded(collapseKey(index), false);
		}
		return localOpen[index] ?? false;
	}

	function toggle(index: number): void {
		if (collapseStore) {
			collapseStore.toggle(collapseKey(index));
		} else {
			localOpen[index] = !(localOpen[index] ?? false);
		}
	}

	let jsonModal = $state<{ title: string; payload: BodyPayload } | null>(null);

	function byteLen(text: string): number {
		return typeof TextEncoder !== 'undefined' ? new TextEncoder().encode(text).length : text.length;
	}

	function makePayload(text: string): BodyPayload {
		return { text, byte_size: byteLen(text), has_body: true };
	}

	function openTopJson(): void {
		if (!systemRaw) return;
		jsonModal = { title: 'System prompt', payload: makePayload(toJson(systemRaw)) };
	}

	function openBlockJson(block: ClassifiedSystemBlock): void {
		if (block.raw == null) return;
		jsonModal = {
			title: `System block ${block.index + 1}`,
			payload: makePayload(toJson(block.raw))
		};
	}

	function setModalOpen(next: boolean): void {
		if (!next) jsonModal = null;
	}
</script>

{#snippet filterLink(blockIndex: number)}
	{#if requestId !== null}
		<a
			href={resolve(`/filter/workbench?from=request:${requestId}&block=${blockIndex}`)}
			class="spc-filter-link"
			aria-label={`Open filter workbench for block ${blockIndex + 1}`}
			title="Filter this block"
			data-slot="system-prompt-card-filter-this"
		>
			<Filter size={12} aria-hidden="true" />
		</a>
	{/if}
{/snippet}

{#snippet jsonButton(block: ClassifiedSystemBlock)}
	{#if block.raw != null}
		<button
			type="button"
			class="spc-json-toggle"
			aria-haspopup="dialog"
			aria-label="Show raw JSON for block {block.index + 1}"
			title="Raw JSON"
			onclick={() => openBlockJson(block)}
			data-slot="system-prompt-card-block-json"
		>
			<Braces size={12} aria-hidden="true" />
		</button>
	{/if}
{/snippet}

{#snippet alteredBadge(block: ClassifiedSystemBlock)}
	{#if isAltered(block)}
		<Tooltip
			text="This block was rewritten before sending it upstream — expand to compare original vs. sent."
		>
			<span
				class="badge-nx badge-warning spc-altered-badge"
				data-slot="system-prompt-card-block-altered"
				data-state="altered"
			>
				altered on wire
			</span>
		</Tooltip>
	{/if}
{/snippet}

<div
	class="spc-card"
	data-augmented-ui="tl-clip border"
	style="--aug-border-all: 1px; --aug-border-bg: linear-gradient(135deg, var(--data-teal-fg) 0%, var(--data-teal-border) 35%, var(--border-default) 70%);"
>
	<div class="spc-card-hd panel-hd">
		<FileText size={13} aria-hidden="true" />
		System prompt
		<span class="spc-badge">{classified.length} blocks</span>
		{#if alteredCount > 0}
			<Tooltip
				text={`${alteredCount} of ${classified.length} block${classified.length === 1 ? '' : 's'} were rewritten by a filter rule before being sent upstream.`}
			>
				<span
					class="badge-nx badge-warning"
					data-slot="system-prompt-card-altered-summary"
					data-state="altered"
				>
					{alteredCount} altered
				</span>
			</Tooltip>
		{/if}
		{#if systemRaw}
			<button
				type="button"
				class="spc-topjson-btn"
				aria-haspopup="dialog"
				aria-label="Show raw JSON for the full system array"
				title="Raw JSON of the full system array"
				onclick={openTopJson}
				data-slot="system-prompt-card-top-json"
			>
				<Braces size={12} aria-hidden="true" />
				<span>raw json</span>
			</button>
		{/if}
	</div>

	{#each classified as block (block.index)}
		{@const open = isOpen(block.index)}
		<div class="spc-block spc-block--{block.kind}">
			{#if block.kind === 'identity' && isAltered(block)}
				<div class="spc-row-wrap">
					<button
						type="button"
						class="spc-row"
						aria-expanded={open}
						aria-label="{open ? 'Collapse' : 'Expand'} identity (block {block.index + 1})"
						onclick={() => toggle(block.index)}
					>
						<DisclosureMarker {open} size={12} />
						<span class="spc-chip spc-chip-identity">{block.label}</span>
						<q class="spc-identity-quote">{block.text.trim()}</q>
					</button>
					{@render alteredBadge(block)}
					{@render filterLink(block.index)}
					{@render jsonButton(block)}
				</div>
				{#if open}
					<div class="spc-body">
						<div class="spc-diff-wrap">
							<PromptCompositionDiff
								composition={block.text}
								observed={sentBlocks?.[block.index] ?? null}
								leftLabel="Original"
								rightLabel="Sent"
							/>
						</div>
					</div>
				{/if}
			{:else if block.kind === 'identity'}
				<div class="spc-row-wrap">
					<div class="spc-row spc-row-static">
						<span class="spc-gutter" aria-hidden="true"></span>
						<span class="spc-chip spc-chip-identity">{block.label}</span>
						<q class="spc-identity-quote">{block.text.trim()}</q>
					</div>
					{@render filterLink(block.index)}
					{@render jsonButton(block)}
				</div>
			{:else if block.kind === 'billing'}
				<div class="spc-row-wrap">
					<button
						type="button"
						class="spc-row"
						aria-expanded={open}
						aria-label="{open ? 'Collapse' : 'Expand'} billing header (block {block.index + 1})"
						onclick={() => toggle(block.index)}
					>
						<DisclosureMarker {open} size={12} />
						<span class="spc-chip spc-chip-billing">{block.label}</span>
						<span class="spc-pairs">
							{#each block.billingPairs as pair (pair.key)}
								<span class="spc-pair">
									<span class="spc-pair-k">{pair.key}</span>
									<span class="spc-pair-v">{pair.value}</span>
								</span>
							{/each}
						</span>
						<span class="spc-raw-toggle" aria-hidden="true">[ raw ]</span>
					</button>
					{@render alteredBadge(block)}
					{@render filterLink(block.index)}
					{@render jsonButton(block)}
				</div>
				{#if open}
					<div class="spc-body">
						{#if isAltered(block)}
							<div class="spc-diff-wrap">
								<PromptCompositionDiff
									composition={block.text}
									observed={sentBlocks?.[block.index] ?? null}
									leftLabel="Original"
									rightLabel="Sent"
								/>
							</div>
						{:else}
							<pre class="spc-pre">{block.text}</pre>
						{/if}
					</div>
				{/if}
			{:else}
				<div class="spc-row-wrap">
					<button
						type="button"
						class="spc-row"
						aria-expanded={open}
						aria-label="{open ? 'Collapse' : 'Expand'} {block.kind === 'instructions'
							? 'instructions'
							: block.label} (block {block.index + 1})"
						onclick={() => toggle(block.index)}
					>
						<DisclosureMarker {open} size={12} />
						<span
							class="spc-chip"
							class:spc-chip-instructions={block.kind === 'instructions'}
							class:spc-chip-other={block.kind === 'other'}>{block.label}</span
						>
						<span class="spc-meta">
							{block.charCount.toLocaleString()} chars · {formatTokens(block.estTokens)}
						</span>
					</button>
					{@render alteredBadge(block)}
					{@render filterLink(block.index)}
					{@render jsonButton(block)}
				</div>
				{#if open}
					<div class="spc-body">
						{#if isAltered(block)}
							<div class="spc-diff-wrap">
								<PromptCompositionDiff
									composition={block.text}
									observed={sentBlocks?.[block.index] ?? null}
									leftLabel="Original"
									rightLabel="Sent"
								/>
							</div>
						{:else}
							<pre class="spc-pre">{block.text}</pre>
						{/if}
					</div>
				{/if}
			{/if}
		</div>
	{/each}
</div>

<Dialog.Root open={jsonModal !== null} onOpenChange={setModalOpen}>
	<Dialog.Content
		scrollBody={false}
		aria-describedby={undefined}
		class="!max-w-3xl !p-0 !gap-0 !flex !flex-col !max-h-[85vh] !overflow-hidden"
	>
		{#if jsonModal}
			<div class="spc-modal-head">
				<Dialog.Title>{jsonModal.title}</Dialog.Title>
			</div>
			<div class="spc-modal-body">
				<BodyViewer payload={jsonModal.payload} label="system" fill />
			</div>
		{/if}
	</Dialog.Content>
</Dialog.Root>

<style>
	.spc-card {
		--edge: 6px;
		--aug-tl: var(--edge);
		background: var(--surface-card);
		padding: 0;
		min-width: 0;
		overflow: hidden;
	}
	.spc-card-hd {
		display: flex;
		align-items: center;
		gap: 6px;
		padding: 12px 12px 10px;
		border-bottom: 1px solid var(--border-subtle);
	}
	.spc-badge {
		display: inline-flex;
		align-items: center;
		padding: 1px 6px;
		border-radius: 2px;
		background: var(--surface-2);
		box-shadow: inset 0 0 0 1px var(--border-subtle);
		color: var(--text-2);
		font-size: 10px;
		font-variant-numeric: tabular-nums;
	}

	.spc-block {
		position: relative;
		border-top: 1px solid var(--border-subtle);
	}
	.spc-block:first-of-type {
		border-top: none;
	}
	.spc-block::before {
		content: '';
		position: absolute;
		left: 0;
		top: 0;
		bottom: 0;
		width: 2px;
		background: var(--spc-rail-color, var(--border-subtle));
	}
	.spc-block--billing {
		--spc-rail-color: var(--data-teal-fg);
	}
	.spc-block--identity {
		--spc-rail-color: var(--status-info-fg);
	}
	.spc-block--instructions {
		--spc-rail-color: var(--role-tool-fg);
	}
	.spc-block--other {
		--spc-rail-color: var(--border-default);
	}
	.spc-row {
		display: flex;
		align-items: center;
		flex-wrap: wrap;
		gap: 6px;
		width: 100%;
		padding: 7px 12px;
		text-align: left;
		background: transparent;
		border: none;
		cursor: pointer;
		color: var(--text-3);
		font-size: 11px;
	}
	.spc-row:hover {
		color: var(--text-2);
		background: var(--surface-2);
	}
	.spc-row:focus-visible {
		outline: none;
		box-shadow: 0 0 0 2px var(--interactive-focus-ring);
	}
	.spc-row-static {
		cursor: default;
	}
	.spc-row-static:hover {
		background: transparent;
		color: var(--text-3);
	}
	.spc-gutter {
		flex-shrink: 0;
		width: 12px;
		height: 12px;
	}

	.spc-row-wrap {
		display: flex;
		align-items: center;
		min-width: 0;
	}
	.spc-row-wrap > .spc-row {
		flex: 1;
		width: auto;
		min-width: 0;
	}
	.spc-filter-link {
		display: inline-flex;
		align-items: center;
		justify-content: center;
		width: 24px;
		height: 24px;
		margin-right: 6px;
		flex-shrink: 0;
		border-radius: 2px;
		color: var(--text-3);
		opacity: 0;
		transition:
			opacity var(--dur-1) var(--ease-snap),
			color var(--dur-1) var(--ease-snap),
			background var(--dur-1) var(--ease-snap);
	}
	.spc-filter-link:hover {
		color: var(--interactive-active-fg);
		background: var(--surface-2);
	}
	.spc-block:hover .spc-filter-link,
	.spc-filter-link:focus-visible {
		opacity: 1;
	}
	.spc-filter-link:focus-visible {
		outline: none;
		box-shadow: 0 0 0 2px var(--interactive-focus-ring);
	}

	.spc-chip {
		display: inline-flex;
		align-items: center;
		padding: 1px 6px;
		border-radius: 2px;
		font-family: var(--font-mono-condensed);
		font-size: 10px;
		text-transform: uppercase;
		letter-spacing: 0.04em;
		white-space: nowrap;
	}
	.spc-chip-billing {
		color: var(--data-teal-fg);
		background: var(--data-teal-bg);
		box-shadow: inset 0 0 0 1px var(--data-teal-border);
	}
	.spc-chip-identity {
		color: var(--status-info-fg);
		background: var(--status-info-bg);
		box-shadow: inset 0 0 0 1px var(--status-info-border);
	}
	.spc-chip-instructions {
		color: var(--role-tool-fg);
		background: var(--role-tool-bg);
		box-shadow: inset 0 0 0 1px var(--role-tool-border);
	}
	.spc-chip-other {
		color: var(--text-2);
		background: var(--surface-2);
		box-shadow: inset 0 0 0 1px var(--border-subtle);
	}

	.spc-pairs {
		display: inline-flex;
		align-items: center;
		flex-wrap: wrap;
		gap: 4px;
		min-width: 0;
	}
	.spc-pair {
		display: inline-flex;
		align-items: baseline;
		gap: 0;
		font-family: var(--font-mono-condensed);
		font-size: 10px;
		white-space: nowrap;
	}
	.spc-pair-k {
		color: var(--text-3);
		background: var(--surface-2);
		padding: 1px 5px 1px 6px;
		border-radius: 2px 0 0 2px;
		box-shadow: inset 0 0 0 1px var(--border-subtle);
	}
	.spc-pair-v {
		color: var(--text-1);
		background: var(--data-teal-bg);
		box-shadow: inset 0 0 0 1px var(--data-teal-border);
		padding: 1px 6px 1px 5px;
		border-radius: 0 2px 2px 0;
		overflow-wrap: anywhere;
		word-break: break-word;
		white-space: normal;
	}

	.spc-identity-quote {
		font-family: var(--font-mono-condensed);
		font-size: 11px;
		font-style: italic;
		color: var(--text-1);
		overflow-wrap: anywhere;
		word-break: break-word;
		min-width: 0;
	}

	.spc-meta {
		margin-left: auto;
		font-family: var(--font-mono-condensed);
		font-size: 10px;
		color: var(--text-faint);
		white-space: nowrap;
	}

	.spc-raw-toggle {
		margin-left: auto;
		font-family: var(--font-mono-condensed);
		font-size: 10px;
		color: var(--text-2);
		white-space: nowrap;
		letter-spacing: 0.04em;
		opacity: 0.7;
		transition:
			opacity var(--dur-1) var(--ease-snap),
			color var(--dur-1) var(--ease-snap);
	}
	.spc-row:hover .spc-raw-toggle {
		color: var(--data-teal-fg);
		opacity: 1;
	}

	.spc-block--billing .spc-row {
		padding-top: 6px;
		padding-bottom: 6px;
	}
	.spc-block--identity .spc-row-static {
		padding-top: 9px;
		padding-bottom: 9px;
	}
	.spc-block--instructions .spc-meta {
		color: var(--text-2);
	}

	.spc-body {
		border-top: 1px solid var(--border-subtle);
		background: var(--surface-bg);
	}
	.spc-pre {
		margin: 0;
		padding: 8px 12px 10px;
		font-family: var(--font-mono-condensed);
		font-size: 11px;
		line-height: 1.55;
		color: var(--text-strong);
		white-space: pre-wrap;
		overflow-wrap: anywhere;
		word-break: break-word;
	}
	.spc-diff-wrap {
		padding: 8px 12px 10px;
	}

	.spc-altered-badge {
		margin-right: 6px;
		flex-shrink: 0;
		white-space: nowrap;
	}

	.spc-topjson-btn {
		margin-left: auto;
		display: inline-flex;
		align-items: center;
		gap: 4px;
		padding: 2px 6px;
		border: none;
		border-radius: 2px;
		background: transparent;
		color: var(--text-3);
		font-family: var(--font-mono-condensed);
		font-size: 10px;
		letter-spacing: 0.04em;
		text-transform: uppercase;
		cursor: pointer;
		transition:
			color var(--dur-1) var(--ease-snap),
			background var(--dur-1) var(--ease-snap);
	}
	.spc-topjson-btn:hover {
		color: var(--data-teal-fg);
		background: var(--surface-2);
	}
	.spc-topjson-btn:focus-visible {
		outline: none;
		box-shadow: 0 0 0 2px var(--interactive-focus-ring);
	}

	.spc-json-toggle {
		display: inline-flex;
		align-items: center;
		justify-content: center;
		width: 24px;
		height: 24px;
		margin-right: 6px;
		flex-shrink: 0;
		border: none;
		border-radius: 2px;
		background: transparent;
		color: var(--text-3);
		cursor: pointer;
		opacity: 0;
		transition:
			opacity var(--dur-1) var(--ease-snap),
			color var(--dur-1) var(--ease-snap),
			background var(--dur-1) var(--ease-snap);
	}
	.spc-json-toggle:hover {
		color: var(--data-teal-fg);
		background: var(--surface-2);
	}
	.spc-block:hover .spc-json-toggle,
	.spc-json-toggle:focus-visible {
		opacity: 1;
	}
	.spc-json-toggle:focus-visible {
		outline: none;
		box-shadow: 0 0 0 2px var(--interactive-focus-ring);
	}

	.spc-modal-head {
		display: flex;
		flex-direction: column;
		gap: 4px;
		padding: 14px 52px 12px 18px;
		border-bottom: 1px solid var(--border-default);
	}
	.spc-modal-body {
		flex: 1;
		min-height: 0;
		overflow-y: auto;
		padding: 14px 18px;
		display: flex;
		flex-direction: column;
	}
</style>
