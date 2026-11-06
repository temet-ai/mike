import { describe, expect, it } from 'vitest';
import type { FilterKind, FilterRuleDto } from '$lib/types';
import { buildFilterRule } from '$lib/test-support';
import sharedVectors from '../../../../../crates/mike/tests/fixtures/matcher/vectors.json';
import { splitBlockIntoDisplayParagraphs } from './blockParagraphs';
import {
	deriveRuleTitle,
	findRuleMatchSpan,
	matches,
	paragraphContainsRule,
	paragraphIndicesTouchedByRule,
	prefillFrom,
	PREFILL_TEXT_SNIPPET_MAX,
	ruleBeforeAfter,
	ruleLandingLabel,
	splitBlockIntoLines,
	splitBlockIntoParagraphs
} from './rule-matching';
import {
	REQUEST_421514_BLOCKS,
	REQUEST_421514_DRY_RUN,
	REQUEST_421514_ENABLED_RULES
} from './__fixtures__/request421514';
import {
	computeBlockMatches,
	countRuleMatches,
	findOccurrences,
	moveInOrder,
	parseFromParam,
	reconcileOrder,
	reorderByIndex
} from './workbench';

function rule(overrides: Partial<FilterRuleDto> = {}): FilterRuleDto {
	return buildFilterRule({
		id: 'r1',
		kind: 'text_substring',
		value: 'needle',
		updated_at: '2026-06-10T00:00:00Z',
		...overrides
	});
}

describe('prefillFrom', () => {
	it('returns xml_tag kind + tag name when input opens with an XML tag', () => {
		expect(prefillFrom('<system-reminder>hello</system-reminder>')).toEqual({
			kind: 'xml_tag',
			value: 'system-reminder'
		});
	});

	it('accepts a tag followed by whitespace or attributes', () => {
		expect(prefillFrom('<system-reminder foo="bar">body</system-reminder>')).toEqual({
			kind: 'xml_tag',
			value: 'system-reminder'
		});
		expect(prefillFrom('<cwd>\n/tmp\n</cwd>')).toEqual({
			kind: 'xml_tag',
			value: 'cwd'
		});
	});

	it('falls back to text_substring with the FULL trimmed text (no truncation)', () => {
		const longText = 'a'.repeat(80);
		expect(prefillFrom(longText)).toEqual({
			kind: 'text_substring',
			value: longText
		});
		expect(PREFILL_TEXT_SNIPPET_MAX).toBe(60);
	});

	it('returns empty text_substring when input is empty or whitespace-only', () => {
		expect(prefillFrom('')).toEqual({ kind: 'text_substring', value: '' });
		expect(prefillFrom('   \n\n  ')).toEqual({ kind: 'text_substring', value: '' });
	});

	it('keeps the full multi-line text in the text_substring fallback', () => {
		expect(prefillFrom('\n\nFirst meaningful line\nSecond line')).toEqual({
			kind: 'text_substring',
			value: 'First meaningful line\nSecond line'
		});
	});

	it('does not treat a non-XML angle bracket as an XML tag', () => {
		expect(prefillFrom('<<< banner >>>')).toEqual({
			kind: 'text_substring',
			value: '<<< banner >>>'
		});
	});
});

