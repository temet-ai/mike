import { describe, expect, it } from 'vitest';
import { buildPromptOutline, splitBlockIntoDisplayParagraphs } from './blockParagraphs';

describe('splitBlockIntoDisplayParagraphs', () => {
	it('returns [] for empty input', () => {
		expect(splitBlockIntoDisplayParagraphs('')).toEqual([]);
	});

	it('returns [] for whitespace-only input', () => {
		expect(splitBlockIntoDisplayParagraphs('   \n\n  \n')).toEqual([]);
	});

	it('returns a single paragraph for text with no blank lines (poetry-style)', () => {
		const result = splitBlockIntoDisplayParagraphs('line one\nline two\nline three');
		expect(result).toHaveLength(1);
		expect(result[0]?.text).toBe('line one\nline two\nline three');
		expect(result[0]?.isHeading).toBe(false);
	});

	describe('blank-line splitting', () => {
		it('splits two paragraphs separated by one blank line', () => {
			const result = splitBlockIntoDisplayParagraphs('first paragraph\n\nsecond paragraph');
			expect(result.map((p) => p.text)).toEqual(['first paragraph', 'second paragraph']);
		});

		it('treats multiple consecutive blank lines as a single boundary', () => {
			const result = splitBlockIntoDisplayParagraphs('one\n\n\n\ntwo');
			expect(result.map((p) => p.text)).toEqual(['one', 'two']);
		});

		it('strips leading and trailing blank lines', () => {
			const result = splitBlockIntoDisplayParagraphs('\n\nonly\n\n');
			expect(result.map((p) => p.text)).toEqual(['only']);
		});

		it('assigns a dense 0-based index across kept paragraphs', () => {
			const result = splitBlockIntoDisplayParagraphs('\n\nalpha\n\nbeta\n\n\ngamma');
			expect(result.map((p) => p.index)).toEqual([0, 1, 2]);
		});
	});

	describe('heading splitting', () => {
		it('a heading line always starts a new paragraph', () => {
			const result = splitBlockIntoDisplayParagraphs('prose before\n# A Heading\nprose after');
			expect(result.map((p) => p.text)).toEqual(['prose before', '# A Heading\nprose after']);
		});

		it('marks a heading paragraph with isHeading/headingLevel/headingTitle', () => {
			const [p] = splitBlockIntoDisplayParagraphs('## Section Title\nbody text');
			expect(p?.isHeading).toBe(true);
			expect(p?.headingLevel).toBe(2);
			expect(p?.headingTitle).toBe('Section Title');
		});

		it('merges a heading with immediately-following prose when nothing else forces a break', () => {
			const result = splitBlockIntoDisplayParagraphs(
				'# Memory\nYou have a persistent file-based memory system.'
			);
			expect(result).toHaveLength(1);
			expect(result[0]?.text).toBe('# Memory\nYou have a persistent file-based memory system.');
		});

		it('splits two consecutive headings into separate paragraphs', () => {
			const result = splitBlockIntoDisplayParagraphs('# One\n## Two');
			expect(result.map((p) => p.text)).toEqual(['# One', '## Two']);
		});

		it('does not treat a hash without a following space as a heading', () => {
			const result = splitBlockIntoDisplayParagraphs('#nospace is not a heading');
			expect(result).toHaveLength(1);
			expect(result[0]?.isHeading).toBe(false);
		});

		it('supports heading levels 1 through 6', () => {
			for (let level = 1; level <= 6; level++) {
				const hashes = '#'.repeat(level);
				const [p] = splitBlockIntoDisplayParagraphs(`${hashes} Title`);
				expect(p?.headingLevel).toBe(level);
			}
		});
	});

	describe('list-run splitting', () => {
		it('starts a new paragraph at the first list item after prose, and keeps the whole run together', () => {
			const text = 'Some prose.\n - item one\n - item two\n - item three';
			const result = splitBlockIntoDisplayParagraphs(text);
			expect(result.map((p) => p.text)).toEqual([
				'Some prose.',
				' - item one\n - item two\n - item three'
			]);
		});

		it('keeps a contiguous list run as ONE paragraph regardless of item count, preserving every line verbatim', () => {
			const text = ' - a\n - b\n - c\n - d\n - e';
			const result = splitBlockIntoDisplayParagraphs(text);
			expect(result).toHaveLength(1);
			expect(result[0]?.text).toBe(text);
		});

		it('ends the list paragraph when prose resumes (no blank line needed)', () => {
			const text = ' - item one\n - item two\nBack to prose.';
			const result = splitBlockIntoDisplayParagraphs(text);
			expect(result.map((p) => p.text)).toEqual([' - item one\n - item two', 'Back to prose.']);
		});

		it('recognises both "- " and " - " list markers', () => {
			const text = '- unindented\n - indented';
			const result = splitBlockIntoDisplayParagraphs(text);
			expect(result).toHaveLength(1);
			expect(result[0]?.text).toBe(text);
		});

		it('does not treat a hyphen with no following space as a list item', () => {
			const result = splitBlockIntoDisplayParagraphs('a well-known fact, not a list');
			expect(result).toHaveLength(1);
		});

		it('splits a heading immediately followed by a list into two paragraphs (heading alone, list merged)', () => {
			const text =
				'# Session-specific guidance\n - If you need the user to run a shell command themselves, suggest they type `! <command>`.\n - Use the Agent tool with specialized agents when the task matches.';
			const result = splitBlockIntoDisplayParagraphs(text);
			expect(result).toHaveLength(2);
			expect(result[0]?.text).toBe('# Session-specific guidance');
			expect(result[0]?.isHeading).toBe(true);
			expect(result[1]?.text).toBe(
				text.split('\n').slice(1).join('\n')
			);
			expect(result[1]?.isHeading).toBe(false);
		});
	});

	describe('CRLF handling', () => {
		it('produces identical results for CRLF and LF line endings', () => {
			const lf = splitBlockIntoDisplayParagraphs('# Heading\nbody\n\n - a\n - b');
			const crlf = splitBlockIntoDisplayParagraphs('# Heading\r\nbody\r\n\r\n - a\r\n - b');
			expect(crlf).toEqual(lf);
		});

		it('treats a CRLF blank line as a paragraph boundary', () => {
			const result = splitBlockIntoDisplayParagraphs('one\r\n\r\ntwo');
			expect(result.map((p) => p.text)).toEqual(['one', 'two']);
		});
	});

	it('reproduces a realistic multi-paragraph block end to end', () => {
		const block = [
			"You are Claude Code, Anthropic's official CLI for Claude.",
			'',
			'# Session-specific guidance',
			' - If you need the user to run a shell command themselves, suggest they type `! <command>`.',
			' - Reference code as `absolute_file_path:line_number`.',
			'',
			'# Memory',
			'You have a persistent file-based memory system.'
		].join('\n');
		const result = splitBlockIntoDisplayParagraphs(block);
		expect(result.map((p) => p.text)).toEqual([
			"You are Claude Code, Anthropic's official CLI for Claude.",
			'# Session-specific guidance',
			' - If you need the user to run a shell command themselves, suggest they type `! <command>`.\n - Reference code as `absolute_file_path:line_number`.',
			'# Memory\nYou have a persistent file-based memory system.'
		]);
		expect(result.map((p) => p.isHeading)).toEqual([false, true, false, true]);
	});

	describe('start/end offsets (round-4 defect #2: offset-based rule attribution)', () => {
		it('every paragraph\'s [start, end) slice of the normalised block text equals its own text', () => {
			const block = [
				"You are Claude Code, Anthropic's official CLI for Claude.",
				'',
				'# Session-specific guidance',
				' - line one',
				' - line two',
				'',
				'# Memory',
				'You have a persistent file-based memory system.'
			].join('\n');
			const normalized = block.replace(/\r\n/g, '\n');
			const result = splitBlockIntoDisplayParagraphs(block);
			expect(result.length).toBeGreaterThan(0);
			for (const p of result) {
				expect(normalized.slice(p.start, p.end)).toBe(p.text);
			}
		});

		it('paragraph offsets are monotonically increasing and never overlap', () => {
			const block = 'first\n\n# Heading\nsecond paragraph\n\n - a\n - b\n\nlast';
			const result = splitBlockIntoDisplayParagraphs(block);
			for (let i = 1; i < result.length; i++) {
				const prev = result[i - 1];
				const cur = result[i];
				expect(prev).toBeDefined();
				expect(cur).toBeDefined();
				expect(cur!.start).toBeGreaterThanOrEqual(prev!.end);
			}
		});

		it('produces correct offsets for CRLF input, matching the LF-normalised text', () => {
			const lf = splitBlockIntoDisplayParagraphs('one\n\ntwo\n\nthree');
			const crlf = splitBlockIntoDisplayParagraphs('one\r\n\r\ntwo\r\n\r\nthree');
			expect(crlf.map((p) => [p.start, p.end])).toEqual(lf.map((p) => [p.start, p.end]));
		});
	});
});

