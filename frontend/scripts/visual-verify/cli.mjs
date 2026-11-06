#!/usr/bin/env node

import path from 'node:path';
import os from 'node:os';
import fs from 'node:fs/promises';
import { execFile } from 'node:child_process';
import { promisify } from 'node:util';
import { randomBytes } from 'node:crypto';
import { chromium } from 'playwright';

import { FRONTEND_ROOT } from './graph.mjs';
import { computeAffectedAtCommit } from './affected-at.mjs';
import {
	resolveRouteUrl,
	renderRoute,
	startDevServer,
	parseViewport,
	slugForRoute
} from './render.mjs';

const execFileP = promisify(execFile);
const REPO_ROOT = path.resolve(FRONTEND_ROOT, '..');

function log(...args) {
	console.error('[visual-verify]', ...args);
}

async function git(args, opts = {}) {
	const { stdout } = await execFileP('git', args, {
		cwd: REPO_ROOT,
		maxBuffer: 32 * 1024 * 1024,
		...opts
	});
	return stdout.trim();
}

async function changedFilesBetween(before, after) {
	const out = await git(['diff', '--name-only', `${before}`, `${after}`, '--', 'frontend/src']);
	return out
		.split('\n')
		.filter(Boolean)
		.map((rel) => path.join(REPO_ROOT, rel));
}

function parseArgs(argv) {
	const args = { route: [], param: [], changed: [] };
	for (let i = 0; i < argv.length; i++) {
		const a = argv[i];
		switch (a) {
			case '--before':
				args.before = argv[++i];
				break;
			case '--after':
				args.after = argv[++i];
				break;
			case '--route':
				args.route.push(argv[++i]);
				break;
			case '--param':
				args.param.push(argv[++i]);
				break;
			case '--changed':
				args.changed.push(argv[++i]);
				break;
			case '--viewport':
				args.viewport = argv[++i];
				break;
			case '--out':
				args.out = argv[++i];
				break;
			case '--inspect-backend':
				args.inspectBackend = argv[++i];
				break;
			default:
				log(`unrecognised arg: ${a}`);
		}
	}
	return args;
}

function paramsFromArgs(list) {
	const params = {};
	for (const kv of list) {
		const idx = kv.indexOf('=');
		if (idx === -1) continue;
		params[kv.slice(0, idx)] = kv.slice(idx + 1);
	}
	return params;
}

async function affectedAtAfter(args, changedFiles) {
	return computeAffectedAtCommit({
		repoRoot: REPO_ROOT,
		sha: args.after,
		changedRelFiles: changedFiles.map((f) => path.relative(REPO_ROOT, f)),
		nodeModules: path.join(FRONTEND_ROOT, 'node_modules')
	});
}

async function cmdAffected(args) {
	if (!args.before || !args.after) {
		log('affected requires --before SHA --after SHA');
		process.exit(1);
	}
	const changedFiles =
		args.changed.length > 0
			? args.changed.map((f) => path.resolve(f))
			: await changedFilesBetween(args.before, args.after);

	if (changedFiles.length === 0) {
		console.log(
			JSON.stringify(
				{ before: args.before, after: args.after, changedFiles: [], affectedRoutes: [] },
				null,
				2
			)
		);
		return;
	}

	const affected = await affectedAtAfter(args, changedFiles);
	console.log(
		JSON.stringify(
			{
				before: args.before,
				after: args.after,
				changedFiles: changedFiles.map((f) => path.relative(REPO_ROOT, f)),
				affectedRoutes: affected
			},
			null,
			2
		)
	);
}

async function withCheckout(sha, fn) {
	const scratch = path.join(
		os.tmpdir(),
		`visual-verify-${sha.slice(0, 8)}-${randomBytes(4).toString('hex')}`
	);
	log(`checkout ${sha} -> ${scratch}`);
	await execFileP('git', ['worktree', 'add', '--detach', scratch, sha], { cwd: REPO_ROOT });
	try {
		return await fn(scratch);
	} finally {
		await removeCheckout(scratch);
	}
}

async function removeCheckout(scratch) {
	try {
		await execFileP('git', ['worktree', 'remove', '--force', scratch], { cwd: REPO_ROOT });
		return;
	} catch (err) {
		log(`worktree remove failed for ${scratch}: ${err.message}; deleting the directory`);
	}
	await fs.rm(scratch, { recursive: true, force: true, maxRetries: 5, retryDelay: 500 }).catch(
		(err) => log(`could not delete ${scratch}: ${err.message}`)
	);
	await execFileP('git', ['worktree', 'prune'], { cwd: REPO_ROOT }).catch(
		(err) => log(`git worktree prune failed: ${err.message}`)
	);
}

async function pnpmInstall(cwd) {
	log(`pnpm install in ${cwd}`);
	await execFileP('pnpm', ['install', '--prefer-offline'], { cwd, maxBuffer: 64 * 1024 * 1024 });
}

async function pickPort() {
	return 5100 + Math.floor(Math.random() * 400);
}

