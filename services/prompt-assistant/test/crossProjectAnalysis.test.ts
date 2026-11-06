import { describe, expect, it } from 'vitest';
import {
	EXCERPTS_PER_PROJECT_CAP,
	EXCERPT_CHAR_CAP,
	EXCERPT_PROJECTS_CAP,
	TOTAL_OUTPUT_CHAR_BUDGET,
	assembleCompareResponse,
	classifyVerdict,
	extractKeyTerms,
	paragraphsWithHits,
	type RawProjectAnalysis
} from '../src/mcp/crossProjectAnalysis.js';

describe('extractKeyTerms', () => {
	it('lowercases, dedupes, and drops stopwords and short words', () => {
		const terms = extractKeyTerms('Never use pronouns for someone whose pronouns are unknown.');
		expect(terms).toContain('pronouns');
		expect(terms).toContain('unknown');
		expect(terms).not.toContain('never');
		expect(terms).not.toContain('for');
		expect(new Set(terms).size).toBe(terms.length);
	});
});

describe('paragraphsWithHits', () => {
	it('returns only paragraphs containing a key term, trimmed', () => {
		const content = '# Heading\nUnrelated paragraph about deployment.\n\nA paragraph about pronoun handling.\n\nAnother unrelated one.';
		const hits = paragraphsWithHits(content, ['pronoun']);
		expect(hits).toEqual(['A paragraph about pronoun handling.']);
	});

	it('returns nothing when there are no key terms', () => {
		expect(paragraphsWithHits('anything at all', [])).toEqual([]);
	});
});

describe('classifyVerdict', () => {
	it('is none with zero hits regardless of action', () => {
		expect(classifyVerdict('drop', [])).toBe('none');
	});

	it('is possible_conflict for a non-drop action even with hits', () => {
		expect(classifyVerdict('replace', ['Always mention pronouns explicitly.'])).toBe('possible_conflict');
	});

	it('is agrees for a drop action whose hit paragraphs prohibit the same thing', () => {
		expect(classifyVerdict('drop', ['Never state a pronoun unless asked.'])).toBe('agrees');
	});

	it('is possible_conflict for a drop action whose hit paragraphs require the same thing', () => {
		expect(classifyVerdict('drop', ['You must always confirm pronouns before replying.'])).toBe('possible_conflict');
	});
});

function stubAnalysis(index: number, hitCount: number, matchInPrompt: boolean): RawProjectAnalysis {
	return {
		project_path: `/mnt/data/projects/stub-${String(index).padStart(3, '0')}`,
		project_name: `Stub Project ${index}`,
		recent_sessions: (index % 7) + 1,
		match_in_prompt: matchInPrompt,
		hitParagraphs: Array.from({ length: hitCount }, (_, i) => ({
			path: 'CLAUDE.md',
			text: `Paragraph ${i} in project ${index} about pronoun handling. `.repeat(20)
		}))
	};
}

describe('assembleCompareResponse — 90-project scale', () => {
	function build90(): RawProjectAnalysis[] {
		const analyses: RawProjectAnalysis[] = [];
		for (let i = 0; i < 90; i++) {
			const hitCount = i < 20 ? (20 - i) % 6 : 0;
			analyses.push(stubAnalysis(i, hitCount, i % 3 === 0));
		}
		return analyses;
	}

	it('keeps the whole serialized result under the total output budget', () => {
		const result = assembleCompareResponse(build90(), '/mnt/data/projects/current', 30, 'drop');
		const size = JSON.stringify(result).length;
		expect(size).toBeLessThanOrEqual(TOTAL_OUTPUT_CHAR_BUDGET);
	});

	it('reports every considered project as either included or omitted, never both counted twice', () => {
		const result = assembleCompareResponse(build90(), null, 30, 'drop');
		expect(result.projects_considered).toBe(90);
		expect(result.projects.length + result.omitted_projects).toBe(90);
	});

	it('orders rows by instruction hits descending, so trimming drops the least relevant first', () => {
		const result = assembleCompareResponse(build90(), null, 30, 'drop');
		const hits = result.projects.map((p) => p.instruction_hits);
		for (let i = 1; i < hits.length; i++) {
			expect(hits[i]).toBeLessThanOrEqual(hits[i - 1]!);
		}
	});

	it('attaches excerpts only to at most EXCERPT_PROJECTS_CAP rows, only when instruction_hits > 0', () => {
		const result = assembleCompareResponse(build90(), null, 30, 'drop');
		const withExcerpts = result.projects.filter((p) => p.excerpts !== undefined);
		expect(withExcerpts.length).toBeLessThanOrEqual(EXCERPT_PROJECTS_CAP);
		for (const row of withExcerpts) {
			expect(row.instruction_hits).toBeGreaterThan(0);
			expect(row.excerpts!.length).toBeLessThanOrEqual(EXCERPTS_PER_PROJECT_CAP);
			for (const excerpt of row.excerpts!) {
				expect(excerpt.text.length).toBeLessThanOrEqual(EXCERPT_CHAR_CAP);
			}
		}
		const withoutExcerpts = result.projects.filter((p) => p.excerpts === undefined);
		for (const row of withoutExcerpts) {
			expect(row.instruction_hits === 0 || withExcerpts.length === EXCERPT_PROJECTS_CAP).toBe(true);
		}
	});

	it('marks truncated when rows had to be dropped to fit the budget', () => {
		const heavy: RawProjectAnalysis[] = Array.from({ length: 90 }, (_, i) => stubAnalysis(i, 3, true));
		const result = assembleCompareResponse(heavy, null, 30, 'drop');
		expect(result.truncated).toBe(true);
		expect(result.omitted_projects).toBeGreaterThan(0);
	});

	it('is deterministic across repeated calls with the same input', () => {
		const analyses = build90();
		const a = assembleCompareResponse(analyses, null, 30, 'drop');
		const b = assembleCompareResponse(analyses, null, 30, 'drop');
		expect(a).toEqual(b);
	});
});