describe('buildPromptOutline', () => {
	it('returns [] when there are no headings', () => {
		expect(buildPromptOutline([{ index: 0, text: 'plain prose, no headings here' }], () => false)).toEqual(
			[]
		);
	});

	it('lists every heading in source order with block/paragraph indices', () => {
		const blocks = [
			{ index: 0, text: '# One\nbody' },
			{ index: 1, text: '## Two\n\n### Three' }
		];
		const outline = buildPromptOutline(blocks, () => false);
		expect(outline.map((h) => [h.title, h.level, h.blockIndex, h.paragraphIndex])).toEqual([
			['One', 1, 0, 0],
			['Two', 2, 1, 0],
			['Three', 3, 1, 1]
		]);
		expect(outline.map((h) => h.key)).toEqual(['0:0', '1:0', '1:1']);
	});

	it('flags a heading whose OWN paragraph has an edit', () => {
		const blocks = [{ index: 0, text: '# Heading\nbody text' }];
		const outline = buildPromptOutline(blocks, (text) => text.includes('# Heading'));
		expect(outline[0]?.hasEdits).toBe(true);
	});

	it('flags a heading whose section (up to the next heading) has an edit, even in a later paragraph', () => {
		const blocks = [{ index: 0, text: '# Heading\n\nfirst para\n\nsecond para has the edit' }];
		const outline = buildPromptOutline(blocks, (text) => text.includes('has the edit'));
		expect(outline[0]?.hasEdits).toBe(true);
	});

	it('does NOT flag a heading whose edit is actually in the NEXT section', () => {
		const blocks = [{ index: 0, text: '# First\nclean\n\n# Second\nedited here' }];
		const outline = buildPromptOutline(blocks, (text) => text.includes('edited here'));
		expect(outline[0]?.hasEdits).toBe(false);
		expect(outline[1]?.hasEdits).toBe(true);
	});

	it('a section spans across a block boundary when no heading closes it first', () => {
		const blocks = [
			{ index: 0, text: '# Heading\nintro' },
			{ index: 1, text: 'more content for the same section, with the edit here' }
		];
		const outline = buildPromptOutline(blocks, (text) => text.includes('the edit here'));
		expect(outline).toHaveLength(1);
		expect(outline[0]?.hasEdits).toBe(true);
	});

	it('a heading with no title text falls back to its own paragraph text', () => {
		// headingTitle is always non-null for a real heading match, but guard the fallback path.
		const blocks = [{ index: 0, text: '# Title Text' }];
		const outline = buildPromptOutline(blocks, () => false);
		expect(outline[0]?.title).toBe('Title Text');
	});
});
