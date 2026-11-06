import fs from 'node:fs/promises';
import path from 'node:path';
import { readProjectFile, type ProjectFile } from './projectFiles.js';

/**
 * The convention Claude Code itself uses for a project's on-disk session
 * directory: the absolute project path with every `/` replaced by `-` (so
 * `/mnt/data/projects/temet/temet-proxy` becomes
 * `-mnt-data-projects-temet-temet-proxy`), verified against the directories
 * that actually exist under `~/.claude/projects/` on this machine. It is
 * always exactly one path segment, so it cannot escape `projects/` even when
 * the project path itself contains `..` — those become a literal `..` inside
 * the single slug component, not a traversal.
 */
export function projectSlug(projectPath: string): string {
	return projectPath.split('/').join('-');
}

export function memoryDirFor(claudeHome: string, projectPath: string): string {
	return path.join(claudeHome, 'projects', projectSlug(projectPath), 'memory');
}

/** MEMORY.md first (it's the index), then every other `*.md` file alphabetically. */
async function listMemoryFiles(memoryDir: string): Promise<string[]> {
	let entries: string[];
	try {
		entries = await fs.readdir(memoryDir);
	} catch {
		return [];
	}
	return entries
		.filter((e) => e.endsWith('.md'))
		.sort((a, b) => {
			if (a === 'MEMORY.md') return -1;
			if (b === 'MEMORY.md') return 1;
			return a.localeCompare(b);
		});
}

/**
 * Reads a project's Claude Code memory: `~/.claude/projects/<slug>/memory/*.md`,
 * read-only, each file capped the same way `read_project_instructions` caps
 * CLAUDE.md and the rule files (see `projectFiles.ts`). Returns `[]` when the
 * project has never been opened by Claude Code (no `memory/` directory yet)
 * rather than erroring — that is a normal, common case, not a failure.
 */
export async function readProjectMemory(claudeHome: string, projectPath: string): Promise<ProjectFile[]> {
	const memoryDir = memoryDirFor(claudeHome, projectPath);
	const names = await listMemoryFiles(memoryDir);
	const files = await Promise.all(names.map((name) => readProjectFile(memoryDir, name)));
	return files.filter((f): f is ProjectFile => f !== null);
}
