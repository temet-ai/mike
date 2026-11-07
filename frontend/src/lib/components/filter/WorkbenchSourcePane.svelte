<script lang="ts">
	import type { FilterRuleDto, SystemBlock } from '$lib/types';
	import { diffBlocks, type DiffRow } from '$lib/utils/blockDiff';
	import { diffText, diffTextWithPreview, type DiffSegment } from './promptDiff';
	import { splitBlockIntoDisplayParagraphs } from './blockParagraphs';
	import { deriveRuleTitle, paragraphIndicesTouchedByRule, ruleBeforeAfter } from './rule-matching';
	import { classifySystemBlock } from '$lib/components/shell/systemBlockKinds';

	export type PromptLens = 'source' | 'changes' | 'result';

	/** Not a real block index — the "Session message" section (G9) is threaded through the
	 *  same block/paragraph machinery as every other block, so it gets the same diffing, margin
	 *  attribution and two-level preview colouring for free. Real block indices are always >= 0. */
	const SESSION_BLOCK_INDEX = -1;

	interface Props {
		blocks: SystemBlock[];
		orderedRules: FilterRuleDto[];
		/** The dry-run's `filtered_system` block texts, or null when no dry-run has run yet. */
		resultBlocks: string[] | null;
		/** A second dry-run result with an assistant proposal applied, shown in blue. */
		previewBlocks: string[] | null;
		/** The dry-run's authoritative firing ids — used to attribute a margin edit to result-only
		 *  content (a brand-new paragraph/block) that has no source paragraph to key off. */
		firingRuleIds: ReadonlySet<string> | null;
		/** The request's last user message (spec 4.2/G9: rules that land there get margin titles
		 *  and a diff too, not just the system blocks). Omitted or empty hides the section. */
		sessionMessage?: string;
		/** The dry-run's filtered last user message, or null/undefined before a dry run resolves
		 *  (falls back to `sessionMessage`, i.e. "no change yet"). */
		resultSessionMessage?: string | null;
		/** Same, with the previewed proposal applied. */
		previewSessionMessage?: string | null;
		lens: PromptLens;
		/** Initial `?block=N` scroll target (block granularity). */
		scrollToBlock: number | null;
		/** "Show in prompt" scroll target: `${blockIndex}:${paragraphIndex}`. */
		scrollToParagraph: string | null;
		onAskAbout: (quote: string) => void;
		onOpenEdit: (ruleId: string) => void;
	}

	let {
		blocks,
		orderedRules,
		resultBlocks,
		previewBlocks,
		firingRuleIds,
		sessionMessage = '',
		resultSessionMessage = null,
		previewSessionMessage = null,
		lens,
		scrollToBlock,
		scrollToParagraph,
		onAskAbout,
		onOpenEdit
	}: Props = $props();

	let paneEl = $state<HTMLDivElement | null>(null);

	const enabledRules = $derived(orderedRules.filter((r) => r.enabled));
	const hasSessionMessage = $derived(sessionMessage.trim().length > 0);

	interface MarginEdit {
		ruleId: string;
		label: string;
	}

	/** Margin edits for a SOURCE-side paragraph — every enabled rule whose match touches this
	 *  paragraph index within `blockText`, found by OFFSET (correct even when the match spans a
	 *  paragraph boundary the splitter draws), never by re-scanning the paragraph's own text in
	 *  isolation (G9: that per-paragraph text scan missed 2 of 9 firing edits live). */
	function marginEditsForIndex(blockText: string, paraIndex: number): MarginEdit[] {
		return enabledRules
			.filter((r) => paragraphIndicesTouchedByRule(blockText, r).includes(paraIndex))
			.map((r) => ({ ruleId: r.id, label: deriveRuleTitle(r) }));
	}

	/** Margin edits for a paragraph that exists ONLY on the result side (a brand-new appended
	 *  paragraph/block, no source paragraph to key off) — attributed from the dry-run's
	 *  authoritative `firing_rule_ids` intersected with this specific paragraph's own content,
	 *  rather than either guessing from every firing rule or showing nothing at all (G9). */
	function resultMarginEdits(paragraphText: string): MarginEdit[] {
		return enabledRules
			.filter((r) => firingRuleIds == null || firingRuleIds.has(r.id))
			.filter((r) => {
				const after = ruleBeforeAfter(r).after.trim();
				return after.length > 0 && paragraphText.includes(after);
			})
			.map((r) => ({ ruleId: r.id, label: deriveRuleTitle(r) }));
	}

	function blockHeaderLabel(blockIndex: number | null, text: string): string {
		if (blockIndex === SESSION_BLOCK_INDEX) return 'Session message';
		if (blockIndex === null) return 'New content';
		const paras = splitBlockIntoDisplayParagraphs(text);
		const heading = paras.find(
			(p) => p.isHeading && p.headingTitle !== null && p.headingTitle.length > 0
		);
		if (heading?.headingTitle) return heading.headingTitle;
		const classified = classifySystemBlock({ text, index: blockIndex }, blockIndex);
		return classified.kind === 'other' ? 'Main prompt' : classified.label;
	}

	interface ParaItem {
		key: string;
		status: 'unchanged' | 'replaced' | 'dropped' | 'appended';
		segments: DiffSegment[] | null;
		text: string;
		quoteText: string;
		marginEdits: MarginEdit[];
		/** `${blockIndex}:${paragraphIndex}` when this paragraph maps to a real source paragraph. */
		scrollKey: string | null;
		/** For an `appended` paragraph only: true when it's new because of the PROPOSAL being
		 *  previewed, false when it's a real, already-applied edit that merely happens to render
		 *  while a preview is active (G8 — the old code coloured every appended paragraph blue
		 *  the instant any preview was on, real edit or not). Always `false` for any other status. */
		previewOnly: boolean;
	}

	interface BlockGroup {
		key: string;
		/** Null for a block with no stable source position (the "Final" lens, or a brand-new appended block). */
		blockIndex: number | null;
		headerLabel: string;
		dropped: boolean;
		preview: boolean;
		items: ParaItem[];
	}

	const isPreviewing = $derived(previewBlocks !== null || previewSessionMessage !== null);

	/** Source-side texts, with the session message threaded on as one more "block" (index
	 *  `SESSION_BLOCK_INDEX`) so it gets the exact same diffing/margin/preview machinery. */
	const sourceTexts = $derived([
		...blocks.map((b) => b.text),
		...(hasSessionMessage ? [sessionMessage] : [])
	]);
	const sourceIndices = $derived([
		...blocks.map((b) => b.index),
		...(hasSessionMessage ? [SESSION_BLOCK_INDEX] : [])
	]);

	/** What's actually shown/sent right now — preview applied on top when one is active. */
	const effectiveResultTexts = $derived([
		...((isPreviewing ? previewBlocks : resultBlocks) ?? blocks.map((b) => b.text)),
		...(hasSessionMessage
			? [
					isPreviewing
						? (previewSessionMessage ?? resultSessionMessage ?? sessionMessage)
						: (resultSessionMessage ?? sessionMessage)
				]
			: [])
	]);

	/** The current edits WITHOUT any preview — the baseline G8's two-level diff colours
	 *  real (green) edits against, independent of whatever proposal is being previewed. */
	const baselineResultTexts = $derived([
		...(resultBlocks ?? blocks.map((b) => b.text)),
		...(hasSessionMessage ? [resultSessionMessage ?? sessionMessage] : [])
	]);

	/** `diffBlocks` reports POSITIONS in `sourceTexts` (0, 1, 2, …), not real block indices —
	 *  translates a position back to the semantic index (`sourceIndices[i]`, `SESSION_BLOCK_INDEX`
	 *  for the appended session-message entry) used for display and for the `${blockIndex}:…`
	 *  scroll keys `FilterWorkbench.svelte` cross-links against. */
	function semanticBlockIndex(positionalIndex: number): number {
		return sourceIndices[positionalIndex] ?? positionalIndex;
	}

	function segmentClass(seg: DiffSegment): string {
		if (seg.status === 'removed') return 'wb-diff-removed';
		if (seg.status === 'added')
			return seg.preview === true ? 'wb-diff-added-preview' : 'wb-diff-added';
		return '';
	}

	const sourceGroups = $derived.by((): BlockGroup[] =>
		sourceIndices.map((blockIndex, i) => {
			const text = sourceTexts[i] ?? '';
			return {
				key: `src-${blockIndex}`,
				blockIndex,
				headerLabel: blockHeaderLabel(blockIndex, text),
				dropped: false,
				preview: false,
				items: splitBlockIntoDisplayParagraphs(text).map((p) => ({
					key: `${blockIndex}:${p.index}`,
					status: 'unchanged' as const,
					segments: null,
					text: p.text,
					quoteText: p.text,
					marginEdits: [],
					scrollKey: `${blockIndex}:${p.index}`,
					previewOnly: false
				}))
			};
		})
	);

	const resultGroups = $derived.by((): BlockGroup[] =>
		effectiveResultTexts.map((text, i) => ({
			key: `res-${i}`,
			blockIndex: null,
			headerLabel: blockHeaderLabel(null, text),
			dropped: false,
			preview: isPreviewing,
			items: splitBlockIntoDisplayParagraphs(text).map((p) => ({
				key: `result-${i}:${p.index}`,
				status: 'unchanged' as const,
				segments: null,
				text: p.text,
				quoteText: p.text,
				marginEdits: [],
				scrollKey: null,
				previewOnly: false
			}))
		}))
	);

	/** Rows from a diff of source vs the NON-preview baseline — looked up by `changeGroupFor`
	 *  to find what a block/paragraph looked like before the currently-previewed proposal, so
	 *  the preview-blue overlay (G8) only marks what the PROPOSAL adds. */
	const baselineRows = $derived.by((): DiffRow[] =>
		isPreviewing ? diffBlocks({ original: sourceTexts, sent: baselineResultTexts }) : []
	);

	function baselineTextFor(rows: DiffRow[], originalIndex: number, fallback: string): string {
		const row = rows.find(
			(r) =>
				(r.status === 'unchanged' || r.status === 'replaced' || r.status === 'dropped') &&
				r.originalIndex === originalIndex
		);
		if (row === undefined) return fallback;
		if (row.status === 'unchanged') return row.text;
		if (row.status === 'replaced') return row.sentText;
		return '';
	}

	function changeGroupFor(row: DiffRow, rowIndex: number): BlockGroup {
		if (row.status === 'unchanged') {
			const blockIndex = semanticBlockIndex(row.originalIndex);
			return {
				key: `chg-${rowIndex}`,
				blockIndex,
				headerLabel: blockHeaderLabel(blockIndex, row.text),
				dropped: false,
				preview: false,
				items: splitBlockIntoDisplayParagraphs(row.text).map((p) => ({
					key: `${blockIndex}:${p.index}`,
					status: 'unchanged',
					segments: null,
					text: p.text,
					quoteText: p.text,
					marginEdits: marginEditsForIndex(row.text, p.index),
					scrollKey: `${blockIndex}:${p.index}`,
					previewOnly: false
				}))
			};
		}
		if (row.status === 'dropped') {
			const blockIndex = semanticBlockIndex(row.originalIndex);
			return {
				key: `chg-${rowIndex}`,
				blockIndex,
				headerLabel: blockHeaderLabel(blockIndex, row.text),
				dropped: true,
				preview: false,
				items: splitBlockIntoDisplayParagraphs(row.text).map((p) => ({
					key: `${blockIndex}:${p.index}`,
					status: 'dropped',
					segments: null,
					text: p.text,
					quoteText: p.text,
					marginEdits: marginEditsForIndex(row.text, p.index),
					scrollKey: `${blockIndex}:${p.index}`,
					previewOnly: false
				}))
			};
		}
		if (row.status === 'appended') {
			// A whole new block also existing among the NON-preview baseline blocks is a real,
			// already-applied edit that happens to render while a preview is active elsewhere —
			// never preview-blue just because SOME preview is on (G8).
			const isRealAppend = baselineRows.some((r) => r.status === 'appended' && r.text === row.text);
			const previewOnly = isPreviewing && !isRealAppend;
			return {
				key: `chg-${rowIndex}`,
				blockIndex: null,
				headerLabel: blockHeaderLabel(null, row.text),
				dropped: false,
				preview: previewOnly,
				items: splitBlockIntoDisplayParagraphs(row.text).map((p, i) => ({
					key: `new-${rowIndex}:${i}`,
					status: 'appended',
					segments: null,
					text: p.text,
					quoteText: p.text,
					marginEdits: resultMarginEdits(p.text),
					scrollKey: null,
					previewOnly
				}))
			};
		}

		// replaced: diff the two sides' paragraphs against each other, the same way blocks
		// diff against each other one level up.
		const blockIndex = semanticBlockIndex(row.originalIndex);
		const sourceParas = splitBlockIntoDisplayParagraphs(row.originalText);
		const resultParas = splitBlockIntoDisplayParagraphs(row.sentText);
		const paraRows = diffBlocks({
			original: sourceParas.map((p) => p.text),
			sent: resultParas.map((p) => p.text)
		});

		// `baselineRows` came from the SAME `sourceTexts` diff as this function's own rows, so
		// it's looked up by the POSITIONAL `row.originalIndex` here, not the semantic `blockIndex`
		// above (which the session-message sentinel, `SESSION_BLOCK_INDEX`, would never match).
		const baselineBlockText = isPreviewing
			? baselineTextFor(baselineRows, row.originalIndex, row.originalText)
			: row.sentText;
		const baselineParas = splitBlockIntoDisplayParagraphs(baselineBlockText);
		const baselineParaRows =
			isPreviewing && baselineBlockText !== row.sentText
				? diffBlocks({
						original: sourceParas.map((p) => p.text),
						sent: baselineParas.map((p) => p.text)
					})
				: [];

		const items: ParaItem[] = paraRows.map((paraRow, pi) => {
			if (paraRow.status === 'unchanged') {
				return {
					key: `${blockIndex}:${paraRow.originalIndex}`,
					status: 'unchanged',
					segments: null,
					text: paraRow.text,
					quoteText: paraRow.text,
					marginEdits: marginEditsForIndex(row.originalText, paraRow.originalIndex),
					scrollKey: `${blockIndex}:${paraRow.originalIndex}`,
					previewOnly: false
				};
			}
			if (paraRow.status === 'dropped') {
				return {
					key: `${blockIndex}:${paraRow.originalIndex}`,
					status: 'dropped',
					segments: null,
					text: paraRow.text,
					quoteText: paraRow.text,
					marginEdits: marginEditsForIndex(row.originalText, paraRow.originalIndex),
					scrollKey: `${blockIndex}:${paraRow.originalIndex}`,
					previewOnly: false
				};
			}
			if (paraRow.status === 'appended') {
				const isRealAppend = baselineParas.some((bp) => bp.text === paraRow.text);
				return {
					key: `${blockIndex}:new-${pi}`,
					status: 'appended',
					segments: null,
					text: paraRow.text,
					quoteText: paraRow.text,
					marginEdits: resultMarginEdits(paraRow.text),
					scrollKey: null,
					previewOnly: isPreviewing && !isRealAppend
				};
			}
			const baselineParaText = isPreviewing
				? baselineTextFor(baselineParaRows, paraRow.originalIndex, paraRow.originalText)
				: paraRow.sentText;
			const segments =
				isPreviewing && baselineParaText !== paraRow.sentText
					? diffTextWithPreview(paraRow.originalText, baselineParaText, paraRow.sentText)
					: diffText(paraRow.originalText, paraRow.sentText);
			return {
				key: `${blockIndex}:${paraRow.originalIndex}`,
				status: 'replaced',
				segments,
				text: paraRow.originalText,
				quoteText: paraRow.originalText,
				marginEdits: marginEditsForIndex(row.originalText, paraRow.originalIndex),
				scrollKey: `${blockIndex}:${paraRow.originalIndex}`,
				previewOnly: false
			};
		});

		return {
			key: `chg-${rowIndex}`,
			blockIndex,
			headerLabel: blockHeaderLabel(blockIndex, row.originalText),
			dropped: false,
			preview: isPreviewing,
			items
		};
	}

	const changeGroups = $derived.by((): BlockGroup[] => {
		const rows = diffBlocks({ original: sourceTexts, sent: effectiveResultTexts });
		return rows.map((row, i) => changeGroupFor(row, i));
	});

	const groups = $derived(
		lens === 'source' ? sourceGroups : lens === 'result' ? resultGroups : changeGroups
	);
	const showMargin = $derived(lens === 'changes');

	let scrolledToBlock = $state<number | null>(null);
	$effect(() => {
		if (scrollToBlock === null || blocks.length === 0) return;
		if (scrolledToBlock === scrollToBlock) return;
		const target = paneEl?.querySelector(`[data-block-index="${scrollToBlock}"]`);
		if (target instanceof HTMLElement) {
			target.scrollIntoView({ behavior: 'smooth', block: 'start' });
			scrolledToBlock = scrollToBlock;
		}
	});

	let scrolledToParaKey = $state<string | null>(null);
	let highlightedParaKey = $state<string | null>(null);
	let highlightTimer: ReturnType<typeof setTimeout> | null = null;
	$effect(() => {
		if (scrollToParagraph === null) return;
		if (scrolledToParaKey === scrollToParagraph) return;
		const target = paneEl?.querySelector(`[data-para-key="${scrollToParagraph}"]`);
		if (target instanceof HTMLElement) {
			target.scrollIntoView({ behavior: 'smooth', block: 'center' });
			scrolledToParaKey = scrollToParagraph;
			highlightedParaKey = scrollToParagraph;
			if (highlightTimer !== null) clearTimeout(highlightTimer);
			highlightTimer = setTimeout(() => {
				highlightedParaKey = null;
				highlightTimer = null;
			}, 1200);
		}
	});