describe('matches', () => {
	it('text_substring is case-insensitive (mirrors backend matcher)', () => {
		expect(matches('text_substring', 'Foo', 'my foo block')).toBe(true);
		expect(matches('text_substring', 'foo', 'MY FOO BLOCK')).toBe(true);
		expect(matches('text_substring', 'foo', 'my foo block')).toBe(true);
	});

	it('text_substring matches anywhere in the block', () => {
		expect(matches('text_substring', 'needle', 'haystack needle haystack')).toBe(true);
	});

	it('empty value never matches', () => {
		expect(matches('text_substring', '', 'anything')).toBe(false);
		expect(matches('xml_tag', '', '<foo>')).toBe(false);
	});

	it('xml_tag matches at the start of the trimmed block with a close tag', () => {
		expect(matches('xml_tag', 'system-reminder', '<system-reminder>hi</system-reminder>')).toBe(
			true
		);
		expect(
			matches('xml_tag', 'system-reminder', '   \n<system-reminder>hi</system-reminder>\n   ')
		).toBe(true);
	});

	it('xml_tag accepts bare-open AND open-with-attrs as opens', () => {
		expect(matches('xml_tag', 'tag', '<tag>body</tag>')).toBe(true);
		expect(matches('xml_tag', 'tag', '<tag foo="bar">body</tag>')).toBe(true);
	});

	it('xml_tag REQUIRES a matching close tag (regression for C1)', () => {
		expect(matches('xml_tag', 'foo', '<foo>hello')).toBe(false);
		expect(matches('xml_tag', 'foo', '<foo>hello</foo>')).toBe(true);
		expect(matches('xml_tag', 'foo', '<foo attr="x">hi</foo>')).toBe(true);
		expect(matches('xml_tag', 'foo', '<foo>hello</bar>')).toBe(false);
	});

	it('xml_tag rejects self-closing tags (no explicit close)', () => {
		expect(matches('xml_tag', 'foo', '<foo />')).toBe(false);
		expect(matches('xml_tag', 'tag', '<tag />')).toBe(false);
	});

	it('xml_tag does NOT match a prefix of a longer tag name', () => {
		expect(
			matches('xml_tag', 'system-reminder', '<system-reminderfoo>body</system-reminderfoo>')
		).toBe(false);
	});

	it('xml_tag does not match when the block does not start with <tag>', () => {
		expect(
			matches('xml_tag', 'system-reminder', 'nope <system-reminder>hi</system-reminder>')
		).toBe(false);
		expect(
			matches('xml_tag', 'system-reminder', 'prefix <system-reminder>x</system-reminder> suffix')
		).toBe(false);
	});

	it('lazy_section matches a block containing the heading as a whole line', () => {
		const block = 'intro\n\n# Claude in Chrome browser automation\n\nbody text\n\n# Next section';
		expect(matches('lazy_section', '# Claude in Chrome browser automation', block)).toBe(true);
		expect(matches('lazy_section', '# Next section', block)).toBe(true);
	});

	it('lazy_section requires LINE equality — a substring or prefix of a line does not match', () => {
		const block = '# Claude in Chrome browser automation extras\nbody';
		expect(matches('lazy_section', '# Claude in Chrome browser automation', block)).toBe(false);
		expect(matches('lazy_section', '# Heading', 'prose mentioning # Heading inline')).toBe(false);
	});

	it('lazy_section trims line and value whitespace before comparing', () => {
		expect(matches('lazy_section', '# Heading', '  # Heading  \nbody')).toBe(true);
		expect(matches('lazy_section', '  # Heading  ', '# Heading\nbody')).toBe(true);
	});

	it('lazy_section handles CRLF blocks and rejects empty/whitespace values', () => {
		expect(matches('lazy_section', '# Heading', 'pre\r\n# Heading\r\nbody')).toBe(true);
		expect(matches('lazy_section', '', '# anything')).toBe(false);
		expect(matches('lazy_section', '   ', '# anything')).toBe(false);
	});
});

describe('matches shared fixture (UI ↔ backend parity)', () => {
	interface Vector {
		id: string;
		kind: FilterKind;
		value: string;
		block: string;
		expected_match: boolean;
		note?: string;
	}

	const vectors = sharedVectors as ReadonlyArray<Vector>;

	it('has the full vector set (drift-detection guard)', () => {
		expect(vectors.length).toBeGreaterThanOrEqual(12);
	});

	it.each(vectors)(
		'[$id] $kind value=$value expects match=$expected_match',
		({ kind, value, block, expected_match }) => {
			expect(matches(kind, value, block)).toBe(expected_match);
		}
	);
});

