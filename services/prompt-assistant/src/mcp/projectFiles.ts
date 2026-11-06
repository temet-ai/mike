import fs from 'node:fs/promises';
import path from 'node:path';

// Size cap per file, so one huge CLAUDE.md cannot blow the model's context.
// Capped WITH a `truncated` flag rather than silently sliced content, per
// the repo's inspection-fidelity convention (a lean view needs a full-content
// sibling; here the sibling is simply reading the file directly on disk).
export const PROJECT_INSTRUCTION_FILE_CAP_BYTES = 65536;

export interface ProjectFile {
	path: string;
	content: string;
	truncated: boolean;
}

/** Reads one file under `root`, refusing any path that escapes it. */
export async function readProjectFile(root: string, relPath: string): Promise<ProjectFile | null> {
	const resolvedRoot = path.resolve(root);
	const resolvedFull = path.resolve(root, relPath);
	if (resolvedFull !== resolvedRoot && !resolvedFull.startsWith(resolvedRoot + path.sep)) {
		return null;
	}
	try {
		const buf = await fs.readFile(resolvedFull);
		const truncated = buf.byteLength > PROJECT_INSTRUCTION_FILE_CAP_BYTES;
		const content = buf.subarray(0, PROJECT_INSTRUCTION_FILE_CAP_BYTES).toString('utf8');
		return { path: relPath, content, truncated };
	} catch {
		return null;
	}
}

/** Lists `.claude/rules/*.md`, relative to `root`. Empty when the directory does not exist. */
export async function listRuleFiles(root: string): Promise<string[]> {
	const rulesDir = path.join(root, '.claude', 'rules');
	try {
		const entries = await fs.readdir(rulesDir, { withFileTypes: true });
		return entries
			.filter((e) => e.isFile() && e.name.endsWith('.md'))
			.map((e) => path.join('.claude', 'rules', e.name));
	} catch {
		return [];
	}
}

/** CLAUDE.md plus every `.claude/rules/*.md` file under a project root. */
export async function readProjectInstructions(root: string): Promise<ProjectFile[]> {
	const claudeMd = await readProjectFile(root, 'CLAUDE.md');
	const ruleRelPaths = await listRuleFiles(root);
	const ruleFiles = await Promise.all(ruleRelPaths.map((p) => readProjectFile(root, p)));
	return [claudeMd, ...ruleFiles].filter((f): f is ProjectFile => f !== null);
}