</script>

{#snippet paraBody(item: ParaItem)}
	{#if item.status === 'replaced' && item.segments !== null}
		<pre class="wb-para-text">{#each item.segments as seg, si (si)}<span class={segmentClass(seg)}
					>{seg.text}</span
				>{/each}</pre>
	{:else}
		<pre
			class="wb-para-text"
			class:wb-para-text--dropped={item.status === 'dropped'}
			class:wb-diff-added={item.status === 'appended' && !item.previewOnly}
			class:wb-diff-added-preview={item.status === 'appended' && item.previewOnly}>{item.text}</pre>
	{/if}
{/snippet}

<section
	class="wb-pane wb-pane--source"
	data-augmented-ui="border"
	aria-label="System prompt"
	data-slot="workbench-prompt-pane"
>
	<!-- svelte-ignore a11y_no_noninteractive_tabindex -- keyboard-focusable scroll region (axe scrollable-region-focusable); carries role=region + aria-label -->
	<div
		bind:this={paneEl}
		class="wb-pane-scroll"
		role="region"
		aria-label="System prompt blocks"
		tabindex="0"
		data-slot="workbench-source-blocks"
	>
		{#each groups as group (group.key)}
			<div class="wb-block" data-block-index={group.blockIndex ?? undefined}>
				<span
					class="wb-block-pill"
					class:wb-block-pill--dropped={group.dropped}
					class:wb-block-pill--preview={group.preview}
				>
					{group.headerLabel}
				</span>
				<div class="wb-block-paragraphs">
					{#if group.dropped}
						<div class="wb-block-dropped-banner">Block dropped</div>
					{/if}
					{#each group.items as item (item.key)}
						<div
							class="wb-para"
							class:wb-para--highlight={highlightedParaKey === item.scrollKey}
							data-para-key={item.scrollKey}
						>
							<div class="wb-para-main">
								{@render paraBody(item)}
							</div>
							<div class="wb-para-margin">
								{#if showMargin}
									{#each item.marginEdits as m (m.ruleId)}
										<button type="button" class="wb-edit-btn" onclick={() => onOpenEdit(m.ruleId)}>
											{m.label}
										</button>
									{/each}
								{/if}
								<button type="button" class="wb-ask-btn" onclick={() => onAskAbout(item.quoteText)}>
									Ask about this
								</button>
							</div>
						</div>
					{/each}
				</div>
			</div>
		{/each}
		{#if groups.length === 0}
			<p class="wb-empty-hint">
				{lens === 'result'
					? 'Nothing is sent — every block was dropped.'
					: 'No system prompt on this request.'}
			</p>
		{/if}
	</div>
</section>

<style>
	.wb-pane {
		--aug-border-all: 1px;
		--aug-border-bg: var(--border-default);
		display: flex;
		flex: 1;
		flex-direction: column;
		min-height: 0;
	}

	.wb-pane-scroll {
		flex: 1;
		min-height: 0;
		overflow-y: auto;
		padding: 14px 18px 40px;
		display: flex;
		flex-direction: column;
		gap: 14px;
	}

	.wb-block {
		display: flex;
		align-items: flex-start;
		gap: 12px;
	}

	.wb-block-paragraphs {
		flex: 1;
		min-width: 0;
		display: flex;
		flex-direction: column;
		gap: 10px;
	}

	.wb-para {
		display: flex;
		align-items: flex-start;
		gap: 12px;
		border-left: 2px solid transparent;
		padding-left: 6px;
		margin-left: -8px;
		transition:
			background 150ms ease,
			border-color 150ms ease;
	}

	.wb-para--highlight {
		border-left-color: var(--funk-cyan);
		background: var(--funk-cyan-dim);
	}

	.wb-para-main {
		flex: 1;
		min-width: 0;
	}

	.wb-para-margin {
		width: 150px;
		flex-shrink: 0;
		display: flex;
		flex-direction: column;
		align-items: flex-start;
		gap: 4px;
	}

	.wb-para:hover .wb-ask-btn {
		opacity: 1;
	}

	.wb-block-pill {
		display: inline-flex;
		align-items: center;
		justify-content: center;
		min-width: 20px;
		padding: 1px 4px;
		font: 500 10px var(--font-mono, 'JetBrains Mono Variable', monospace);
		font-variant-numeric: tabular-nums;
		border-radius: 3px;
		border: 1px solid var(--border-subtle);
		background: var(--surface-2);
		color: var(--text-3);
		flex-shrink: 0;
		margin-top: 2px;
	}
	.wb-block-pill--dropped {
		color: var(--status-danger-fg);
		background: var(--status-danger-bg);
		border-color: var(--status-danger-border);
	}
	.wb-block-pill--preview {
		color: var(--funk-blue);
		background: var(--funk-blue-dim);
		border-color: var(--funk-blue-border);
	}

	.wb-block-dropped-banner {
		font: 10px var(--font-mono, 'JetBrains Mono Variable', monospace);
		text-transform: uppercase;
		letter-spacing: 0.05em;
		color: var(--status-danger-fg);
	}

	.wb-para-text {
		flex: 1;
		margin: 0;
		padding: 0;
		font: 13px var(--font-mono, 'JetBrains Mono Variable', monospace);
		line-height: 1.7;
		color: var(--text-1);
		white-space: pre-wrap;
		word-break: break-word;
		overflow: visible;
	}

	.wb-para-text--dropped {
		opacity: 0.7;
		color: var(--text-3);
		text-decoration: line-through;
		text-decoration-color: var(--border-strong);
	}

	.wb-diff-removed {
		color: var(--text-3);
		text-decoration: line-through;
		text-decoration-color: var(--border-strong);
	}

	.wb-diff-added {
		background: var(--funk-green-dim);
		box-shadow: inset 0 -1px 0 var(--funk-green-border);
	}

	.wb-diff-added-preview {
		background: var(--funk-blue-dim);
		box-shadow: inset 0 -1px 0 var(--funk-blue-border);
	}

	.wb-edit-btn {
		max-width: 150px;
		text-align: left;
		padding: 3px 6px;
		background: transparent;
		border: 1px solid var(--border-default);
		color: var(--text-2);
		font: 10px var(--font-mono, 'JetBrains Mono Variable', monospace);
		line-height: 1.4;
		cursor: pointer;
	}
	.wb-edit-btn:hover {
		border-color: var(--funk-blue-border);
		color: var(--funk-blue);
	}
	.wb-edit-btn:focus-visible {
		outline: none;
		box-shadow: 0 0 0 2px var(--interactive-focus-ring);
	}

	.wb-ask-btn {
		opacity: 0;
		height: 22px;
		padding: 0 6px;
		background: transparent;
		border: 1px solid var(--funk-magenta-border);
		color: var(--funk-magenta);
		font: 10px var(--font-mono, 'JetBrains Mono Variable', monospace);
		cursor: pointer;
		transition: opacity 120ms ease;
	}
	.wb-ask-btn:focus-visible {
		opacity: 1;
		outline: none;
		box-shadow: 0 0 0 2px var(--interactive-focus-ring);
	}

	.wb-empty-hint {
		font: 11px var(--font-mono, 'JetBrains Mono Variable', monospace);
		color: var(--text-2);
		font-style: italic;
	}
</style>