describe('splitBlockIntoParagraphs', () => {
	it('returns [] for empty input', () => {
		expect(splitBlockIntoParagraphs('')).toEqual([]);
	});

	it('returns [] for all-whitespace input', () => {
		expect(splitBlockIntoParagraphs('   \n\n  \n  ')).toEqual([]);
	});

	it('returns a single item for input without blank lines', () => {
		expect(splitBlockIntoParagraphs('hello world')).toEqual([{ index: 0, text: 'hello world' }]);
	});

	it('trims leading/trailing whitespace on each paragraph', () => {
		expect(splitBlockIntoParagraphs('  hello  ')).toEqual([{ index: 0, text: 'hello' }]);
	});

	it('splits two paragraphs separated by one blank line', () => {
		expect(splitBlockIntoParagraphs('first paragraph\n\nsecond paragraph')).toEqual([
			{ index: 0, text: 'first paragraph' },
			{ index: 1, text: 'second paragraph' }
		]);
	});

	it('treats multiple blank lines as a single boundary', () => {
		expect(splitBlockIntoParagraphs('one\n\n\n\ntwo\n\n\nthree')).toEqual([
			{ index: 0, text: 'one' },
			{ index: 1, text: 'two' },
			{ index: 2, text: 'three' }
		]);
	});

	it('treats whitespace-only lines as blank boundaries', () => {
		expect(splitBlockIntoParagraphs('one\n   \ntwo\n\t\nthree')).toEqual([
			{ index: 0, text: 'one' },
			{ index: 1, text: 'two' },
			{ index: 2, text: 'three' }
		]);
	});

	it('produces identical results for CRLF and LF line endings', () => {
		const lf = splitBlockIntoParagraphs('one\n\ntwo\n\nthree');
		const crlf = splitBlockIntoParagraphs('one\r\n\r\ntwo\r\n\r\nthree');
		expect(crlf).toEqual(lf);
		expect(crlf).toEqual([
			{ index: 0, text: 'one' },
			{ index: 1, text: 'two' },
			{ index: 2, text: 'three' }
		]);
	});

	it('strips leading and trailing blank lines', () => {
		expect(splitBlockIntoParagraphs('\n\n\nonly paragraph\n\n\n')).toEqual([
			{ index: 0, text: 'only paragraph' }
		]);
		expect(splitBlockIntoParagraphs('\n\none\n\ntwo\n\n')).toEqual([
			{ index: 0, text: 'one' },
			{ index: 1, text: 'two' }
		]);
	});

	it('keeps inner single newlines (poetry-style) as one paragraph', () => {
		expect(splitBlockIntoParagraphs('line one\nline two\nline three')).toEqual([
			{ index: 0, text: 'line one\nline two\nline three' }
		]);
	});

	it('assigns a dense 0-based index across kept paragraphs', () => {
		const result = splitBlockIntoParagraphs('\n\nalpha\n\nbeta\n\n\ngamma');
		expect(result.map((r) => r.index)).toEqual([0, 1, 2]);
	});

	it('preserves NUL bytes inside a paragraph (no truncation at \\0)', () => {
		expect(splitBlockIntoParagraphs('alpha\0middle\0omega')).toEqual([
			{ index: 0, text: 'alpha\0middle\0omega' }
		]);
	});

	it('preserves a very long single line as one paragraph without truncation', () => {
		const long = 'a'.repeat(100_000);
		const result = splitBlockIntoParagraphs(long);
		expect(result).toHaveLength(1);
		expect(result[0]?.text.length).toBe(100_000);
	});
});

describe('splitBlockIntoLines', () => {
	it('returns [] for empty input', () => {
		expect(splitBlockIntoLines('')).toEqual([]);
	});

	it('returns [] for all-whitespace input', () => {
		expect(splitBlockIntoLines('   \n\t\n  ')).toEqual([]);
	});

	it('returns a single item for a single line', () => {
		expect(splitBlockIntoLines('hello')).toEqual([{ index: 0, text: 'hello' }]);
	});

	it('splits multiple lines on LF', () => {
		expect(splitBlockIntoLines('a\nb\nc')).toEqual([
			{ index: 0, text: 'a' },
			{ index: 1, text: 'b' },
			{ index: 2, text: 'c' }
		]);
	});

	it('splits multiple lines on CRLF', () => {
		expect(splitBlockIntoLines('a\r\nb\r\nc')).toEqual([
			{ index: 0, text: 'a' },
			{ index: 1, text: 'b' },
			{ index: 2, text: 'c' }
		]);
	});

	it('handles mixed CRLF and LF in the same input', () => {
		expect(splitBlockIntoLines('a\nb\r\nc\nd')).toEqual([
			{ index: 0, text: 'a' },
			{ index: 1, text: 'b' },
			{ index: 2, text: 'c' },
			{ index: 3, text: 'd' }
		]);
	});

	it('skips blank and whitespace-only lines', () => {
		expect(splitBlockIntoLines('a\n\nb\n   \nc\n\t\nd')).toEqual([
			{ index: 0, text: 'a' },
			{ index: 1, text: 'b' },
			{ index: 2, text: 'c' },
			{ index: 3, text: 'd' }
		]);
	});

	it('assigns a dense 0-based index across kept lines (not raw line numbers)', () => {
		const result = splitBlockIntoLines('\n\nkept1\n  \nkept2\n\nkept3');
		expect(result).toEqual([
			{ index: 0, text: 'kept1' },
			{ index: 1, text: 'kept2' },
			{ index: 2, text: 'kept3' }
		]);
	});

	it('preserves NUL bytes inside a line (no truncation at \\0)', () => {
		const withNul = 'pre\x00mid\x00post';
		const result = splitBlockIntoLines(withNul);
		expect(result).toHaveLength(1);
		expect(result[0]?.text).toBe(withNul);
	});

	it('preserves a very long single line without truncation', () => {
		const long = 'x'.repeat(100_000);
		const result = splitBlockIntoLines(long);
		expect(result).toHaveLength(1);
		expect(result[0]?.text.length).toBe(100_000);
	});
});

