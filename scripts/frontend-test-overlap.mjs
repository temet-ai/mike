#!/usr/bin/env node
/**
 * frontend-test-overlap.mjs
 *
 * Inventory frontend test files that have sibling Storybook stories and rank
 * them by how much DOM/render work they do. The goal is to make it easy to
 * spot tests that are probably duplicating Storybook's visual coverage.
 *
 * Usage:
 *   node scripts/frontend-test-overlap.mjs
 *   node scripts/frontend-test-overlap.mjs --all
 *   node scripts/frontend-test-overlap.mjs --json
 *   node scripts/frontend-test-overlap.mjs --gate
 */

import { readFileSync, readdirSync, statSync, existsSync } from 'node:fs';
import { join, dirname, basename, relative } from 'node:path';
import { fileURLToPath } from 'node:url';

const __dirname = dirname(fileURLToPath(import.meta.url));
const REPO = join(__dirname, '..');
const FRONTEND_SRC = join(REPO, 'frontend', 'src');

const args = new Set(process.argv.slice(2));
const JSON_OUTPUT = args.has('--json');
const INCLUDE_ALL = args.has('--all');
const GATE = args.has('--gate');

function walk(dir, predicate) {
	const results = [];
	for (const entry of readdirSync(dir)) {
		const full = join(dir, entry);
		const stat = statSync(full);
		if (stat.isDirectory()) {
			results.push(...walk(full, predicate));
		} else if (predicate(full)) {
			results.push(full);
		}
	}
	return results;
}

function countMatches(text, regex) {
	const matches = text.match(regex);
	return matches ? matches.length : 0;
}

function lineHits(text, needles) {
	const lines = text.split(/\r?\n/);
	const hits = [];
	lines.forEach((line, index) => {
		if (needles.some((needle) => line.includes(needle))) {
			hits.push(index + 1);
		}
	});
	return hits;
}

function siblingStoryFor(testFile) {
	const storyName = basename(testFile).replace(/\.test\.[^.]+$/, '.stories.svelte');
	return join(dirname(testFile), storyName);
}

function formatLines(lines) {
	if (lines.length === 0) return '-';
	return lines.join(', ');
}

const testFiles = walk(FRONTEND_SRC, (f) => /\.test\.[tj]sx?$/.test(f));

const entries = testFiles.map((file) => {
	const source = readFileSync(file, 'utf8');
	const storyFile = siblingStoryFor(file);
	const hasStory = existsSync(storyFile);
	const storySource = hasStory ? readFileSync(storyFile, 'utf8') : '';

	const renderLines = lineHits(source, ['render(']);
	const interactionLines = lineHits(source, [
		'fireEvent.',
		'.click(',
		'dispatchEvent(',
		'vi.useFakeTimers',
		'vi.advanceTimersByTime',
		'vi.advanceTimersByTimeAsync',
		'tick()',
		'waitFor(',
		'userEvent.'
	]);

	const testCases = countMatches(source, /\b(?:it|test)(?:\.each)?\s*\(/g);
	const storyCount = hasStory ? countMatches(storySource, /<Story\b/g) : 0;
	const likelyVisual = hasStory && renderLines.length > 0 && interactionLines.length === 0;

	return {
		file: relative(REPO, file),
		storyFile: hasStory ? relative(REPO, storyFile) : null,
		storyCount,
		testCases,
		renderLines,
		interactionLines,
		likelyVisual
	};
});

const storyBacked = entries.filter((entry) => entry.storyFile);
const storyless = entries.filter((entry) => !entry.storyFile);

storyBacked.sort((a, b) => {
	if (a.likelyVisual !== b.likelyVisual) return a.likelyVisual ? -1 : 1;
	if (b.renderLines.length !== a.renderLines.length) return b.renderLines.length - a.renderLines.length;
	return a.file.localeCompare(b.file);
});

storyless.sort((a, b) => a.file.localeCompare(b.file));

const likelyVisual = storyBacked.filter((entry) => entry.likelyVisual);

if (JSON_OUTPUT) {
	console.log(
		JSON.stringify(
			{
				totalTestFiles: entries.length,
				storyBacked: storyBacked.length,
				storyless: storyless.length,
				likelyVisual,
				storyBackedEntries: storyBacked,
				storylessEntries: storyless
			},
			null,
			2
		)
	);
} else {
	console.log('frontend-test-overlap');
	console.log(
		`tests: ${entries.length}  story-backed: ${storyBacked.length}  storyless: ${storyless.length}  likely-visual: ${likelyVisual.length}`
	);
	console.log('');

	if (storyBacked.length > 0) {
		console.log('Story-backed tests');
		for (const entry of storyBacked) {
			const tag = entry.likelyVisual ? 'visual-candidate' : 'behavior';
			console.log(
				`- ${entry.file} | ${tag} | stories=${entry.storyCount} | cases=${entry.testCases} | render=${entry.renderLines.length} | interactions=${entry.interactionLines.length}`
			);
			if (entry.renderLines.length > 0) {
				console.log(`  render lines: ${formatLines(entry.renderLines)}`);
			}
			if (entry.interactionLines.length > 0) {
				console.log(`  interaction lines: ${formatLines(entry.interactionLines)}`);
			}
		}
		console.log('');
	}

	if (INCLUDE_ALL && storyless.length > 0) {
		console.log('Storyless tests');
		for (const entry of storyless) {
			console.log(`- ${entry.file} | cases=${entry.testCases} | render=${entry.renderLines.length}`);
		}
		console.log('');
	}

	if (likelyVisual.length > 0) {
		console.log('Likely visual duplicates to review first');
		for (const entry of likelyVisual) {
			console.log(`- ${entry.file} (${entry.storyFile})`);
		}
		console.log('');
	} else {
		console.log('Likely visual duplicates to review first: none');
		console.log('');
	}
}

if (GATE && likelyVisual.length > 0) {
	console.error(
		`frontend-test-overlap --gate FAILED: ${likelyVisual.length} likely visual duplicate(s) found.`
	);
	process.exit(1);
}