async function renderState({ sha, routes, params, viewport, outDir, inspectBackend, label }) {
	return withCheckout(sha, async (scratch) => {
		const frontendDir = path.join(scratch, 'frontend');
		await pnpmInstall(frontendDir);
		const port = await pickPort();
		const server = await startDevServer({ cwd: frontendDir, port, inspectBackend });
		log(`${label} dev server up at ${server.baseUrl}`);
		const browser = await chromium.launch();
		const results = [];
		try {
			for (const route of routes) {
				const resolved = resolveRouteUrl(route.urlPath, params);
				if (!resolved.ok) {
					results.push({
						route: route.urlPath,
						rendered: false,
						reason: `missing params: ${resolved.missing.join(', ')}`
					});
					continue;
				}
				const outFile = path.join(outDir, `${slugForRoute(route.urlPath)}.${label}.png`);
				const r = await renderRoute({
					browser,
					baseUrl: server.baseUrl,
					urlPath: resolved.path,
					outFile,
					viewport
				});
				if (r.ok) {
					results.push({
						route: route.urlPath,
						rendered: true,
						file: path.relative(REPO_ROOT, outFile),
						url: resolved.path
					});
				} else {
					results.push({ route: route.urlPath, rendered: false, reason: r.error });
				}
			}
		} finally {
			await browser.close();
			await server.stop();
		}
		return results;
	});
}

async function cmdRender(args) {
	if (!args.before || !args.after) {
		log('render requires --before SHA --after SHA');
		process.exit(1);
	}
	const viewport = parseViewport(args.viewport);
	const params = paramsFromArgs(args.param);
	const inspectBackend = args.inspectBackend ?? 'http://127.0.0.1:9989';
	const outDir = path.resolve(
		args.out ?? path.join(REPO_ROOT, 'artifacts', 'threads', 'verifier', 'wip', '157')
	);
	await fs.mkdir(outDir, { recursive: true });

	let affectedRoutes;
	let changedFiles;
	if (args.route.length > 0) {
		affectedRoutes = args.route.map((urlPath) => ({ id: urlPath, urlPath, hit: [], chains: [] }));
		changedFiles = args.changed.map((f) => path.relative(REPO_ROOT, f));
	} else {
		changedFiles =
			args.changed.length > 0
				? args.changed.map((f) => path.resolve(f))
				: await changedFilesBetween(args.before, args.after);
		if (changedFiles.length === 0) {
			log('no frontend/src files differ between before and after; nothing to render');
			process.exit(3);
		}
		affectedRoutes = await affectedAtAfter(args, changedFiles);
		changedFiles = changedFiles.map((f) => path.relative(REPO_ROOT, f));
	}

	if (affectedRoutes.length === 0) {
		console.log(
			JSON.stringify(
				{ before: args.before, after: args.after, changedFiles, affectedRoutes: [] },
				null,
				2
			)
		);
		log('no route in the import graph depends on the changed files — nothing to render');
		process.exit(3);
	}

	log(
		`${affectedRoutes.length} affected route(s): ${affectedRoutes.map((r) => r.urlPath).join(', ')}`
	);

	const [beforeResults, afterResults] = await Promise.all([
		renderState({
			sha: args.before,
			routes: affectedRoutes,
			params,
			viewport,
			outDir,
			inspectBackend,
			label: 'before'
		}),
		renderState({
			sha: args.after,
			routes: affectedRoutes,
			params,
			viewport,
			outDir,
			inspectBackend,
			label: 'after'
		})
	]);

	const manifest = {
		generatedBy: 'frontend/scripts/visual-verify/cli.mjs',
		before: { sha: args.before },
		after: { sha: args.after },
		viewport,
		changedFiles,
		routes: affectedRoutes.map((route, i) => ({
			id: route.id,
			urlPath: route.urlPath,
			dependencyCount: route.dependencyCount ?? null,
			hit: route.hit ?? [],
			chains: route.chains ?? [],
			before: beforeResults[i],
			after: afterResults[i]
		})),
		renderedAt: new Date().toISOString()
	};

	const manifestPath = path.join(outDir, 'manifest.json');
	await fs.writeFile(manifestPath, JSON.stringify(manifest, null, 2) + '\n');
	log(`manifest written: ${path.relative(REPO_ROOT, manifestPath)}`);

	const anyRendered = manifest.routes.some((r) => r.before?.rendered && r.after?.rendered);
	console.log(JSON.stringify(manifest, null, 2));
	process.exit(anyRendered ? 0 : 1);
}

async function main() {
	const [, , cmd, ...rest] = process.argv;
	const args = parseArgs(rest);
	if (cmd === 'affected') return cmdAffected(args);
	if (cmd === 'render') return cmdRender(args);
	log('usage: visual-verify <affected|render> --before SHA --after SHA [options]');
	process.exit(2);
}

main().catch((err) => {
	log('fatal:', err.stack ?? err.message);
	process.exit(1);
});