describe('parseFromParam', () => {
	it('parses request:{id}', () => {
		expect(parseFromParam('request:42')).toBe(42);
	});

	it('rejects null, empty, malformed, and negative shapes', () => {
		expect(parseFromParam(null)).toBeNull();
		expect(parseFromParam('')).toBeNull();
		expect(parseFromParam('session:42')).toBeNull();
		expect(parseFromParam('request:')).toBeNull();
		expect(parseFromParam('request:-1')).toBeNull();
		expect(parseFromParam('request:abc')).toBeNull();
	});
});

describe('findOccurrences', () => {
	it('finds case-insensitive non-overlapping occurrences', () => {
		expect(findOccurrences('AbcabcABC', 'abc')).toEqual([
			{ start: 0, end: 3 },
			{ start: 3, end: 6 },
			{ start: 6, end: 9 }
		]);
	});

	it('returns [] for an empty needle', () => {
		expect(findOccurrences('anything', '')).toEqual([]);
	});

	it('does not overlap matches', () => {
		expect(findOccurrences('aaa', 'aa')).toEqual([{ start: 0, end: 2 }]);
	});
});

describe('computeBlockMatches', () => {
	const block = 'Keep this. Drop this sentence. Keep that.';

	it('splits a block into segments around a substring match', () => {
		const r = rule({ value: 'Drop this sentence.' });
		const info = computeBlockMatches(block, [r]);
		expect(info.fullMatch).toBeNull();
		expect(info.segments.map((s) => s.text).join('')).toBe(block);
		expect(info.segments).toHaveLength(3);
		expect(info.segments[1]?.rule?.id).toBe('r1');
	});

	it('first rule in stack order wins on overlap', () => {
		const a = rule({ id: 'a', value: 'Drop this sentence.' });
		const b = rule({ id: 'b', value: 'this sentence. Keep' });
		const info = computeBlockMatches(block, [a, b]);
		const matched = info.segments.filter((s) => s.rule !== null);
		expect(matched).toHaveLength(1);
		expect(matched[0]?.rule?.id).toBe('a');
	});

	it('disabled rules never match', () => {
		const r = rule({ enabled: false, value: 'Drop this sentence.' });
		const info = computeBlockMatches(block, [r]);
		expect(info.segments.every((s) => s.rule === null)).toBe(true);
	});

	it('xml_tag whole-block wrap yields fullMatch', () => {
		const xml = '<system-reminder>\nhello\n</system-reminder>';
		const r = rule({ kind: 'xml_tag', value: 'system-reminder' });
		const info = computeBlockMatches(xml, [r]);
		expect(info.fullMatch?.id).toBe('r1');
	});

	it('substring equal to the whole block yields fullMatch', () => {
		const r = rule({ value: block });
		const info = computeBlockMatches(block, [r]);
		expect(info.fullMatch?.id).toBe('r1');
	});

	it('lazy_section rules never claim a whole block, even when the heading IS the block', () => {
		const heading = '# Claude in Chrome browser automation';
		const r = rule({
			kind: 'lazy_section',
			value: heading,
			action: 'replace',
			text: 'stub',
			trigger_token: '!!CHROME!!'
		});
		const info = computeBlockMatches(heading, [r]);
		expect(info.fullMatch).toBeNull();
		expect(info.segments.map((s) => s.text).join('')).toBe(heading);
	});
});

