import test from 'node:test';
import assert from 'node:assert/strict';
import path from 'node:path';
import os from 'node:os';
import fs from 'node:fs/promises';
import { execFileSync } from 'node:child_process';

import { FRONTEND_ROOT } from './graph.mjs';
import { computeAffectedAtCommit } from './affected-at.mjs';

const git = (cwd, ...args) => execFileSync('git', args, { cwd, encoding: 'utf8' }).trim();

async function put(repo, rel, text) {
	const file = path.join(repo, rel);
	await fs.mkdir(path.dirname(file), { recursive: true });
	await fs.writeFile(file, text);
}

test('affected routes come from the compared commit, not the working tree', async () => {
	const repo = await fs.mkdtemp(path.join(os.tmpdir(), 'vv-affected-test-'));
	try {
		git(repo, 'init', '-q');
		git(repo, 'config', 'user.email', 't@example.com');
		git(repo, 'config', 'user.name', 't');
		await put(repo, 'frontend/vite.config.ts', 'export default {};\n');
		await put(repo, 'frontend/src/routes/a/+page.svelte', '<p>a</p>\n');
		await put(repo, 'frontend/src/routes/b/+page.svelte', '<p>b</p>\n');
		git(repo, 'add', '-A');
		git(repo, 'commit', '-qm', 'before');
		await put(repo, 'frontend/src/routes/a/+page.svelte', '<p>a2</p>\n');
		git(repo, 'commit', '-qam', 'after');
		const after = git(repo, 'rev-parse', 'HEAD');

		await fs.rm(path.join(repo, 'frontend/src/routes/a'), { recursive: true });
		await put(repo, 'frontend/src/routes/c/+page.svelte', '<p>c</p>\n');

		const affected = await computeAffectedAtCommit({
			repoRoot: repo,
			sha: after,
			changedRelFiles: ['frontend/src/routes/a/+page.svelte'],
			nodeModules: path.join(FRONTEND_ROOT, 'node_modules')
		});
		assert.deepEqual(
			affected.map((r) => r.urlPath),
			['/a']
		);
		assert.deepEqual(affected[0].hit, ['src/routes/a/+page.svelte']);
	} finally {
		await fs.rm(repo, { recursive: true, force: true });
	}
});
