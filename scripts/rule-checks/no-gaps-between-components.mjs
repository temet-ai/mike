#!/usr/bin/env node
// Self-check: this script is the rule. There is no separate rule document.
//
// Walks a live page and reports every place two adjacent component-like
// siblings have a vertical gap. Needs the SPA running (./scripts/start-proxy.sh)
// because "is this a component" is a computed-style question, not a grep one:
// a `gap:` in the source may separate an icon from its label (fine) or two
// panels (not fine), and only the rendered tree tells them apart.
//
// Usage: node scripts/rule-checks/no-gaps-between-components.mjs [url] [rootSelector]
// Exit 0 = CLEAN, 1 = gaps found, 2 = could not run.

import { createRequire } from 'node:module';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const here = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(here, '..', '..');
const require = createRequire(path.join(repoRoot, 'frontend', 'package.json'));

let chromium;
try {
	({ chromium } = require('playwright'));
} catch {
	console.error('SKIP: playwright not resolvable from frontend/ — run pnpm install there first.');
	process.exit(2);
}

const url = process.argv[2] ?? 'http://127.0.0.1:9990/sessions';
const root = process.argv[3] ?? 'body';

const browser = await chromium.launch();
const page = await browser.newPage({ viewport: { width: 1400, height: 900 } });
try {
	await page.goto(url, { waitUntil: 'networkidle', timeout: 60000 });
} catch (err) {
	console.error(`SKIP: could not load ${url} — is the SPA running? (${err.message})`);
	await browser.close();
	process.exit(2);
}
await page.waitForTimeout(2500);

const findings = await page.evaluate((rootSel) => {
	const host = document.querySelector(rootSel);
	if (!host) return null;
	const name = (el) => {
		const cls = String(el.className || '')
			.trim()
			.split(/\s+/)
			.filter((c) => !/^s-[A-Za-z0-9]{10,}$/.test(c))
			.join('.');
		return el.tagName.toLowerCase() + (cls ? '.' + cls : '');
	};
	const pageEl = document.querySelector('.col.detail') ?? document.body;
	const pageBg = getComputedStyle(pageEl).backgroundColor;
	// Component-like: carries its own frame, fill, or border — i.e. it reads as
	// a distinct surface rather than as inline content.
	const isComponent = (el) => {
		const c = getComputedStyle(el);
		if (el.classList.contains('edge')) return true;
		if (c.backgroundColor !== 'rgba(0, 0, 0, 0)' && c.backgroundColor !== pageBg) return true;
		if (c.backgroundImage !== 'none') return true;
		return ['Top', 'Right', 'Bottom', 'Left'].some(
			(s) => parseFloat(c[`border${s}Width`]) > 0 && c[`border${s}Style`] !== 'none'
		);
	};
	const out = [];
	const walk = (parent) => {
		const kids = [...parent.children].filter((k) => k.getBoundingClientRect().height > 0);
		let prev = null;
		for (const k of kids) {
			const r = k.getBoundingClientRect();
			if (prev) {
				const gap = r.top - prev.r.bottom;
				const overlapX = Math.min(r.right, prev.r.right) - Math.max(r.left, prev.r.left);
				if (gap > 1 && overlapX > 20 && isComponent(k) && isComponent(prev.el)) {
					out.push({
						gap: Number(gap.toFixed(1)),
						parent: name(parent),
						before: name(prev.el),
						after: name(k)
					});
				}
			}
			prev = { el: k, r };
		}
		for (const k of kids) walk(k);
	};
	walk(host);
	return out;
}, root);

await browser.close();

if (findings === null) {
	console.error(`SKIP: root selector "${root}" not found on ${url}`);
	process.exit(2);
}
if (findings.length === 0) {
	console.log(`CLEAN — no inter-component gaps on ${url}`);
	process.exit(0);
}
findings.sort((a, b) => b.gap - a.gap);
for (const f of findings) {
	console.log(`GAP ${String(f.gap).padStart(6)}px  in ${f.parent}  between ${f.before}  and  ${f.after}`);
}
console.log(`\n${findings.length} inter-component gap(s) on ${url}`);
process.exit(1);