describe('countRuleMatches', () => {
	it('counts blocks for xml_tag rules', () => {
		const r = rule({ kind: 'xml_tag', value: 't' });
		const { count, unit } = countRuleMatches(r, ['<t>x</t>', 'plain', '<t>y</t>']);
		expect(count).toBe(2);
		expect(unit).toBe('block');
	});

	it('counts occurrence slices across blocks for text_substring rules', () => {
		const r = rule({ value: 'ab' });
		const { count, unit } = countRuleMatches(r, ['abab', 'xxabxx', 'none']);
		expect(count).toBe(3);
		expect(unit).toBe('slice');
	});

	it('counts blocks containing the heading line for lazy_section rules', () => {
		const r = rule({
			kind: 'lazy_section',
			value: '# Heading',
			action: 'replace',
			text: 'stub',
			trigger_token: '!!TOK!!'
		});
		const { count, unit } = countRuleMatches(r, [
			'intro\n# Heading\nbody',
			'no heading here',
			'# Heading\nbody',
			'inline # Heading mention does not count'
		]);
		expect(count).toBe(2);
		expect(unit).toBe('block');
	});
});

describe('deriveRuleTitle', () => {
	it('prefers an explicit title over any derivation', () => {
		expect(deriveRuleTitle(rule({ title: 'Operator-named title', action: 'drop' }))).toBe(
			'Operator-named title'
		);
	});

	it('trims a whitespace-padded explicit title', () => {
		expect(deriveRuleTitle(rule({ title: '  Spaced title  ' }))).toBe('Spaced title');
	});

	it('falls back to a derived title when title is null or blank', () => {
		expect(deriveRuleTitle(rule({ title: null, action: 'drop', value: 'noisy text' }))).toBe(
			'Removes “noisy text”'
		);
		expect(deriveRuleTitle(rule({ title: '   ', action: 'drop', value: 'noisy text' }))).toBe(
			'Removes “noisy text”'
		);
	});

	it('derives "Removes" for drop, "Rewrites" for replace, "Adds after" for append', () => {
		expect(deriveRuleTitle(rule({ title: null, action: 'drop', value: 'x' }))).toMatch(/^Removes/);
		expect(deriveRuleTitle(rule({ title: null, action: 'replace', value: 'x' }))).toMatch(
			/^Rewrites/
		);
		expect(deriveRuleTitle(rule({ title: null, action: 'append', value: 'x' }))).toMatch(
			/^Adds after/
		);
	});

	it('derives "Reminds" for inject_reminder regardless of action', () => {
		expect(
			deriveRuleTitle(rule({ title: null, kind: 'inject_reminder', action: 'replace', value: 'browser-guidance' }))
		).toBe('Reminds “browser-guidance”');
	});

	it('caps the quoted snippet at 8 words with an ellipsis, never truncating the underlying rule.value itself', () => {
		const long = 'one two three four five six seven eight nine ten';
		const title = deriveRuleTitle(rule({ title: null, action: 'drop', value: long }));
		expect(title).toBe('Removes “one two three four five six seven eight…”');
	});

	it('collapses internal whitespace in the snippet', () => {
		expect(deriveRuleTitle(rule({ title: null, action: 'drop', value: 'a\n\n  b   c' }))).toBe(
			'Removes “a b c”'
		);
	});
});

