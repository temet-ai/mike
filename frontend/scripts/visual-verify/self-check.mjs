#!/usr/bin/env node

import path from 'node:path';
import fs from 'node:fs/promises';
import { computeAffectedRoutes, FRONTEND_ROOT, resolveGlobPrefixDirectory } from './graph.mjs';
import { resolveRouteUrl } from './render.mjs';

const failures = [];

function check(name, condition, detail) {
	if (condition) {
		console.log(`  ok   ${name}`);
	} else {
		console.log(`  FAIL ${name}${detail ? ' — ' + detail : ''}`);
		failures.push(name);
	}
}

const abs = (rel) => path.join(FRONTEND_ROOT, rel);

async function main() {
	console.log('visual-verify self-check\n');

	{
		const affected = await computeAffectedRoutes({ changedFiles: [abs('src/app.css')] });
		const sessionRoute = affected.find((r) => r.id === '/sessions/[sessionId]');
		check(
			'app.css change reaches /sessions/[sessionId]',
			!!sessionRoute,
			'app.css is a real root-layout dependency of every route'
		);
		check(
			'app.css dependency count stays bounded (not a Tailwind-content-scan explosion)',
			!!sessionRoute && sessionRoute.dependencyCount < 300,
			sessionRoute
				? `dependencyCount=${sessionRoute.dependencyCount} (unfixed walk measured 720)`
				: 'route not found'
		);
		const anyStoryOrTestInChain =
			sessionRoute?.chains.flat().some((f) => /\.(stories\.svelte|test\.ts)$/.test(f)) ?? true;
		check('no chain passes through a *.stories.svelte / *.test.ts file', !anyStoryOrTestInChain);
	}

	{
		const affected = await computeAffectedRoutes({
			changedFiles: [abs('src/lib/components/detail/SystemPromptBreakdown.svelte')]
		});
		check(
			'a barrel-only-reachable, actually-unused component does not falsely flag every route',
			affected.length === 0,
			`got ${affected.length} affected route(s): ${affected.map((r) => r.id).join(', ')}`
		);
	}

	{
		const affected = await computeAffectedRoutes({
			changedFiles: [abs('src/lib/components/shell/SessionOverview.svelte')]
		});
		const ids = affected.map((r) => r.id);
		check(
			'SessionOverview.svelte change affects exactly /sessions/[sessionId]',
			ids.length === 1 && ids[0] === '/sessions/[sessionId]',
			`got: ${ids.join(', ') || '(none)'}`
		);
		const route = affected[0];
		const chain = route?.chains?.[0] ?? [];
		check(
			'the chain traces SessionOverview.svelte directly to the route entry (one hop)',
			chain.length === 2 &&
				chain[0].endsWith('SessionOverview.svelte') &&
				chain[1].endsWith('+page.svelte'),
			`chain=${JSON.stringify(chain)}`
		);
	}

	{
		const shellDir = path.join(FRONTEND_ROOT, 'src/lib/components/shell');
		const fakeImporter = path.join(FRONTEND_ROOT, 'src/routes/tools/+page.svelte');
		const resolved = resolveGlobPrefixDirectory('$lib/components/shell/', fakeImporter);
		check(
			'a $lib/ glob prefix resolves to the real directory',
			resolved === shellDir,
			`got ${resolved}`
		);

		const relResolved = resolveGlobPrefixDirectory('./', fakeImporter);
		check(
			"a relative glob prefix resolves against the importer's own directory",
			relResolved === path.dirname(fakeImporter),
			`got ${relResolved}`
		);

		const unknown = resolveGlobPrefixDirectory('$unknownAlias/foo/', fakeImporter);
		check(
			'an alias this resolver does not know returns null rather than a guess',
			unknown === null,
			`got ${unknown}`
		);
	}

	{
		const probeDir = path.join(FRONTEND_ROOT, 'src/routes/__visual_verify_self_check_probe__');
		const probeFile = path.join(probeDir, '+page.svelte');
		const probeSource = [
			'<script lang="ts">',
			"\tconst __name = 'SessionOverview';",
			'\t// eslint-disable-next-line no-unused-expressions',
			'\timport(`$lib/components/shell/${__name}.svelte`);',
			'</script>',
			'',
			'<div>self-check end-to-end probe route — deleted immediately by the check that created it</div>',
			''
		].join('\n');
		try {
			await fs.mkdir(probeDir, { recursive: true });
			await fs.writeFile(probeFile, probeSource, 'utf8');
			const affected = await computeAffectedRoutes({
				changedFiles: [abs('src/lib/components/shell/SessionOverview.svelte')]
			});
			const ids = affected.map((r) => r.id);
			check(
				'a dynamic-import-glob route is discovered through the real regex+crawl wiring, not just the helper function',
				ids.includes('/__visual_verify_self_check_probe__'),
				`got: ${ids.join(', ') || '(none)'}`
			);
		} finally {
			await fs.rm(probeDir, { recursive: true, force: true });
		}
	}

	{
		const affected = await computeAffectedRoutes({
			changedFiles: [abs('src/lib/components/shell/SessionOverview.svelte')]
		});
		const ids = affected.map((r) => r.id);
		check(
			'an unrelated route (/tools) is not flagged by a session-only component change',
			!ids.includes('/tools')
		);
	}

	{
		const ok = resolveRouteUrl('/sessions/[sessionId]', { sessionId: 'abc-123' });
		check(
			'resolveRouteUrl substitutes a supplied param',
			ok.ok === true && ok.path === '/sessions/abc-123',
			JSON.stringify(ok)
		);

		const missing = resolveRouteUrl('/sessions/[sessionId]/requests/[requestId]', {
			sessionId: 'abc-123'
		});
		check(
			'resolveRouteUrl reports a missing param by name rather than guessing or silently skipping',
			missing.ok === false && missing.missing.includes('requestId'),
			JSON.stringify(missing)
		);
	}

	console.log('');
	if (failures.length > 0) {
		console.log(`self-check: FAIL (${failures.length}/${failures.length + 0} listed above failed)`);
		process.exit(1);
	}
	console.log('self-check: PASS');
}

main().catch((err) => {
	console.error('self-check: fatal error', err);
	process.exit(1);
});
