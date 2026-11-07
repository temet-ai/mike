import fs from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import { afterEach, beforeEach, describe, expect, it } from 'vitest';
import { memoryDirFor, projectSlug, readProjectMemory } from '../src/mcp/memory.js';

describe('projectSlug', () => {
	it('matches the convention verified against ~/.claude/projects/ on this machine: every / becomes -', () => {
		expect(projectSlug('/mnt/data/projects/temet/temet-proxy')).toBe('-mnt-data-projects-temet-temet-proxy');
		expect(projectSlug('/home/unit/Work')).toBe('-home-unit-Work');
	});
});

describe('memoryDirFor', () => {
	it('is <claudeHome>/projects/<slug>/memory', () => {
		expect(memoryDirFor('/home/vimes/.claude', '/mnt/data/projects/temet/temet-proxy')).toBe(
			'/home/vimes/.claude/projects/-mnt-data-projects-temet-temet-proxy/memory'
		);
	});
});

describe('readProjectMemory', () => {
	// A throwaway tmp directory stands in for ~/.claude — never the real home,
	// so this test can never read (or leak) an actual operator's memory files.
	let claudeHome: string;

	beforeEach(async () => {
		claudeHome = await fs.mkdtemp(path.join(os.tmpdir(), 'prompt-assistant-memory-unit-'));
	});
	afterEach(async () => {
		await fs.rm(claudeHome, { recursive: true, force: true });
	});

	it('returns [] when the project has no memory directory at all', async () => {
		const files = await readProjectMemory(claudeHome, '/mnt/data/projects/never-opened');
		expect(files).toEqual([]);
	});

	it('puts MEMORY.md first, then every other *.md file alphabetically, and ignores non-md files', async () => {
		const dir = memoryDirFor(claudeHome, '/mnt/data/projects/demo');
		await fs.mkdir(dir, { recursive: true });
		await fs.writeFile(path.join(dir, 'zzz-last.md'), 'z');
		await fs.writeFile(path.join(dir, 'MEMORY.md'), 'index');
		await fs.writeFile(path.join(dir, 'aaa-first.md'), 'a');
		await fs.writeFile(path.join(dir, 'notes.txt'), 'not markdown, must be ignored');

		const files = await readProjectMemory(claudeHome, '/mnt/data/projects/demo');
		expect(files.map((f) => f.path)).toEqual(['MEMORY.md', 'aaa-first.md', 'zzz-last.md']);
		expect(files[0]?.content).toBe('index');
	});

	it('caps an oversized memory file and flags it truncated, rather than silently slicing it', async () => {
		const dir = memoryDirFor(claudeHome, '/mnt/data/projects/big');
		await fs.mkdir(dir, { recursive: true });
		const big = 'x'.repeat(70_000);
		await fs.writeFile(path.join(dir, 'MEMORY.md'), big);

		const files = await readProjectMemory(claudeHome, '/mnt/data/projects/big');
		expect(files).toHaveLength(1);
		expect(files[0]?.truncated).toBe(true);
		expect(files[0]?.content.length).toBeLessThan(big.length);
	});

	it('never reads outside <claudeHome>/projects/<slug>/memory even for a crafted project path', async () => {
		// The slug is one path.join() segment (every / became -), so a project
		// path cannot smuggle in an extra path separator to escape the memory dir.
		const dir = memoryDirFor(claudeHome, '/mnt/data/projects/../../etc');
		await fs.mkdir(dir, { recursive: true });
		await fs.writeFile(path.join(dir, 'MEMORY.md'), 'safely contained');

		const files = await readProjectMemory(claudeHome, '/mnt/data/projects/../../etc');
		expect(files.map((f) => f.path)).toEqual(['MEMORY.md']);
		// And it never escapes claudeHome/projects itself:
		expect(dir.startsWith(path.join(claudeHome, 'projects'))).toBe(true);
	});
});