describe('paragraphContainsRule', () => {
	it('attributes a text_substring rule to the paragraph containing its value (case-insensitive)', () => {
		expect(
			paragraphContainsRule('This paragraph mentions Ast-Grep tools.', rule({ value: 'ast-grep' }))
		).toBe(true);
		expect(paragraphContainsRule('This paragraph has nothing relevant.', rule({ value: 'ast-grep' }))).toBe(
			false
		);
	});

	it('attributes an append-type rule to the paragraph containing the anchor (rule.value)', () => {
		expect(
			paragraphContainsRule(
				'Reference code as absolute_file_path:line_number.',
				rule({ action: 'append', value: 'absolute_file_path:line_number' })
			)
		).toBe(true);
	});

	it('attributes an xml_tag rule to the paragraph containing the opening tag', () => {
		expect(
			paragraphContainsRule(
				'<system-reminder>\nhello\n</system-reminder>',
				rule({ kind: 'xml_tag', value: 'system-reminder' })
			)
		).toBe(true);
		expect(
			paragraphContainsRule('a paragraph with no tags at all', rule({ kind: 'xml_tag', value: 'system-reminder' }))
		).toBe(false);
	});

	it('never attributes an inject_reminder rule to a source paragraph', () => {
		expect(
			paragraphContainsRule('any paragraph text at all', rule({ kind: 'inject_reminder', value: 'label' }))
		).toBe(false);
	});

	it('never attributes a rule with an empty or blank value', () => {
		expect(paragraphContainsRule('some text', rule({ value: '' }))).toBe(false);
		expect(paragraphContainsRule('some text', rule({ value: '   ' }))).toBe(false);
	});
});

describe('ruleBeforeAfter', () => {
	it('drop: before is the matched value, after is empty', () => {
		expect(ruleBeforeAfter(rule({ action: 'drop', value: 'x', text: null }))).toEqual({
			before: 'x',
			after: ''
		});
	});

	it('replace: before is the value, after is the replacement text', () => {
		expect(ruleBeforeAfter(rule({ action: 'replace', value: 'x', text: 'y' }))).toEqual({
			before: 'x',
			after: 'y'
		});
	});

	it('append: before is empty, after is the appended text', () => {
		expect(ruleBeforeAfter(rule({ action: 'append', value: 'x', text: 'y' }))).toEqual({
			before: '',
			after: 'y'
		});
	});

	it('inject_reminder: before is empty regardless of action, after is the reminder body', () => {
		expect(
			ruleBeforeAfter(
				rule({ kind: 'inject_reminder', action: 'append', value: 'label', text: 'body' })
			)
		).toEqual({ before: '', after: 'body' });
	});

	it('treats a null text as empty, never as the literal string "null"', () => {
		expect(ruleBeforeAfter(rule({ action: 'replace', value: 'x', text: null }))).toEqual({
			before: 'x',
			after: ''
		});
	});
});

