import path from 'node:path';
import fs from 'node:fs/promises';
import { existsSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { createServer } from 'vite';

const HERE = path.dirname(fileURLToPath(import.meta.url));
export const FRONTEND_ROOT = path.resolve(HERE, '..', '..');
const ROUTES_DIR = path.join(FRONTEND_ROOT, 'src', 'routes');
const SRC_DIR = path.join(FRONTEND_ROOT, 'src');

const STYLESHEET_RE = /\.(css|scss|sass|less|styl|stylus|postcss)$/;

async function walkFiles(dir, out) {
	const entries = await fs.readdir(dir, { withFileTypes: true });
	for (const entry of entries) {
		const full = path.join(dir, entry.name);
		if (entry.isDirectory()) {
			await walkFiles(full, out);
		} else {
			out.push(full);
		}
	}
}

function dirToUrlPath(relDir) {
	if (relDir === '') return '/';
	const segments = relDir.split(path.sep).filter(Boolean);
	const kept = segments.filter((s) => !(s.startsWith('(') && s.endsWith(')')));
	return '/' + kept.join('/');
}

export async function discoverRoutes(routesDir = ROUTES_DIR) {
	const files = [];
	await walkFiles(routesDir, files);
	const pages = files.filter((f) => path.basename(f) === '+page.svelte');

	const routes = pages.map((pageFile) => {
		const dir = path.dirname(pageFile);
		const relDir = path.relative(routesDir, dir);
		const urlPath = dirToUrlPath(relDir);

		const parts = relDir === '' ? [] : relDir.split(path.sep);
		const ancestorDirs = [routesDir];
		let acc = routesDir;
		for (const part of parts) {
			acc = path.join(acc, part);
			ancestorDirs.push(acc);
		}
		const layoutChain = ancestorDirs
			.map((d) => path.join(d, '+layout.svelte'))
			.filter((f) => existsSync(f));

		return { id: urlPath, urlPath, dir, pageFile, layoutChain };
	});

	routes.sort((a, b) => a.id.localeCompare(b.id));
	return routes;
}

const IMPORT_DEFAULT_AND_NAMED_RE =
	/import\s+(?:type\s+)?([\w$]+)\s*(?:,\s*\{([^}]*)\})?\s*from\s*['"]([^'"]+)['"]/g;
