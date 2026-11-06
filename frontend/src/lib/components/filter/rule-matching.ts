import type { FilterKind, FilterRuleDto } from '$lib/types';
import { splitBlockIntoDisplayParagraphs, type DisplayParagraph } from './blockParagraphs';
import { classifySystemBlock } from '$lib/components/shell/systemBlockKinds';

export const PREFILL_TEXT_SNIPPET_MAX = 60;

const XML_OPEN_TAG = /^<([\w-]+)[\s>]/;

export function prefillFrom(text: string): { kind: FilterKind; value: string } {
	const trimmed = text.trim();
	const xmlMatch = trimmed.match(XML_OPEN_TAG);
	if (xmlMatch && xmlMatch[1]) {
		return { kind: 'xml_tag', value: xmlMatch[1] };
	}
	return { kind: 'text_substring', value: trimmed };
}

export function matches(kind: FilterKind, value: string, block: string): boolean {
	if (value.length === 0) return false;
	if (kind === 'text_substring') {
		return block.toLowerCase().includes(value.toLowerCase());
	}
	if (kind === 'lazy_section') {
		const heading = value.trim();
		if (heading.length === 0) return false;
		return block.split(/\r\n|\n/).some((line) => line.trim() === heading);
	}
	const trimmed = block.trim();
	const openBare = `<${value}>`;
	const openWithAttr = `<${value} `;
	const closeTag = `</${value}>`;
	const opens = trimmed.startsWith(openBare) || trimmed.startsWith(openWithAttr);
	return opens && trimmed.endsWith(closeTag);
}

export function splitBlockIntoParagraphs(text: string): { index: number; text: string }[] {
	const normalised = text.replace(/\r\n/g, '\n');
	const parts = normalised.split(/\n(?:[ \t]*\n)+/);
	const out: { index: number; text: string }[] = [];
	for (const raw of parts) {
		const trimmed = raw.trim();
		if (trimmed.length === 0) continue;
		out.push({ index: out.length, text: trimmed });
	}
	return out;
}

export function splitBlockIntoLines(text: string): { index: number; text: string }[] {
	const lines = text.split(/\r\n|\n/);
	const out: { index: number; text: string }[] = [];
	for (const raw of lines) {
		const trimmed = raw.trim();
		if (trimmed.length === 0) continue;
		out.push({ index: out.length, text: trimmed });
	}
	return out;
}

const TITLE_WORD_LIMIT = 8;

function firstWords(text: string, limit = TITLE_WORD_LIMIT): string {
	const collapsed = text.trim().replace(/\s+/g, ' ');
	if (collapsed.length === 0) return '';
	const words = collapsed.split(' ');
	if (words.length <= limit) return collapsed;
	return `${words.slice(0, limit).join(' ')}…`;
}

/**
 * The plain-language title shown for an edit row (spec 4.3): `rule.title` when the
 * operator set one, else a title derived from the rule's shape. Never a rule id or
 * an internal action code — those never appear in visible text.
 */
export function deriveRuleTitle(
	rule: Pick<FilterRuleDto, 'title' | 'kind' | 'action' | 'value'>
): string {
	if (rule.title !== null && rule.title.trim().length > 0) return rule.title.trim();
	const snippet = firstWords(rule.value);
	if (rule.kind === 'inject_reminder') return `Reminds “${snippet}”`;
	if (rule.action === 'drop') return `Removes “${snippet}”`;
	if (rule.action === 'replace') return `Rewrites “${snippet}”`;
	return `Adds after “${snippet}”`;
}

/**
 * Whether a specific PARAGRAPH (a fragment of a block, per `blockParagraphs.ts`) is the one a
 * rule's edit attaches to — the paragraph whose source text contains the rule's `value`. This
 * also covers append-type rules with no special-casing: for `append`, `rule.value` already IS
 * the anchor text the rule inserts content after, so "contains the value" and "contains the
 * anchor" are the same check.
 *
 * This is a per-paragraph heuristic: exact for a match that falls entirely inside one
 * paragraph, but it MISSES a match whose text spans a paragraph boundary this splitter draws
 * (a heading immediately followed by a blank line then its body, for example — since the two
 * sides of that boundary are never joined into one paragraph's `.text`, no single paragraph's
 * substring check ever succeeds). `findRuleMatchSpan` + `paragraphIndicesTouchedByRule` below
 * search the whole block instead and are the correct primitive for "does this rule's edit
 * touch the prompt at all"; this function stays for call sites that already have one
 * paragraph's text in hand and only need a same-paragraph check (e.g. highlighting).
 */
export function paragraphContainsRule(
	paragraphText: string,
	rule: Pick<FilterRuleDto, 'kind' | 'value'>
): boolean {
	if (rule.kind === 'inject_reminder') return false;
	if (rule.value.trim().length === 0) return false;
	if (rule.kind === 'xml_tag') {
		return paragraphText.toLowerCase().includes(`<${rule.value}`.toLowerCase());
	}
	return paragraphText.toLowerCase().includes(rule.value.toLowerCase());
}

