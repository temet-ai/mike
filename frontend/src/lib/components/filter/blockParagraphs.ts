/**
 * Splits a system-prompt block into paragraphs for DISPLAY: the unit the prompt pane diffs,
 * attributes edits to, and lets the operator quote into the assistant. A "block" (an item of
 * `system_blocks.blocks`) is often one big chunk of unrelated guidance — main-session block 4
 * routinely runs 8+ separately-edited paragraphs — so per-block granularity is too coarse for
 * "manipulate a part of the prompt."
 *
 * Boundary rules, applied line by line over the block text:
 *  - A blank line always ends the current paragraph (as before).
 *  - A markdown heading line (`#` … `######`, followed by a space) always starts a new
 *    paragraph. It does not forcibly end its own paragraph — prose immediately following it
 *    with no blank line in between continues to accumulate into the SAME paragraph, unless
 *    that next line itself triggers a boundary (another heading, a blank line, or the start of
 *    a list run).
 *  - A list-run START (a line matching `- ` / ` - `, i.e. optional leading whitespace then a
 *    literal hyphen and a space, when the immediately preceding accumulated line was NOT
 *    itself a list line) always starts a new paragraph. Once inside a list run, every
 *    subsequent list line keeps appending to that SAME paragraph — a contiguous run of bullets
 *    is one paragraph, never split per item. Leaving the list run (a non-blank, non-list line
 *    after list lines) also starts a new paragraph, so a list's paragraph never mixes with
 *    surrounding prose.
 *
 * Net effect: a heading is its own one-line paragraph when a list or another heading
 * immediately follows it (no blank line), but stays merged with immediately-following prose
 * (e.g. "# Memory\nYou have a persistent file-based memory…") when nothing else forces a break.
 */

export interface DisplayParagraph {
	index: number;
	text: string;
	isHeading: boolean;
	headingLevel: number | null;
	/** The heading text with the leading `#`s and the following whitespace stripped. */
	headingTitle: string | null;
	/**
	 * This paragraph's `[start, end)` character range in the block's own text, normalised to
	 * `\n` line endings (matching what `findRuleMatchSpan` in `rule-matching.ts` searches). Lets
	 * a rule's match offset be mapped back to the paragraph it falls in even when the match
	 * itself spans a boundary this splitter would otherwise draw between two paragraphs (a
	 * heading immediately followed by a blank line then its body, for example).
	 */
	start: number;
	end: number;
}

const HEADING_LINE = /^(#{1,6})\s+(\S.*)$/;
const LIST_ITEM_LINE = /^\s*-\s+\S/;

interface LineRecord {
	text: string;
	start: number;
	end: number;
}

export function splitBlockIntoDisplayParagraphs(text: string): DisplayParagraph[] {
	const normalized = text.replace(/\r\n/g, '\n');
	const rawLines = normalized.split('\n');

	const lines: LineRecord[] = [];
	let cursor = 0;
	for (const line of rawLines) {
		lines.push({ text: line, start: cursor, end: cursor + line.length });
		cursor += line.length + 1; // +1 for the '\n' consumed by split, absent after the last line
	}

	const groups: LineRecord[][] = [];
	let current: LineRecord[] = [];
	let inList = false;

	function flush(): void {
		if (current.length > 0) groups.push(current);
		current = [];
		inList = false;
	}

	for (const line of lines) {
		if (line.text.trim().length === 0) {
			flush();
			continue;
		}
		if (HEADING_LINE.test(line.text)) {
			flush();
			current.push(line);
			continue;
		}
		if (LIST_ITEM_LINE.test(line.text)) {
			if (!inList) flush();
			inList = true;
			current.push(line);
			continue;
		}
		// Plain prose line: leaving a list run is its own boundary.
		if (inList) flush();
		current.push(line);
	}
	flush();

	return finish(groups);
}

function finish(groups: LineRecord[][]): DisplayParagraph[] {
	const out: DisplayParagraph[] = [];
	for (const group of groups) {
		// No trim(): every group already starts and ends on a non-blank line (blank lines
		// always flush before being added), so trimming here would only strip a real leading
		// space from the group's first line — e.g. eating the leading space off " - item one"
		// while every other bullet in the same run keeps its " - ". Preserve it verbatim.
		const joined = group.map((l) => l.text).join('\n');
		if (joined.length === 0) continue;
		// A heading paragraph's FIRST line is the heading; later lines (merged prose) must not
		// be checked against the pattern too, and must not stop `.match` from finding it — the
		// regex has no `m`/`s` flags, so it's run against line 0 alone, not the whole joined text.
		const headingMatch = (group[0]?.text ?? '').match(HEADING_LINE);
		const firstLine = group[0];
		const lastLine = group[group.length - 1];
		out.push({
			index: out.length,
			text: joined,
			isHeading: headingMatch !== null,
			headingLevel: headingMatch ? (headingMatch[1]?.length ?? null) : null,
			headingTitle: headingMatch ? (headingMatch[2]?.trim() ?? null) : null,
			start: firstLine?.start ?? 0,
			end: lastLine?.end ?? 0
		});
	}
	return out;
}

export interface PromptOutlineHeading {
	/** `${blockIndex}:${paragraphIndex}` — matches the paragraph's DOM scroll-target key. */
	key: string;
	blockIndex: number;
	paragraphIndex: number;
	level: number;
	title: string;
	/** True when this heading's own paragraph, or any paragraph before the next heading, has an edit. */
	hasEdits: boolean;
}

/**
 * The outline's "In this prompt" list: every markdown heading across every block's paragraphs,
 * in source order, each flagged with whether an edit touches its section (this heading's
 * paragraph up to, but not including, the next heading — spanning block boundaries if the
 * section isn't closed by a heading before the block ends). `paragraphHasEdit` is injected
 * rather than importing rule-matching here, so this module stays dependency-free and testable
 * without a `FilterRuleDto` fixture.
 */
export function buildPromptOutline(
	blocks: readonly { index: number; text: string }[],
	paragraphHasEdit: (paragraphText: string) => boolean
): PromptOutlineHeading[] {
	interface FlatParagraph {
		blockIndex: number;
		paragraphIndex: number;
		para: DisplayParagraph;
	}

	const flat: FlatParagraph[] = [];
	for (const block of blocks) {
		for (const para of splitBlockIntoDisplayParagraphs(block.text)) {
			flat.push({ blockIndex: block.index, paragraphIndex: para.index, para });
		}
	}

	const headings: PromptOutlineHeading[] = [];
	for (let i = 0; i < flat.length; i++) {
		const fp = flat[i];
		if (fp === undefined || !fp.para.isHeading) continue;

		let hasEdits = paragraphHasEdit(fp.para.text);
		for (let j = i + 1; j < flat.length; j++) {
			const next = flat[j];
			if (next === undefined || next.para.isHeading) break;
			if (paragraphHasEdit(next.para.text)) {
				hasEdits = true;
				break;
			}
		}

		headings.push({
			key: `${fp.blockIndex}:${fp.paragraphIndex}`,
			blockIndex: fp.blockIndex,
			paragraphIndex: fp.paragraphIndex,
			level: fp.para.headingLevel ?? 1,
			title: fp.para.headingTitle ?? fp.para.text,
			hasEdits
		});
	}
	return headings;
}