const IMPORT_NAMED_ONLY_RE = /import\s+(?:type\s+)?\{([^}]*)\}\s*from\s*['"]([^'"]+)['"]/g;
const IMPORT_NAMESPACE_RE = /import\s+\*\s+as\s+[\w$]+\s*from\s*['"]([^'"]+)['"]/g;
const IMPORT_SIDE_EFFECT_RE = /^\s*import\s*['"]([^'"]+)['"]/gm;
const DYNAMIC_IMPORT_RE = /import\(\s*['"]([^'"]+)['"]\s*\)/g;
const DYNAMIC_TEMPLATE_IMPORT_RE = /import\(\s*`([^`]*)`\s*\)/g;
const ANY_DYNAMIC_IMPORT_RE = /import\(/g;
const EXPORT_NAMED_FROM_RE = /export\s+(?:type\s+)?\{([^}]*)\}\s*from\s*['"]([^'"]+)['"]/g;
const EXPORT_STAR_RE = /export\s+\*(?:\s+as\s+[\w$]+)?\s+from\s*['"]([^'"]+)['"]/g;

function splitNamedBindings(raw) {
	return raw
		.split(',')
		.map((s) => s.trim())
		.filter(Boolean)
		.filter((s) => !s.startsWith('type '));
}

function extractScriptText(filePath, source) {
	if (!filePath.endsWith('.svelte')) return source;
	const scriptRe = /<script[^>]*>([\s\S]*?)<\/script>/g;
	let out = '';
	let m;
	while ((m = scriptRe.exec(source))) out += m[1] + '\n';
	return out;
}

function parseImportEdges(scriptText, { warnUnresolvedDynamicImport } = {}) {
	const edges = [];
	const seen = new Set();
	const consumedDynamicImportIndices = new Set();

	const mark = (specifier, matchIndex, names) => {
		const key = specifier + '@' + matchIndex;
		if (seen.has(key)) return;
		seen.add(key);
		edges.push({ specifier, names });
	};

	for (const m of scriptText.matchAll(IMPORT_DEFAULT_AND_NAMED_RE)) {
		const namedRaw = m[2];
		const specifier = m[3];
		const names = ['default'];
		if (namedRaw) {
			for (const n of splitNamedBindings(namedRaw)) {
				names.push(n.split(/\s+as\s+/)[0].trim());
			}
		}
		mark(specifier, m.index, names);
	}
	for (const m of scriptText.matchAll(IMPORT_NAMED_ONLY_RE)) {
		const [, namedRaw, specifier] = m;
		const names = splitNamedBindings(namedRaw).map((n) => n.split(/\s+as\s+/)[0].trim());
		mark(specifier, m.index, names);
	}
	for (const m of scriptText.matchAll(IMPORT_NAMESPACE_RE)) {
		mark(m[1], m.index, null);
	}
	for (const m of scriptText.matchAll(IMPORT_SIDE_EFFECT_RE)) {
		mark(m[1], m.index, []);
	}
	for (const m of scriptText.matchAll(DYNAMIC_IMPORT_RE)) {
		mark(m[1], m.index, null);
		consumedDynamicImportIndices.add(m.index);
	}
	for (const m of scriptText.matchAll(DYNAMIC_TEMPLATE_IMPORT_RE)) {
		consumedDynamicImportIndices.add(m.index);
		const raw = m[1];
		const interpIdx = raw.indexOf('${');
		if (interpIdx === -1) {
			mark(raw, m.index, null);
			continue;
		}
		const prefix = raw.slice(0, interpIdx);
		const key = 'glob:' + prefix + '@' + m.index;
		if (!seen.has(key)) {
			seen.add(key);
			edges.push({ globPrefix: prefix, names: null });
		}
	}
	if (warnUnresolvedDynamicImport) {
		for (const m of scriptText.matchAll(ANY_DYNAMIC_IMPORT_RE)) {
			if (!consumedDynamicImportIndices.has(m.index)) {
				warnUnresolvedDynamicImport(m.index);
			}
		}
	}
	return edges;
}

function parseReexportTable(scriptText) {
	const table = new Map();
	let hasStarExport = false;

	for (const m of scriptText.matchAll(EXPORT_NAMED_FROM_RE)) {
		const [, namedRaw, specifier] = m;
		for (const raw of splitNamedBindings(namedRaw)) {
			const parts = raw.split(/\s+as\s+/).map((s) => s.trim());
			const sourceName = parts[0];
			const exportedName = parts[1] ?? parts[0];
			table.set(exportedName, { specifier, sourceName });
		}
	}
	hasStarExport = [...scriptText.matchAll(EXPORT_STAR_RE)].length > 0;
	return { table, hasStarExport };
}

function toRootUrlAt(filePath, root) {
	const rel = path.relative(root, filePath);
	return '/' + rel.split(path.sep).join('/');
}

async function withViteServer(root, fn) {
	const prevCwd = process.cwd();
	process.chdir(root);
	const server = await createServer({
		root,
		configFile: path.join(root, 'vite.config.ts'),
		server: { middlewareMode: true, hmr: false, watch: null },
		optimizeDeps: { noDiscovery: true },
		logLevel: 'silent'
	});
	try {
		return await fn(server);
	} finally {
		await server.close();
		process.chdir(prevCwd);
	}
}

async function resolveSpecifier(server, specifier, importerFile) {
	try {
		const resolved = await server.pluginContainer.resolveId(specifier, importerFile);
		return resolved?.id?.split('?')[0] ?? null;
	} catch {
		return null;
	}
}

const SVELTEKIT_LIB_ALIAS_RE = /^\$lib\//;

export function resolveGlobPrefixDirectory(prefix, importerFile, srcDir = SRC_DIR) {
	const isDirectoryShaped = prefix.endsWith('/');

	let dir;
	if (SVELTEKIT_LIB_ALIAS_RE.test(prefix)) {
		dir = path.join(srcDir, 'lib', prefix.replace(SVELTEKIT_LIB_ALIAS_RE, ''));
	} else if (prefix.startsWith('./') || prefix.startsWith('../')) {
		dir = path.resolve(path.dirname(importerFile), prefix);
	} else {
		return null;
	}
	dir = dir.replace(/[/\\]+$/, '');

	return isDirectoryShaped ? dir : path.dirname(dir);
}

export async function crawlClosure(server, entryFile) {
	const FRONTEND_ROOT = server.config.root;
	const SRC_DIR = path.join(FRONTEND_ROOT, 'src');
	const toRootUrl = (f) => toRootUrlAt(f, FRONTEND_ROOT);
	const files = new Set();
	const via = new Map([[entryFile, null]]);
	const barrelTableCache = new Map();
	const queue = [entryFile];

	async function getBarrelTable(file, scriptText) {
		if (barrelTableCache.has(file)) return barrelTableCache.get(file);
		const t = parseReexportTable(scriptText);
		barrelTableCache.set(file, t);
		return t;
	}

	function enqueue(file, parent) {
		if (files.has(file) || !file.startsWith(SRC_DIR)) return;
		files.add(file);
		if (!via.has(file)) via.set(file, parent);
		queue.push(file);
	}

	while (queue.length) {
		const file = queue.shift();
		files.add(file);

		try {
			await server.transformRequest(toRootUrl(file));
		} catch {
			continue;
		}
		if (STYLESHEET_RE.test(file)) continue;

		let source;
		try {
			source = await fs.readFile(file, 'utf8');
		} catch {
			continue;
		}
		const scriptText = extractScriptText(file, source);
		const importEdges = parseImportEdges(scriptText, {
			warnUnresolvedDynamicImport: (matchIndex) => {
				const line = scriptText.slice(0, matchIndex).split('\n').length;
				console.error(
					`[visual-verify] WARNING: unresolvable dynamic import() in ${path.relative(FRONTEND_ROOT, file)}:${line} — the specifier is not a string literal or a template literal with a static prefix, so no static file list exists for it. This edge is NOT in the graph; a route reached only through it may be silently missed.`
				);
			}
		});

		for (const edge of importEdges) {
			if (!('globPrefix' in edge)) continue;
			const dir = resolveGlobPrefixDirectory(edge.globPrefix, file, SRC_DIR);
			if (!dir) {
				console.error(
					`[visual-verify] WARNING: dynamic import() glob prefix "${edge.globPrefix}" in ${path.relative(FRONTEND_ROOT, file)} uses an alias this resolver doesn't know — treated as unresolved, not guessed.`
				);
				continue;
			}
			let entries;
			try {
				entries = await fs.readdir(dir, { withFileTypes: true });
			} catch {
				continue;
			}
			for (const entry of entries) {
				if (!entry.isFile()) continue;
				if (!/\.(svelte|ts|js)$/.test(entry.name)) continue;
				enqueue(path.join(dir, entry.name), file);
			}
		}

		const { table: ownReexports } = await getBarrelTable(file, scriptText);
		for (const { specifier: reexportSpecifier } of ownReexports.values()) {
			const target = await resolveSpecifier(server, reexportSpecifier, file);
			if (target && target.startsWith(SRC_DIR) && target !== file) enqueue(target, file);
		}

		for (const edge of importEdges) {
			if ('globPrefix' in edge) continue;
			const { specifier, names } = edge;
			const target = await resolveSpecifier(server, specifier, file);
			if (!target || !target.startsWith(SRC_DIR)) continue;
			if (target === file) continue;

			if (names === null || STYLESHEET_RE.test(target)) {
				enqueue(target, file);
				continue;
			}

			let targetSource;
			try {
				targetSource = await fs.readFile(target, 'utf8');
			} catch {
				enqueue(target, file);
				continue;
			}
			const targetScript = extractScriptText(target, targetSource);
			const { table, hasStarExport } = await getBarrelTable(target, targetScript);

			const isPureBarrel = table.size > 0 && !hasStarExport;
			if (!isPureBarrel || names.length === 0) {
				enqueue(target, file);
				continue;
			}

			let anyResolved = false;
			for (const name of names) {
				const entry = table.get(name);
				if (!entry) continue;
				anyResolved = true;
				const subTarget = await resolveSpecifier(server, entry.specifier, target);
				if (subTarget && subTarget.startsWith(SRC_DIR)) {
					enqueue(subTarget, target);
				}
			}
			if (!anyResolved) enqueue(target, file);
		}
	}

	files.add(entryFile);
	return { files, via };
}

