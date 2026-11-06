import { describe, expect, it } from 'vitest';
import { parseSimpleMarkdown } from './simpleMarkdown';

describe('parseSimpleMarkdown', () => {
	it('parses a heading', () => {
		const blocks = parseSimpleMarkdown('## Template to follow exactly');
		expect(blocks).toEqual([
			{
				kind: 'heading',
				level: 2,
				segments: [{ text: 'Template to follow exactly', bold: false, code: false, href: null }]
			}
		]);
	});

	it('parses an ordered list item with inline bold', () => {
		const blocks = parseSimpleMarkdown('1. **Interactive pair.** Find the request.');
		expect(blocks).toEqual([
			{
				kind: 'ordered',
				marker: '1.',
				segments: [
					{ text: 'Interactive pair.', bold: true, code: false, href: null },
					{ text: ' Find the request.', bold: false, code: false, href: null }
				]
			}
		]);
	});

	it('parses a bullet list item', () => {
		const blocks = parseSimpleMarkdown('- report.md');
		expect(blocks).toEqual([
			{ kind: 'bullet', segments: [{ text: 'report.md', bold: false, code: false, href: null }] }
		]);
	});

	it('parses inline code as a distinct segment', () => {
		const blocks = parseSimpleMarkdown('Read `report.md` in full.');
		expect(blocks).toEqual([
			{
				kind: 'paragraph',
				segments: [
					{ text: 'Read ', bold: false, code: false, href: null },
					{ text: 'report.md', bold: false, code: true, href: null },
					{ text: ' in full.', bold: false, code: false, href: null }
				]
			}
		]);
	});

	it('collects a fenced code block verbatim, without stripping content', () => {
		const blocks = parseSimpleMarkdown('```\nline one\nline two\n```');
		expect(blocks).toEqual([{ kind: 'code', text: 'line one\nline two' }]);
	});

	it('skips blank lines between blocks', () => {
		const blocks = parseSimpleMarkdown('one\n\ntwo');
		expect(blocks).toHaveLength(2);
	});

	it('never drops or truncates plain paragraph text', () => {
		const long = 'x'.repeat(5000);
		const blocks = parseSimpleMarkdown(long);
		expect(blocks).toEqual([
			{ kind: 'paragraph', segments: [{ text: long, bold: false, code: false, href: null }] }
		]);
	});

	it('parses a markdown link as a distinct href-carrying segment', () => {
		const blocks = parseSimpleMarkdown('See [the docs](https://example.com/docs) for more.');
		expect(blocks).toEqual([
			{
				kind: 'paragraph',
				segments: [
					{ text: 'See ', bold: false, code: false, href: null },
					{ text: 'the docs', bold: false, code: false, href: 'https://example.com/docs' },
					{ text: ' for more.', bold: false, code: false, href: null }
				]
			}
		]);
	});

	it('keeps a link with an unsafe or relative target as literal text', () => {
		for (const source of ['[click](javascript:alert(1))', '[x](data:text/html,hi)', '[rel](/filter)']) {
			expect(parseSimpleMarkdown(source)).toEqual([
				{ kind: 'paragraph', segments: [{ text: source, bold: false, code: false, href: null }] }
			]);
		}
	});

	it('does not parse a link inside a code span as a link', () => {
		const blocks = parseSimpleMarkdown('`[not a link](x)`');
		expect(blocks).toEqual([
			{
				kind: 'paragraph',
				segments: [{ text: '[not a link](x)', bold: false, code: true, href: null }]
			}
		]);
	});

	it('does not throw on an unclosed fence mid-stream, and keeps the partial content as a code block (a streamed assistant reply can arrive with an in-progress ``` fence before the closing fence has streamed in yet)', () => {
		const partial = 'Here is a snippet:\n```js\nconst x = 1;\nfunction f() {';
		expect(() => parseSimpleMarkdown(partial)).not.toThrow();
		const blocks = parseSimpleMarkdown(partial);
		expect(blocks).toEqual([
			{ kind: 'paragraph', segments: [{ text: 'Here is a snippet:', bold: false, code: false, href: null }] },
			{ kind: 'code', text: 'const x = 1;\nfunction f() {' }
		]);
	});
});
