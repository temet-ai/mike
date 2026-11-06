import path from 'node:path';
import os from 'node:os';
import fs from 'node:fs/promises';
import { execFile } from 'node:child_process';
import { promisify } from 'node:util';

import { computeAffectedRoutes } from './graph.mjs';

const execFileP = promisify(execFile);

export async function materializeCommit({ repoRoot, sha, nodeModules }) {
	const scratch = await fs.realpath(
		await fs.mkdtemp(path.join(os.tmpdir(), `visual-verify-${sha.slice(0, 8)}-`))
	);
	const archive = path.join(scratch, 'frontend.tar');
	try {
		await execFileP('git', ['archive', '--format=tar', '-o', archive, sha, 'frontend'], {
			cwd: repoRoot,
			maxBuffer: 32 * 1024 * 1024
		});
		await execFileP('tar', ['-xf', archive, '-C', scratch]);
		await fs.rm(archive, { force: true });
		if (nodeModules) {
			await fs.symlink(nodeModules, path.join(scratch, 'frontend', 'node_modules'), 'dir');
		}
	} catch (err) {
		await fs.rm(scratch, { recursive: true, force: true });
		throw err;
	}
	return scratch;
}

export async function computeAffectedAtCommit({ repoRoot, sha, changedRelFiles, nodeModules }) {
	const scratch = await materializeCommit({ repoRoot, sha, nodeModules });
	try {
		const root = path.join(scratch, 'frontend');
		const changedFiles = changedRelFiles.map((rel) => path.join(scratch, rel));
		return await computeAffectedRoutes({ changedFiles, root });
	} finally {
		await fs.rm(scratch, { recursive: true, force: true, maxRetries: 5, retryDelay: 200 });
	}
}