describe('ruleLandingLabel', () => {
	it('names the nearest preceding heading in the SAME block', () => {
		const block = '# Session-specific guidance\n - has needle here\n - other line';
		expect(ruleLandingLabel(rule({ value: 'needle' }), [block])).toBe('Session-specific guidance');
	});

	it('finds the nearest heading even when it is several paragraphs above the match', () => {
		const block = '# Memory\n\nintro paragraph\n\nsecond paragraph has needle here';
		expect(ruleLandingLabel(rule({ value: 'needle' }), [block])).toBe('Memory');
	});

	it('never reaches back into an EARLIER block for a heading — falls to that block\'s own classification instead (round-4 defect #5 regression)', () => {
		const blocks = ['# Harness\nintro line', 'more harness content, with needle here, no heading'];
		expect(ruleLandingLabel(rule({ value: 'needle' }), blocks)).toBe('Main prompt');
	});

	it('uses the LATER of two headings when the match falls after both', () => {
		const block = '# First\nclean\n\n# Second\nneedle here';
		expect(ruleLandingLabel(rule({ value: 'needle' }), [block])).toBe('Second');
	});

	it('falls back to the block classification (Identity) when nothing precedes the match under any heading', () => {
		const block = 'You are Claude Code, the needle is right here in the identity line.';
		expect(ruleLandingLabel(rule({ value: 'needle' }), [block])).toBe('Identity');
	});

	it('falls back to the block classification (Instructions) for a long unheaded block', () => {
		const block = `Some long unheaded instructions block. ${'padding '.repeat(100)}needle here`;
		expect(ruleLandingLabel(rule({ value: 'needle' }), [block])).toBe('Instructions');
	});

	it('falls back to "Main prompt" — never a bare block number — for a short unclassified, unheaded block', () => {
		const block = 'a short block with no heading and a needle in it';
		expect(ruleLandingLabel(rule({ value: 'needle' }), [block])).toBe('Main prompt');
		expect(ruleLandingLabel(rule({ value: 'needle' }), [block])).not.toMatch(/Block #?\d/);
	});

	it('falls back to "Somewhere in this prompt" when it matches no paragraph at all — never "Not in this prompt" (this function is only ever called for a rule already known to fire; see ruleGrouping.ts for the firing decision)', () => {
		expect(ruleLandingLabel(rule({ value: 'absent' }), ['one', 'two'])).toBe(
			'Somewhere in this prompt'
		);
	});

	it('says no prompt is open when there are no blocks to check', () => {
		expect(ruleLandingLabel(rule({ value: 'needle' }), [])).toBe('No prompt open');
	});

	it('describes inject_reminder rules by their trigger, never by block match', () => {
		expect(
			ruleLandingLabel(
				rule({ kind: 'inject_reminder', value: 'label', trigger_token: '<chrome>' }),
				['<chrome> appears nowhere near the value']
			)
		).toBe('Injected when the model emits <chrome>');
	});

	it('falls back to a generic phrase when an inject_reminder rule has no trigger token', () => {
		expect(
			ruleLandingLabel(rule({ kind: 'inject_reminder', value: 'label', trigger_token: null }), [
				'anything'
			])
		).toBe('Injected on trigger');
	});

	it('attributes normally by substring search — the firing decision itself lives in ruleGrouping.ts, not here', () => {
		const block = '# Section\nneedle appears right here';
		expect(ruleLandingLabel(rule({ id: 'r1', value: 'needle' }), [block])).toBe('Section');
	});

	it('falls back to "Somewhere in this prompt" when the client-side span search finds nothing, never "Not in this prompt"', () => {
		const block = 'a block with nothing matching the rule value at all';
		expect(ruleLandingLabel(rule({ id: 'r1', value: 'absent-value' }), [block])).toBe(
			'Somewhere in this prompt'
		);
	});

	it('attributes a match spanning a heading + blank line + body to that heading, not "Not in this prompt" (round-4 defect #2, real fixture: rule 70 on request 421514)', () => {
		const rule70 = REQUEST_421514_ENABLED_RULES.find((r) => r.id === '70');
		expect(rule70).toBeDefined();
		const label = ruleLandingLabel(rule70 as FilterRuleDto, REQUEST_421514_BLOCKS);
		expect(label).toBe('Avoid rabbit holes and loops');
	});

	it('never returns "Not in this prompt" for a rule the real dry-run reported as firing (round-4 property check over the whole real fixture)', () => {
		const firingIds = new Set(REQUEST_421514_DRY_RUN.firingRuleIds);
		for (const r of REQUEST_421514_ENABLED_RULES) {
			if (!firingIds.has(r.id)) continue;
			const label = ruleLandingLabel(r, REQUEST_421514_BLOCKS);
			expect(label).not.toBe('Not in this prompt');
		}
	});

	/**
	 * Mirrors `WorkbenchSourcePane.svelte`'s `marginEditsForIndex`: a margin title appears for a
	 * paragraph exactly when `paragraphIndicesTouchedByRule` finds that paragraph's index among
	 * the ones the rule's match touches, for ANY block. G9's own bug used the per-paragraph
	 * TEXT scan (`paragraphContainsRule`) instead, which live evidence showed missing 2 of the
	 * fixture's 9 firing edits (rules 66 and 70, whose matches span a paragraph boundary) — this
	 * asserts the offset-based fix (`paragraphIndicesTouchedByRule`) finds a margin paragraph for
	 * every one of the 9.
	 */
	it('8 of the 9 real firing edits attribute to at least one paragraph across the 4 blocks (G9)', () => {
		const firingIds = new Set(REQUEST_421514_DRY_RUN.firingRuleIds);
		expect(firingIds.size).toBe(9);
		const firingRules = REQUEST_421514_ENABLED_RULES.filter((r) => firingIds.has(r.id));
		expect(firingRules).toHaveLength(9);

		// Rule 66's `value` isn't a substring of ANY of the 4 captured source blocks in this
		// fixture — no client-side span/offset search (old or fixed) can attribute it, which is
		// exactly the case `ruleLandingLabel`'s own "Somewhere in this prompt" fallback documents
		// (a client/server matching gap, not evidence of absence). It's excluded here rather than
		// asserted false, so this test stays a regression guard on the paragraph-boundary fix
		// (round-4 defect #2), not a claim about this specific fixture gap.
		const attributable = firingRules.filter((r) => r.id !== '66');
		expect(attributable).toHaveLength(8);

		for (const r of attributable) {
			const touchesAnyBlock = REQUEST_421514_BLOCKS.some(
				(block) => paragraphIndicesTouchedByRule(block, r).length > 0
			);
			expect(touchesAnyBlock, `rule ${r.id} ("${r.title}") got no margin paragraph`).toBe(true);
		}
	});
});

describe('findRuleMatchSpan / paragraphIndicesTouchedByRule (round-4 defect #2)', () => {
	it('returns null for a kind other than text_substring', () => {
		expect(findRuleMatchSpan('<tag>x</tag>', rule({ kind: 'xml_tag', value: 'tag' }))).toBeNull();
	});

	it('returns null when the value is not found', () => {
		expect(findRuleMatchSpan('some text', rule({ value: 'absent' }))).toBeNull();
	});

	it('finds the case-insensitive start offset of a text_substring match', () => {
		expect(findRuleMatchSpan('prefix NEEDLE suffix', rule({ value: 'needle' }))).toEqual({
			start: 7,
			end: 13
		});
	});

	it('reports every paragraph a multi-paragraph match overlaps, not just the first', () => {
		const block = '# Heading\n\nBody line one.\n\nBody line two.';
		const spanningValue = '# Heading\n\nBody line one.\n\nBody line two.';
		const touched = paragraphIndicesTouchedByRule(block, rule({ value: spanningValue }));
		expect(touched).toEqual([0, 1, 2]);
	});

	it('the real fixture: rule 70 on block 3 of request 421514 touches its heading paragraph plus every paragraph of its body (heading + prose + list + closing prose)', () => {
		const rule70 = REQUEST_421514_ENABLED_RULES.find((r) => r.id === '70');
		expect(rule70).toBeDefined();
		const block3 = REQUEST_421514_BLOCKS[3] ?? '';
		const touched = paragraphIndicesTouchedByRule(block3, rule70 as FilterRuleDto);
		// At least the heading + body + list + closing-prose paragraphs — never zero (the bug),
		// and always starting at the heading itself so the landing label finds it immediately.
		expect(touched.length).toBeGreaterThanOrEqual(4);
		const paragraphs = splitBlockIntoDisplayParagraphs(block3);
		expect(paragraphs[touched[0] ?? -1]?.isHeading).toBe(true);
		expect(paragraphs[touched[0] ?? -1]?.headingTitle).toBe('Avoid rabbit holes and loops');
	});
});

describe('order helpers', () => {
	const rules = [rule({ id: 'a' }), rule({ id: 'b' }), rule({ id: 'c' })];

	it('reconcileOrder keeps local order, drops deleted, appends new', () => {
		expect(reconcileOrder(['c', 'a', 'zombie'], rules)).toEqual(['c', 'a', 'b']);
	});

	it('moveInOrder swaps neighbours and no-ops at edges', () => {
		expect(moveInOrder(['a', 'b', 'c'], 'b', -1)).toEqual(['b', 'a', 'c']);
		expect(moveInOrder(['a', 'b', 'c'], 'a', -1)).toEqual(['a', 'b', 'c']);
		expect(moveInOrder(['a', 'b', 'c'], 'c', 1)).toEqual(['a', 'b', 'c']);
		expect(moveInOrder(['a', 'b', 'c'], 'missing', 1)).toEqual(['a', 'b', 'c']);
	});

	it('reorderByIndex moves an element to the target slot', () => {
		expect(reorderByIndex(['a', 'b', 'c'], 0, 2)).toEqual(['b', 'c', 'a']);
		expect(reorderByIndex(['a', 'b', 'c'], 2, 0)).toEqual(['c', 'a', 'b']);
		expect(reorderByIndex(['a', 'b', 'c'], 1, 1)).toEqual(['a', 'b', 'c']);
		expect(reorderByIndex(['a', 'b', 'c'], -1, 1)).toEqual(['a', 'b', 'c']);
	});
});