export function explainChain(file, via) {
	const chain = [file];
	let cur = file;
	while (via.has(cur) && via.get(cur) !== null) {
		cur = via.get(cur);
		chain.push(cur);
	}
	return chain;
}

export async function computeAffectedRoutes({ changedFiles, root = FRONTEND_ROOT }) {
	const changed = new Set(changedFiles.map((f) => path.resolve(f)));
	const routes = await discoverRoutes(path.join(root, 'src', 'routes'));

	return withViteServer(root, async (server) => {
		const results = [];
		for (const route of routes) {
			const entries = [route.pageFile, ...route.layoutChain];
			const closureFiles = new Set();
			const viaMerged = new Map();
			for (const entry of entries) {
				const { files, via } = await crawlClosure(server, entry);
				for (const f of files) closureFiles.add(f);
				for (const [f, parent] of via) if (!viaMerged.has(f)) viaMerged.set(f, parent);
			}
			const hit = [...changed].filter((f) => closureFiles.has(f));
			if (hit.length === 0) continue;
			results.push({
				id: route.id,
				urlPath: route.urlPath,
				pageFile: path.relative(root, route.pageFile),
				layoutChain: route.layoutChain.map((f) => path.relative(root, f)),
				dependencyCount: closureFiles.size,
				hit: hit.map((f) => path.relative(root, f)),
				chains: hit.map((f) => explainChain(f, viaMerged).map((c) => path.relative(root, c)))
			});
		}
		return results;
	});
}

export async function computeAllRouteClosures({ root = FRONTEND_ROOT } = {}) {
	const routes = await discoverRoutes(path.join(root, 'src', 'routes'));
	return withViteServer(root, async (server) => {
		const out = [];
		for (const route of routes) {
			const entries = [route.pageFile, ...route.layoutChain];
			const closureFiles = new Set();
			for (const entry of entries) {
				const { files } = await crawlClosure(server, entry);
				for (const f of files) closureFiles.add(f);
			}
			out.push({ ...route, closureFiles });
		}
		return out;
	});
}