/**
 * Finds where a `text_substring` rule's `value` starts in a block's own text (normalised to
 * `\n` line endings, matching `DisplayParagraph.start`/`end`). Returns `null` for a rule kind
 * whose match isn't a plain substring search (`xml_tag`/`lazy_section` are checked against the
 * WHOLE block by `matches()`, not one span within it) or when nothing matches.
 */
export function findRuleMatchSpan(
	blockText: string,
	rule: Pick<FilterRuleDto, 'kind' | 'value'>
): { start: number; end: number } | null {
	if (rule.kind !== 'text_substring') return null;
	if (rule.value.length === 0) return null;
	const normalized = blockText.replace(/\r\n/g, '\n');
	const idx = normalized.toLowerCase().indexOf(rule.value.toLowerCase());
	if (idx === -1) return null;
	return { start: idx, end: idx + rule.value.length };
}

/**
 * Every paragraph (in source order) a rule's edit touches within one block — the offset-based
 * replacement for scanning paragraph-by-paragraph with `paragraphContainsRule`, correct even
 * when the match spans more than one paragraph (round-4 defect: rule 70's replaced value spans
 * a heading, a blank line, and the paragraph after it — `paragraphContainsRule` finds it in
 * none of the three, since none of their individual `.text` values contain the whole span).
 * Falls back to the per-paragraph heuristic for `xml_tag`/`lazy_section`, which `findRuleMatchSpan`
 * deliberately doesn't handle (see its own doc comment).
 */
export function paragraphIndicesTouchedByRule(
	blockText: string,
	rule: Pick<FilterRuleDto, 'kind' | 'value'>
): number[] {
	const paragraphs = splitBlockIntoDisplayParagraphs(blockText);
	const span = findRuleMatchSpan(blockText, rule);
	if (span === null) {
		return paragraphs.filter((p) => paragraphContainsRule(p.text, rule)).map((p) => p.index);
	}
	return paragraphs.filter((p) => p.start < span.end && span.start < p.end).map((p) => p.index);
}

/** The before/after pair shown when an edit row is open (spec 4.3). */
export function ruleBeforeAfter(
	rule: Pick<FilterRuleDto, 'kind' | 'action' | 'value' | 'text'>
): { before: string; after: string } {
	if (rule.kind === 'inject_reminder') return { before: '', after: rule.text ?? '' };
	if (rule.action === 'drop') return { before: rule.value, after: '' };
	if (rule.action === 'replace') return { before: rule.value, after: rule.text ?? '' };
	return { before: '', after: rule.text ?? '' };
}

/**
 * "Where it lands" (spec 4.3; regrouped 2026-09-23 so this label is a GROUP HEADING for
 * rules already known to fire on the open request, never a per-row verdict on whether a
 * rule applies — that decision now lives in `ruleGrouping.ts`, upstream of this
 * function). A WORDED description of where the edit attaches, never a block number,
 * never "Not in this prompt" (this function's caller only ever invokes it for a rule
 * that fires here). In order:
 *   1. The nearest markdown heading AT OR BEFORE the paragraph the edit attributes to, found
 *      by searching the rule's match OFFSET within one block at a time
 *      (`paragraphIndicesTouchedByRule`) rather than scanning paragraph text for containment —
 *      correct even when the match spans a paragraph boundary the splitter draws (a heading
 *      immediately followed by a blank line then its body). The backward heading search never
 *      leaves the block the match was found in, so a block with no heading above the match
 *      never borrows one from an earlier block.
 *   2. If nothing precedes it under any heading in that block, the block's own classification
 *      (`classifySystemBlock`, the same "Identity"/"Instructions"/"Billing header" labels
 *      already shown elsewhere in the app) rather than inventing a second vocabulary for
 *      the same blocks.
 *   3. "Somewhere in this prompt" when the rule is known to fire (the caller guarantees this)
 *      but this client-side span search couldn't locate it — a gap between the client's
 *      substring matching and the server's real matcher, never evidence of absence.
 */
export function ruleLandingLabel(
	rule: Pick<FilterRuleDto, 'id' | 'kind' | 'value' | 'trigger_token'>,
	blockTexts: readonly string[]
): string {
	if (rule.kind === 'inject_reminder') {
		return rule.trigger_token
			? `Injected when the model emits ${rule.trigger_token}`
			: 'Injected on trigger';
	}
	if (blockTexts.length === 0) return 'No prompt open';

	for (let blockIndex = 0; blockIndex < blockTexts.length; blockIndex++) {
		const blockText = blockTexts[blockIndex] ?? '';
		const touched = paragraphIndicesTouchedByRule(blockText, rule);
		if (touched.length === 0) continue;

		const paragraphs = splitBlockIntoDisplayParagraphs(blockText);
		const firstTouched = touched[0] ?? 0;
		for (let i = firstTouched; i >= 0; i--) {
			const para: DisplayParagraph | undefined = paragraphs[i];
			if (para?.isHeading && para.headingTitle !== null && para.headingTitle.length > 0) {
				return para.headingTitle;
			}
		}

		const classified = classifySystemBlock({ text: blockText, index: blockIndex }, blockIndex);
		return classified.kind === 'other' ? 'Main prompt' : classified.label;
	}

	return 'Somewhere in this prompt';
}
