import path from 'node:path';
import { spawn } from 'node:child_process';
import { chromium } from 'playwright';

export function resolveRouteUrl(urlPath, params = {}) {
	const missing = [];
	const resolved = urlPath.replace(/\[([a-zA-Z0-9_]+)\]/g, (_, name) => {
		if (!(name in params)) {
			missing.push(name);
			return `[${name}]`;
		}
		return encodeURIComponent(params[name]);
	});
	if (missing.length > 0) return { ok: false, missing };
	return { ok: true, path: resolved };
}

async function waitForQuiet(page, { quietMs = 700, timeoutMs = 20_000 } = {}) {
	await new Promise((resolve) => {
		let timer = setTimeout(resolve, quietMs);
		const hardStop = setTimeout(() => {
			clearTimeout(timer);
			page.off('request', bump);
			resolve();
		}, timeoutMs);
		function bump() {
			clearTimeout(timer);
			timer = setTimeout(() => {
				clearTimeout(hardStop);
				page.off('request', bump);
				resolve();
			}, quietMs);
		}
		page.on('request', bump);
	});
}

export async function renderRoute({ browser, baseUrl, urlPath, outFile, viewport }) {
	const context = await browser.newContext({ viewport });
	const page = await context.newPage();
	try {
		await page.goto(new URL(urlPath, baseUrl).toString(), {
			waitUntil: 'domcontentloaded',
			timeout: 30_000
		});
		await waitForQuiet(page);
		await page.waitForTimeout(250);
		await page.screenshot({ path: outFile, fullPage: true });
		return { ok: true };
	} catch (err) {
		return { ok: false, error: err.message };
	} finally {
		await context.close();
	}
}

async function waitForServerReady(url, { timeoutMs = 30_000 } = {}) {
	const deadline = Date.now() + timeoutMs;
	while (Date.now() < deadline) {
		try {
			const res = await fetch(url, { signal: AbortSignal.timeout(2000) });
			if (res.ok || res.status < 500) return true;
		} catch {}
		await new Promise((r) => setTimeout(r, 300));
	}
	return false;
}

export async function startDevServer({ cwd, port, inspectBackend, host = '127.0.0.1' }) {
	const child = spawn('pnpm', ['exec', 'vite', 'dev', '--port', String(port), '--strictPort'], {
		cwd,
		env: {
			...process.env,
			MIKE_FRONTEND_PORT: String(port),
			MIKE_FRONTEND_HOST: host,
			INSPECT_BACKEND: inspectBackend
		},
		stdio: ['ignore', 'pipe', 'pipe'],
		detached: true
	});

	const killGroup = (signal) => {
		try {
			process.kill(-child.pid, signal);
		} catch {
			child.kill(signal);
		}
	};

	let out = '';
	child.stdout?.on('data', (d) => (out += d.toString()));
	child.stderr?.on('data', (d) => (out += d.toString()));

	const baseUrl = `http://${host}:${port}`;
	const ready = await waitForServerReady(baseUrl, { timeoutMs: 45_000 });
	if (!ready) {
		killGroup('SIGKILL');
		throw new Error(
			`vite dev did not become ready on ${baseUrl} within timeout.\n${out.slice(-4000)}`
		);
	}

	return {
		baseUrl,
		log: () => out,
		async stop() {
			killGroup('SIGTERM');
			await new Promise((resolve) => {
				const t = setTimeout(resolve, 5000);
				child.once('exit', () => {
					clearTimeout(t);
					resolve();
				});
			});
			killGroup('SIGKILL');
		}
	};
}

export function parseViewport(spec) {
	const m = /^(\d+)x(\d+)$/.exec(spec ?? '');
	if (!m) return { width: 1440, height: 900 };
	return { width: Number(m[1]), height: Number(m[2]) };
}

export function slugForRoute(urlPath) {
	const slug = urlPath
		.replace(/^\/+/, '')
		.replace(/[[\]/]+/g, '-')
		.replace(/-+$/, '');
	return slug === '' ? 'root' : slug;
}

export { path, chromium };
