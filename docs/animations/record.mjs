// Records the interactive demos playing their scripted run, for viewers that
// cannot run HTML (GitHub shows .html as source). Each demo becomes an
// animated WebP next to it, embedded by the docs that link the demo. A GIF of
// the same run is 30 MB, too large for GitHub to show.
//
//   make doc-recordings        # or: node docs/animations/record.mjs [live-turn|fleet-run] [--shot]
//
// Needs the frontend's Playwright (pnpm install in frontend/) and ffmpeg.
import { createRequire } from 'node:module';
import { execFileSync } from 'node:child_process';
import { mkdtempSync, readdirSync, renameSync, rmSync, statSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

const here = dirname(fileURLToPath(import.meta.url));
const require = createRequire(join(here, '../../frontend/package.json'));
const { chromium } = require('playwright');

const VIEW = { width: 1440, height: 800 };
const MAX_MS = 480_000;
const DEMOS = {
	'live-turn': { top: 180 },
	'fleet-run': { top: 200 },
};

const args = process.argv.slice(2);
const shot = args.includes('--shot');
const names = args.filter((a) => !a.startsWith('--'));
const todo = names.length ? names : Object.keys(DEMOS);

// A demo has finished when neither Play nor Pause can be pressed: Play stays
// disabled once the scripted run is used up, Pause is enabled only while busy.
// Typing the scripted prompt also disables both, so the state must hold.
async function finished(page, maxMs) {
	const start = Date.now();
	let doneSince = 0;
	while (Date.now() - start < maxMs) {
		await page.waitForTimeout(250);
		const done = await page.evaluate(() =>
			document.getElementById('play').disabled && document.getElementById('pause').disabled);
		if (!done) doneSince = 0;
		else if (!doneSince) doneSince = Date.now();
		else if (Date.now() - doneSince > 4_000) return Date.now() - start;
	}
	throw new Error(`demo did not finish within ${maxMs / 1000} s`);
}

async function record(browser, name) {
	const tmp = mkdtempSync(join(tmpdir(), `mike-${name}-`));
	const ctx = await browser.newContext({
		viewport: VIEW,
		deviceScaleFactor: 1,
		colorScheme: 'dark',
		reducedMotion: 'no-preference',
		recordVideo: shot ? undefined : { dir: tmp, size: VIEW },
	});
	const page = await ctx.newPage();
	const opened = Date.now();
	await page.goto(pathToFileURL(join(here, `${name}.html`)).href);
	await page.evaluate((y) => window.scrollTo(0, y), DEMOS[name].top);
	await page.waitForTimeout(800);
	if (shot) {
		await page.screenshot({ path: join(here, `${name}.png`) });
		await ctx.close();
		rmSync(tmp, { recursive: true, force: true });
		return;
	}
	const skip = ((Date.now() - opened) / 1000).toFixed(2);
	await page.evaluate(() => document.getElementById('play').click());
	const ms = await finished(page, MAX_MS);
	await ctx.close();
	const webm = join(tmp, readdirSync(tmp).find((f) => f.endsWith('.webm')));
	encode(webm, name, skip);
	rmSync(tmp, { recursive: true, force: true });
	console.log(`${name}: ${(ms / 1000).toFixed(0)} s recorded`);
}

function encode(webm, name, skip) {
	const fps = 10;
	const width = 1100;
	const scale = `fps=${fps},scale=${width}:-1:flags=lanczos`;
	const webp = join(here, `${name}.webp`);
	execFileSync('ffmpeg', ['-y', '-loglevel', 'error', '-ss', skip, '-i', webm,
		'-vf', scale, '-c:v', 'libwebp_anim', '-lossless', '0', '-q:v', '70',
		'-compression_level', '6', '-loop', '0', `${webp}.tmp.webp`]);
	renameSync(`${webp}.tmp.webp`, webp);
	console.log(`  ${webp}: ${(statSync(webp).size / 1e6).toFixed(1)} MB`);
}

const browser = await chromium.launch();
try {
	for (const name of todo) await record(browser, name);
} finally {
	await browser.close();
}
