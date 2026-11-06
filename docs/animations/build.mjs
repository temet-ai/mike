#!/usr/bin/env node
// Builds the animated SVGs embedded in README.md and docs/*.md.
//
// Every SVG animates with SMIL and CSS only, never script, so GitHub renders it
// inside an <img>. The facts in each scene come from docs/REQUEST_FLOW.md and
// docs/STARTUP_FLOW.md; update those docs first, then the scene, then rerun:
//
//   node docs/animations/build.mjs      (or: make doc-animations)
//
// The interactive version of the same scenes is docs/animations/mike-in-motion.html.
import { writeFileSync } from 'node:fs';

const OUT = new URL('.', import.meta.url);
const P = {
	bg: '#0e1116', s1: '#14181e', s2: '#191f26', s3: '#1f2630', line: '#333d4b', lineStrong: '#57657a',
	t1: '#dae1ea', t2: '#b6c1cf', t3: '#8b95a4',
	cyan: '#22d9ff', magenta: '#ff2e88', green: '#2ee56b', red: '#ff4d4d', blue: '#4d8dff', yellow: '#ffc933', violet: '#b48cff'
};
const C = { req: P.cyan, res: P.green, db: P.blue, mut: P.magenta, evt: P.yellow, bad: P.red, mcp: P.violet, mute: P.t3 };
const KINDS = {
	outside: { fg: '#c9d1dc', fill: '#1b1f25', ln: '#4d5664' },
	mike: { fg: '#5fe0a0', fill: '#152a20', ln: '#2b7a52' },
	vendor: { fg: '#d97757', fill: '#1b1f25', ln: '#7a4a3a' },
	store: { fg: '#4d8dff', fill: '#16202f', ln: '#2f4f86' },
	tool: { fg: '#b48cff', fill: '#221b33', ln: '#5f4a8c' },
	terminal: { fg: '#dae1ea', fill: '#191f26', ln: '#57657a' },
	stage: { fg: '#b6c1cf', fill: '#191f26', ln: '#333d4b' },
	armed: { fg: '#ff2e88', fill: '#2a1420', ln: '#8a2a55' }
};
const MONO = "'JetBrains Mono', ui-monospace, SFMono-Regular, Menlo, Consolas, monospace";
const DISPLAY = "'Syne', 'Avenir Next', 'Segoe UI', Helvetica, Arial, sans-serif";
const HEAD = 58;

const esc = (s) => String(s).replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;').replace(/"/g, '&quot;');
const n3 = (x) => Math.round(x * 1000) / 1000;
const chamfer = (x, y, w, h, c = 10) => `M${x + c},${y} H${x + w} V${y + h - c} L${x + w - c},${y + h} H${x} V${y + c} Z`;

/* ─── timing primitives ─────────────────────────────────────── */
// Opacity that is 1 inside each [start, end] span (seconds) and 0 outside, looping every T.
function fade(T, spans, ramp = 0.14) {
	const pts = [[0, 0]];
	for (const [s0, e0] of spans) {
		const s = Math.max(0, s0), e = Math.min(T, e0);
		pts.push([s, 0], [Math.min(T, s + ramp), 1], [e, 1], [Math.min(T, e + ramp), 0]);
	}
	pts.push([T, 0]);
	const clean = [];
	for (const [t, v] of pts) {
		const last = clean[clean.length - 1];
		if (last && t <= last[0]) {
			if (t === last[0] && v === last[1]) continue;
			clean.push([n3(last[0] + 0.001), v]);
		} else clean.push([t, v]);
	}
	if (clean[0][1] === 1) clean.unshift([0, 1]);
	const kt = clean.map(([t]) => n3(Math.min(1, t / T))).join(';');
	const vals = clean.map(([, v]) => v).join(';');
	return `<animate attributeName="opacity" dur="${T}s" repeatCount="indefinite" values="${vals}" keyTimes="${kt}"/>`;
}
// Position along a path: travel from 0 to 1 between s and e, invisible otherwise.
function motion(T, d, s, e, reverse) {
	const [a, b] = reverse ? [1, 0] : [0, 1];
	const kt = `0;${n3(s / T)};${n3(e / T)};1`;
	return `<animateMotion dur="${T}s" repeatCount="indefinite" path="${d}" keyPoints="${a};${a};${b};${b}" keyTimes="${kt}" calcMode="spline" keySplines="0 0 1 1;0.45 0 0.55 1;0 0 1 1"/>`;
}

/* ─── diagram primitives ────────────────────────────────────── */
function anchorFn(nodes) {
	return (s) => {
		if (Array.isArray(s)) return s;
		const [id, side, off = '0'] = s.split(':');
		const n = nodes[id];
		const o = Number(off);
		const cx = n.x + n.w / 2, cy = n.y + n.h / 2;
		return { t: [cx + o, n.y], b: [cx + o, n.y + n.h], l: [n.x, cy + o], r: [n.x + n.w, cy + o] }[side];
	};
}
function nodeSvg(n) {
	const k = KINDS[n.kind];
	const lines = n.lines || [n.label];
	const center = n.align === 'center';
	const tx = center ? n.x + n.w / 2 : n.x + 14;
	const anchor = center ? 'middle' : 'start';
	let y = n.y + (n.top ?? 25);
	let out = `<path d="${chamfer(n.x, n.y, n.w, n.h, n.c ?? 10)}" fill="${k.fill}" stroke="${k.ln}" stroke-width="1.2"/>`;
	lines.forEach((ln, i) => (out += `<text x="${tx}" y="${y + i * 15}" text-anchor="${anchor}" class="nl" fill="${k.fg}">${esc(ln)}</text>`));
	y += (lines.length - 1) * 15;
	[].concat(n.sub || []).forEach((s, i) => (out += `<text x="${tx}" y="${y + 17 + i * 14}" text-anchor="${anchor}" class="ns" fill="${n.subFill || P.t3}">${esc(s)}</text>`));
	return out;
}
function focusSvg(T, n, spans, color) {
	if (!spans.length) return '';
	const k = KINDS[n.kind];
	return `<path d="${chamfer(n.x, n.y, n.w, n.h, n.c ?? 10)}" fill="none" stroke="${color || k.fg}" stroke-width="2" class="glow" style="color:${color || k.fg}" opacity="0">${fade(T, spans)}</path>`;
}
function packetSvg(T, d, f, W) {
	const right = f.labelRight ?? false;
	const lx = right ? -10 : 10;
	return `<g opacity="0">${fade(T, [[f.s, f.e + 0.12]], 0.06)}${motion(T, d, f.s, f.e, f.reverse)}` +
		`<circle r="11" fill="${f.color}" opacity="0.22"/><circle r="5" fill="${f.color}"/>` +
		(f.label ? `<text x="${lx}" y="-10" text-anchor="${right ? 'end' : 'start'}" class="pl" fill="${f.color}">${esc(f.label)}</text>` : '') +
		'</g>';
}

/* ─── scene assembly ────────────────────────────────────────── */
// steps: [{ d, title, text: [line, line?], focus: [nodeIds], flows: [{ path, at, dur, color, label, reverse, persist }] }]
function timeline(steps) {
	let t = 0.6;
	const flows = [], caps = [], focus = {};
	for (const st of steps) {
		const s = t, e = t + st.d;
		caps.push({ s, e, title: st.title, text: [].concat(st.text || []) });
		(st.focus || []).forEach((id) => (focus[id] = focus[id] || []).push([s, e - 0.1]));
		(st.flows || []).forEach((f) => {
			const fs = s + (f.at || 0);
			flows.push({ ...f, s: fs, e: fs + (f.dur || 1.1) });
		});
		st.at = s;
		t = e;
	}
	return { T: n3(t + 0.8), flows, caps, focus };
}
function captionSvg(T, caps, W, y) {
	let out = `<line x1="0" y1="${y}" x2="${W}" y2="${y}" stroke="${P.line}"/>`;
	caps.forEach((c, i) => {
		c.text.forEach((ln) => ln.length > 118 && console.warn(`caption line too long (${ln.length}): ${ln}`));
		const num = `${String(i + 1).padStart(2, '0')} / ${String(caps.length).padStart(2, '0')}`;
		out += `<g opacity="0">${fade(T, [[c.s, c.e - 0.05]])}` +
			`<text x="24" y="${y + 27}" class="cn">${num}</text>` +
			`<text x="110" y="${y + 27}" class="ct">${esc(c.title)}</text>` +
			c.text.map((ln, k) => `<text x="110" y="${y + 49 + k * 17}" class="cx">${esc(ln)}</text>`).join('') +
			'</g>';
	});
	out += `<rect x="0" y="${y + 90}" width="0" height="3" fill="${P.cyan}" opacity="0.7"><animate attributeName="width" dur="${T}s" repeatCount="indefinite" values="0;${W}" keyTimes="0;1"/></rect>`;
	return out;
}
function frame({ W, H, title, desc, eyebrow, heading }, body) {
	return `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 ${W} ${H}" width="${W}" height="${H}" role="img" aria-labelledby="t d">
<title id="t">${esc(title)}</title>
<desc id="d">${esc(desc)}</desc>
<style>
text { font-family: ${MONO}; }
.nl { font-size: 13px; font-weight: 600; }
.ns { font-size: 10.5px; }
.pl { font-size: 11px; font-weight: 600; paint-order: stroke; stroke: ${P.bg}; stroke-width: 4px; stroke-linejoin: round; }
.wl { font-size: 10.5px; fill: ${P.t2}; paint-order: stroke; stroke: ${P.bg}; stroke-width: 4px; }
.eb { font-size: 10.5px; letter-spacing: 2px; fill: ${P.cyan}; }
.hd { font-family: ${DISPLAY}; font-size: 19px; font-weight: 700; fill: ${P.t1}; }
.cn { font-size: 11px; fill: ${P.cyan}; }
.ct { font-size: 13.5px; font-weight: 700; fill: ${P.t1}; }
.cx { font-size: 12px; fill: ${P.t2}; }
.sm { font-size: 11px; fill: ${P.t2}; }
.xs { font-size: 10px; fill: ${P.t3}; }
.wire { fill: none; stroke: ${P.lineStrong}; stroke-width: 1.4; stroke-dasharray: 3 5; opacity: 0.6; }
.live { fill: none; stroke-width: 2; stroke-dasharray: 7 6; animation: march 0.5s linear infinite; }
.live.rev { animation-direction: reverse; }
.lit { fill: none; stroke-width: 1.5; }
.glow { filter: drop-shadow(0 0 6px currentColor); }
@keyframes march { to { stroke-dashoffset: -13; } }
</style>
<path d="${chamfer(0.5, 0.5, W - 1, H - 1, 14)}" fill="${P.bg}" stroke="${P.line}"/>
<text x="24" y="24" class="eb">${esc(eyebrow || 'MIKE · HOW IT WORKS')}</text>
<text x="24" y="46" class="hd">${esc(heading)}</text>
${body}
</svg>
`;
}
// Standard scene: nodes + wires + packets + captions.
function diagramScene(cfg) {
	const { W, DH, nodes, paths, steps, extra } = cfg;
	const H = HEAD + DH + 96;
	const { T, flows, caps, focus } = timeline(steps);
	const anchor = anchorFn(nodes);
	const D = {};
	for (const [id, p] of Object.entries(paths)) D[id] = 'M' + p.pts.map(anchor).map((q) => q.map(n3).join(',')).join(' L');
	let wires = '', lits = '', lives = '', packets = '', texts = '';
	for (const id of Object.keys(paths)) wires += `<path d="${D[id]}" class="wire"/>`;
	const litSpans = {};
	for (const f of flows) {
		const d = D[f.path];
		lives += `<path d="${d}" class="live${f.reverse ? ' rev' : ''}" stroke="${f.color}" opacity="0">${fade(T, [[f.s, f.e]])}</path>`;
		if (f.persist !== false) (litSpans[f.path] = litSpans[f.path] || { color: f.color, s: f.e }).s = Math.min(litSpans[f.path].s, f.e);
		const end = anchor(paths[f.path].pts[f.reverse ? 0 : paths[f.path].pts.length - 1]);
		packets += packetSvg(T, d, { ...f, labelRight: end[0] > W * 0.72 || (f.reverse && anchor(paths[f.path].pts[0])[0] > W * 0.72) }, W);
	}
	for (const [id, l] of Object.entries(litSpans)) {
		lits += `<path d="${D[id]}" class="lit" stroke="${l.color}" opacity="0"${paths[id].marker ? ` marker-end="url(#${paths[id].marker})"` : ''}>${fade(T, [[l.s, T - 0.45]])}</path>`;
		const p = paths[id];
		if (p.text) {
			const pts = p.pts.map(anchor);
			const [a, b] = p.tpos != null ? [pts[p.tpos], pts[p.tpos + 1]] : [pts[0], pts[pts.length - 1]];
			const x = p.tx ?? (a[0] + b[0]) / 2;
			texts += `<text x="${n3(x)}" y="${n3(Math.min(a[1], b[1]) - 7)}" text-anchor="${p.tanchor || 'middle'}" class="wl" opacity="0">${esc(p.text)}${fade(T, [[l.s - 0.9, T - 0.45]])}</text>`;
		}
	}
	let nodeLayer = '', focusLayer = '';
	for (const [id, n] of Object.entries(nodes)) {
		nodeLayer += `<g${n.dim ? ` opacity="0.5"` : ''}>${nodeSvg(n)}</g>`;
		focusLayer += focusSvg(T, n, focus[id] || []);
	}
	const body = `${cfg.defs || ''}<g transform="translate(0,${HEAD})">${cfg.under ? cfg.under(T) : ''}${wires}${lits}${lives}${texts}${nodeLayer}${focusLayer}${extra ? extra({ T, steps }) : ''}${packets}</g>` +
		captionSvg(T, caps, W, HEAD + DH);
	return { svg: frame({ ...cfg, W, H }, body), T };
}

/* ═══ Scene: overview (README) and map (ARCHITECTURE) ════════════ */
function mapScene(withSidecar) {
	const dy = withSidecar ? 0 : -150;
	const at = (y) => y + dy;
	const nodes = {
		claude: { x: 30, y: at(250), w: 180, h: 76, kind: 'outside', label: 'Claude Code', sub: 'ANTHROPIC_BASE_URL' },
		proxy: { x: 330, y: at(230), w: 200, h: 116, kind: 'mike', label: 'mike proxy', sub: [':9988 · /v1 and /v2', 'capture + opt-in stages', 'SSE tee + parser'] },
		anthropic: { x: 790, y: at(250), w: 180, h: 76, kind: 'vendor', label: 'Anthropic API', sub: 'api.anthropic.com' },
		pg: { x: 330, y: at(440), w: 200, h: 76, kind: 'store', label: 'Postgres', sub: 'capture + normalized bodies' },
		inspect: { x: 590, y: at(440), w: 180, h: 76, kind: 'mike', label: 'Inspect API', sub: ':9989 · /api, /api/ws' },
		spa: { x: 810, y: at(440), w: 160, h: 76, kind: 'terminal', label: 'Web UI', sub: ':9990 · SvelteKit' },
		mcp: { x: 30, y: at(440), w: 180, h: 76, kind: 'tool', label: 'mike-mcp', sub: 'stdio in, HTTP out' }
	};
	const paths = {
		c2p: { pts: ['claude:r:-12', 'proxy:l:-22'] },
		p2c: { pts: ['proxy:l:+14', 'claude:r:+12'] },
		p2a: { pts: ['proxy:r:-22', 'anthropic:l:-12'] },
		a2p: { pts: ['anthropic:l:+12', 'proxy:r:+14'] },
		p2db: { pts: ['proxy:b:-40', 'pg:t:-40'] },
		p2db2: { pts: ['proxy:b:+40', 'pg:t:+40'] },
		pg2i: { pts: ['pg:r', 'inspect:l'] },
		i2spa: { pts: ['inspect:r', 'spa:l'] },
		c2m: { pts: ['claude:b', 'mcp:t'] },
		m2i: { pts: ['mcp:b', [120, at(570)], [680, at(570)], 'inspect:b'] }
	};
	if (withSidecar) {
		nodes.fwd = { x: 330, y: 60, w: 200, h: 76, kind: 'mike', label: 'proxy-forward', sub: ':9987 · HTTPS MITM, optional', dim: true };
		paths.c2f = { pts: ['claude:t', [120, 98], 'fwd:l'] };
		paths.f2p = { pts: ['fwd:b', 'proxy:t'] };
		paths.f2a = { pts: ['fwd:r', [880, 98], 'anthropic:t'] };
	}
	const steps = [
		{ d: 3, title: 'A turn leaves Claude Code', focus: ['claude', 'proxy'],
			flows: [{ path: 'c2p', color: C.req, label: 'POST /v1/messages' }],
			text: 'Claude Code runs with ANTHROPIC_BASE_URL=http://localhost:9988, so every model call lands on mike first.' },
		{ d: 2.8, title: 'The original bytes are kept, and a stub row goes in', focus: ['proxy', 'pg'],
			flows: [{ path: 'p2db', color: C.db, label: 'stub + original bytes' }],
			text: ['engine::accept keeps the untouched inbound body before any stage runs. The Phase-1 stub row is', 'attempted before forwarding when persistence is available; it establishes the durable RequestId.'] },
		{ d: 2.8, title: 'Forwarded to Anthropic', focus: ['proxy', 'anthropic'],
			flows: [{ path: 'p2a', color: C.req, label: 'forwarded body' }],
			text: ['Rewrites depend on settings, authored rules and queued messages. This example follows the plain path;', 'the logical inbound capture stays separate from the forwarded representation.'] },
		{ d: 3.4, title: 'The answer streams back', focus: ['anthropic', 'proxy', 'claude'],
			flows: [{ path: 'a2p', color: C.res, label: 'SSE' }, { path: 'p2c', color: C.res, label: 'tee', at: 1.2 }],
			text: 'The stream is tee\'d: live to Claude Code, and into the SSE parser that fills in the capture.' },
		{ d: 2.6, title: 'Terminal snapshot', focus: ['proxy', 'pg'],
			flows: [{ path: 'p2db2', color: C.db, label: 'snapshot_terminal' }],
			text: ['The terminal snapshot persists completion metadata and the response. Normalized capture may later', 'reconstruct compacted bodies; body APIs report provenance and fidelity. There is no separate blob store.'] },
		{ d: 3.2, title: 'Live in the web UI', focus: ['pg', 'inspect', 'spa'],
			flows: [{ path: 'pg2i', color: C.evt, label: 'NOTIFY' }, { path: 'i2spa', color: C.evt, label: '/api/ws', at: 1.1 }],
			text: 'NOTIFY proxy.row_changed → LISTEN task → event_tx → /api/ws. Views also fetch or poll their own data.' },
		{ d: 6, title: 'Claude asks mike about its own session', focus: ['claude', 'mcp', 'inspect', 'pg'],
			flows: [
				{ path: 'c2m', color: C.mcp, label: 'MCP tool call' },
				{ path: 'm2i', color: C.mcp, label: 'GET /api/requests', at: 1.0, dur: 1.3 },
				{ path: 'pg2i', color: C.db, label: 'SELECT', reverse: true, at: 2.4, dur: 0.8, persist: false },
				{ path: 'pg2i', color: C.res, label: 'rows', at: 3.2, dur: 0.8, persist: false },
				{ path: 'm2i', color: C.res, label: 'JSON', reverse: true, at: 4.0, dur: 1.2, persist: false },
				{ path: 'c2m', color: C.res, label: 'result', reverse: true, at: 5.1, dur: 0.8, persist: false }
			],
			text: 'mike-mcp uses stdio and the inspect API. Body reads expose fidelity; select narrows returned JSON.' }
	];
	if (withSidecar) {
		steps.push({ d: 4.4, title: 'Optional: the forward sidecar', focus: ['claude', 'fwd', 'proxy', 'anthropic'],
			flows: [
				{ path: 'c2f', color: C.req, label: 'HTTPS' },
				{ path: 'f2p', color: C.req, label: 'routed leg', at: 1.2, dur: 0.9 },
				{ path: 'f2a', color: C.mute, label: 'failover', at: 2.4 }
			],
			text: ['proxy-forward :9987 is HTTPS MITM. Ordinary non-WebSocket /v1/* and /v2/* HTTP requests use :9988;', 'WebSocket upgrades go direct; unhealthy-capture failover also goes straight to api.anthropic.com.'] });
	}
	return diagramScene({
		W: 1000, DH: withSidecar ? 600 : 450, nodes, paths, steps,
		heading: withSidecar ? 'One turn, every port it touches' : 'mike: capture every turn between Claude Code and Anthropic',
		title: withSidecar ? 'mike system map, animated' : 'How mike works, animated',
		desc: 'Animated diagram. A Claude Code request goes through the mike proxy on port 9988 to Anthropic; the original bytes and the response are written to Postgres; a NOTIFY updates the web UI over /api/ws; mike-mcp lets Claude Code query the capture through the inspect API.' + (withSidecar ? ' The optional proxy-forward sidecar on 9987 routes traffic to 9988 or fails over to Anthropic.' : '')
	});
}

/* ═══ Scene: thirteen stages (REQUEST_FLOW, ARCHITECTURE §4) ═════ */
function stagesScene() {
	const STAGES = [
		['accept', 'fixed'], ['filter'], ['reminder_on_token'], ['lazy_sections'], ['recall_injector', 'armed'], ['lessons'],
		['self_identity'], ['workspace_briefing'], ['agent_lessons'], ['reminders', 'armed'], ['steerer', 'armed'], ['read_dedup', 'armed'],
		['file_read_compression'], ['proxy_tools'], ['composer'], ['upstream', 'fixed']
	];
	const INFO = {
		accept: ['accept', 'stored_body = the original inbound bytes, captured before any stage runs.', 'This copy is what the inspect API and mike-mcp serve, whatever happens next.'],
		recall_injector: ['recall_injector · armed', 'Gate: recall_injector toggle (default off). Injects recalled context from earlier turns,', 'when the configured recall trigger is present (for example mem:recall), through a JSON round-trip.'],
		reminders: ['reminders · armed', 'Gate: reminders toggle (default off). Appends queued reminders, same serde_json round-trip.'],
		steerer: ['primary-lane queue drain', 'The drain runs on primary lanes. InjectionGate checks each source; only steerer-origin notes', 'require the steerer toggle. Explicitly authored mailbox messages use their own delivery gate.'],
		read_dedup: ['read_dedup · armed', 'Gate: compression_dedup.enabled (default off). Repeated Read results become forwarded-only stubs;', 'the logical inbound capture keeps the original tool-result text.'],
		composer: ['composer · armed', 'Gate: composition_enabled boot config (default off). Template mode replaces system[2].text in place:', 'eligible on primary_first only; bytes outside that string survive, but changed text changes the cache prefix.'],
		upstream: ['upstream', 'Capture admission → Phase-1 stub → send. The forwarded body differs from the capture;', 'the logical inbound capture never changed. Other stages did not rewrite in this example.']
	};
	const CW = 110, CH = 66, X0 = 25, Y1 = 30, Y2 = 200;
	const nodes = {}, centers = [];
	STAGES.forEach(([id, st], i) => {
		const row2 = i >= 8, col = row2 ? 15 - i : i;
		const x = X0 + col * 120, y = row2 ? Y2 : Y1;
		centers.push([x + CW / 2, y + CH / 2]);
		const lines = id === 'steerer' ? ['steerer', '+ queue drain'] : id.length > 13 ? splitName(id) : [id];
		nodes[id] = {
			x, y, w: CW, h: CH, c: 8, align: 'center', top: lines.length > 1 ? 23 : 27, lines,
			kind: id === 'accept' ? 'mike' : id === 'upstream' ? 'vendor' : st === 'armed' ? 'armed' : 'stage',
			sub: id === 'accept' ? 'keeps inbound' : id === 'upstream' ? '→ Anthropic' : st === 'armed' ? 'active' : 'idle here',
			subFill: st === 'armed' ? P.magenta : undefined
		};
	});
	// one path through every chip centre; the packet pauses at armed chips
	const d = 'M' + centers.map((c) => c.join(',')).join(' L');
	const segLen = centers.slice(1).map((c, i) => Math.hypot(c[0] - centers[i][0], c[1] - centers[i][1]));
	const total = segLen.reduce((a, b) => a + b, 0);
	let t = 1.6, dist = 0;
	const kt = [[0, 0], [t, 0]];
	const arrive = {};
	const caps = [{ s: 0.3, e: t, id: 'accept' }];
	STAGES.slice(1).forEach(([id, st], i) => {
		t += 0.32; dist += segLen[i];
		kt.push([t, dist / total]);
		arrive[id] = t;
		if (st === 'armed') { caps.push({ s: t, e: t + 2.6, id }); t += 2.6; kt.push([t, dist / total]); }
		else if (id === 'upstream') { caps.push({ s: t, e: t + 5, id }); t += 5; kt.push([t, 1]); }
		else t += 0.06;
	});
	const T = n3(t + 0.2);
	kt.push([T, 1]);
	const ktStr = kt.map(([x]) => n3(x / T)).join(';');
	const kpStr = kt.map(([, p]) => n3(p)).join(';');
	const DH = 470, W = 1000, H = HEAD + DH + 96;

	let chips = '', flashes = '';
	for (const [id, st] of STAGES) {
		chips += nodeSvg(nodes[id]);
		if (arrive[id] != null) {
			const n = nodes[id];
			const col = st === 'armed' ? P.magenta : id === 'upstream' ? '#d97757' : P.lineStrong;
			const hold = st === 'armed' ? 2.5 : id === 'upstream' ? 4.8 : 0.25;
			flashes += `<path d="${chamfer(n.x, n.y, n.w, n.h, 8)}" fill="none" stroke="${col}" stroke-width="2" class="glow" style="color:${col}" opacity="0">${fade(T, [[arrive[id], arrive[id] + hold]], 0.08)}</path>`;
		}
	}
	chips += `<path d="${chamfer(nodes.accept.x, nodes.accept.y, CW, CH, 8)}" fill="none" stroke="#5fe0a0" stroke-width="2" opacity="0">${fade(T, [[0.3, 1.6]])}</path>`;
	const trail = `<path d="${d}" fill="none" stroke="${P.lineStrong}" stroke-width="1.4" stroke-dasharray="3 5" opacity="0.6"/>`;
	const packet = `<g><animateMotion dur="${T}s" repeatCount="indefinite" path="${d}" keyPoints="${kpStr}" keyTimes="${ktStr}" calcMode="linear"/><circle r="11" fill="${C.req}" opacity="0.22"/><circle r="5.5" fill="${C.req}"/></g>`;
	const labels = `<text x="25" y="${Y1 - 10}" class="xs">stages_before_steerer() →</text><text x="975" y="${Y2 - 10}" text-anchor="end" class="xs">← stages_after_steerer(), then the composer</text>`;

	// body panels
	const by = 292;
	const base = [['system[0..1]', '"You are Claude Code…", <env>'], ['system[2]', '"# CLAUDE.md (project rules)"'], ['messages[0..4]', 'repeated Read + recall-tag history'], ['messages[5]', 'user: "run it again"'], ['tools', '[Bash, Read, Edit, …]']];
	const col = (x, head, color, sub) => `<text x="${x}" y="${by + 16}" class="sm" style="font-weight:700;letter-spacing:1.5px;fill:${color}">${head}</text><text x="${x}" y="${by + 32}" class="xs">${sub}</text>` +
		base.map(([k, v], i) => `<text x="${x}" y="${by + 52 + i * 16}" class="xs">${esc(k)}</text><text x="${x + 118}" y="${by + 52 + i * 16}" class="sm">${esc(v)}</text>`).join('');
	let bodies = `<path d="${chamfer(10, by - 8, 980, 180, 10)}" fill="${P.s1}" stroke="${P.line}"/><line x1="500" y1="${by}" x2="500" y2="${by + 164}" stroke="${P.line}"/>` +
		col(26, 'CAPTURED · stored_body', P.blue, 'logical inbound capture · body APIs expose fidelity') +
		col(516, 'FORWARDED UPSTREAM', P.magenta, 'whatever the last stage to fire produced');
	const adds = [['recall_injector', '+ <system-reminder> recalled lessons'], ['reminders', '+ queued reminders'], ['steerer', '+ <system-reminder> steerer note']];
	adds.forEach(([id, txt], i) => {
		bodies += `<g opacity="0">${fade(T, [[arrive[id] + 0.3, T - 0.45]])}<rect x="512" y="${by + 134 + i * 15 - 11}" width="470" height="14" fill="#2a1420"/><text x="516" y="${by + 134 + i * 15}" class="xs" style="fill:${P.magenta}">${esc(id)}</text><text x="634" y="${by + 134 + i * 15}" class="sm" style="fill:${P.t1}">${esc(txt)}</text></g>`;
	});
	// Read dedup changes only the forwarded history in this primary-turn example
	bodies += `<g opacity="0">${fade(T, [[arrive.read_dedup + 0.3, T - 0.45]])}<rect x="512" y="${by + 84 - 11}" width="470" height="14" fill="#2a1420"/><text x="516" y="${by + 84}" class="xs" style="fill:${P.magenta}">messages[0..4]</text><text x="634" y="${by + 84}" class="sm" style="fill:${P.t1}">history + dedup stub</text></g>`;
	const verdict = `<g opacity="0">${fade(T, [[arrive.upstream + 0.4, T - 0.45]])}<text x="484" y="${by + 16}" text-anchor="end" class="sm" style="fill:${P.green};font-weight:700">unchanged</text><text x="974" y="${by + 16}" text-anchor="end" class="sm" style="fill:${P.magenta};font-weight:700">4 stages fired · differs</text></g>`;

	const capY = HEAD + DH;
	let capsSvg = `<line x1="0" y1="${capY}" x2="${W}" y2="${capY}" stroke="${P.line}"/>`;
	caps.forEach((c) => {
		const [title, ...text] = INFO[c.id];
		capsSvg += `<g opacity="0">${fade(T, [[c.s, c.e - 0.05]])}<text x="24" y="${capY + 27}" class="cn">STAGE</text><text x="110" y="${capY + 27}" class="ct">${esc(title)}</text>` +
			text.map((ln, k) => `<text x="110" y="${capY + 49 + k * 17}" class="cx">${esc(ln)}</text>`).join('') + '</g>';
	});
	capsSvg += `<rect x="0" y="${capY + 90}" width="0" height="3" fill="${P.cyan}" opacity="0.7"><animate attributeName="width" dur="${T}s" repeatCount="indefinite" values="0;${W}" keyTimes="0;1"/></rect>`;

	const body = `<g transform="translate(0,${HEAD})">${labels}${trail}${chips}${flashes}${bodies}${verdict}${packet}</g>${capsSvg}`;
	return {
		svg: frame({
			W, H, heading: 'Request pipeline: an example with four active rewrite paths',
			title: 'mike request pipeline, animated',
			desc: 'Animated diagram of pipeline::STAGE_ORDER. A request packet passes accept, then filter, reminder_on_token, lazy_sections, recall_injector, lessons, self_identity, workspace_briefing, agent_lessons, reminders, the steerer queue drain, read_dedup, file_read_compression, proxy_tools and the composer, then goes upstream. In this primary-turn example recall_injector, reminders, the queue drain and read_dedup rewrite the forwarded body; the logical inbound capture never changes.'
		}, body), T
	};
}
function splitName(n) {
	const parts = n.split('_');
	let best = [n], score = 1e9;
	for (let k = 1; k < parts.length; k++) {
		const a = parts.slice(0, k).join('_') + '_', b = parts.slice(k).join('_');
		if (Math.max(a.length, b.length) < score) { score = Math.max(a.length, b.length); best = [a, b]; }
	}
	return best;
}

/* ═══ Scene: answer replaced (REQUEST_FLOW buffered forwarder) ══ */
function replaceScene() {
	const nodes = {
		up: { x: 30, y: 30, w: 170, h: 70, kind: 'vendor', label: 'Anthropic', sub: 'SSE stream' },
		fs: { x: 260, y: 30, w: 220, h: 70, kind: 'mike', label: 'forward_streaming', sub: 'weasel || tool_loop armed?' },
		tee: { x: 540, y: 30, w: 170, h: 70, kind: 'mike', label: 'plain tee', sub: 'default path, not taken' },
		client: { x: 810, y: 168, w: 160, h: 100, kind: 'outside', label: 'Claude Code', sub: 'renders the stream' },
		scan: { x: 30, y: 180, w: 170, h: 76, kind: 'mike', label: 'scanners', sub: ['DeltaScanner', 'ToolCallScanner'] },
		buf: { x: 260, y: 180, w: 220, h: 76, kind: 'mike', label: 'buffer', sub: ['Vec<Bytes>', 'max_buffer_bytes guard'] },
		sig: { x: 540, y: 180, w: 170, h: 76, kind: 'mike', label: 'InterceptSignal', sub: ['Clean · Hit', 'ProxyToolCall'] },
		fu: { x: 260, y: 330, w: 220, h: 76, kind: 'mike', label: 'follow-up request', sub: ['fresh RequestId + stub', 'mark_intercepted'] },
		up2: { x: 30, y: 330, w: 170, h: 76, kind: 'vendor', label: 'Anthropic', sub: 'asked again' }
	};
	const paths = {
		up2fs: { pts: ['up:r', 'fs:l'] },
		fs2tee: { pts: ['fs:r', 'tee:l'] },
		tee2c: { pts: ['tee:r', [890, 65], 'client:t'] },
		fs2buf: { pts: ['fs:b', 'buf:t'] },
		buf2scan: { pts: ['buf:l', 'scan:r'] },
		scan2sig: { pts: ['scan:t', [115, 150], [625, 150], 'sig:t'] },
		buf2sig: { pts: ['buf:r', 'sig:l'] },
		sig2c: { pts: ['sig:r', 'client:l'] },
		sig2fu: { pts: ['sig:b', [625, 368], 'fu:r'] },
		fu2up2: { pts: ['fu:l', 'up2:r'] },
		up22c: { pts: ['up2:b', [115, 422], [890, 422], 'client:b'] }
	};
	const steps = [
		{ d: 3, title: 'Weasel intercept is armed', focus: ['up', 'fs'], flows: [{ path: 'up2fs', color: C.res, label: 'SSE' }],
			text: ['The weasel_intercept toggle is on at /settings and the forbidden-words list is not empty (here: ["should work"]).', 'So forward_streaming buffers the stream instead of tee\'ing it.'] },
		{ d: 3.4, title: 'Buffered and scanned', focus: ['fs', 'buf', 'scan'],
			flows: [{ path: 'fs2buf', color: C.res, label: 'chunks' }, { path: 'buf2scan', color: C.req, label: 'each chunk', at: 1.1 }],
			text: 'Every chunk is held in a Vec<Bytes> and fed to the parser, where DeltaScanner watches the text as it arrives.' },
		{ d: 2.6, title: 'Signal: Hit', focus: ['scan', 'sig'], flows: [{ path: 'scan2sig', color: C.bad, label: 'Hit("should work")', dur: 1.4 }],
			text: 'At stream end the scanner sends one InterceptSignal over a oneshot. A forbidden word appeared: Hit(word).' },
		{ d: 2.8, title: 'The answer is thrown away', focus: ['buf', 'sig'], flows: [{ path: 'buf2sig', color: C.bad, label: 'discard' }],
			text: 'run_weasel_correction does not edit the answer; it discards the buffer. Claude Code never sees it.' },
		{ d: 3, title: 'A correction request', focus: ['sig', 'fu'], flows: [{ path: 'sig2fu', color: C.mut, label: 'correction body' }],
			text: ['New body = original request + the discarded text + correction_prompt, with a fresh RequestId and stub.', 'The original row is marked intercepted = true with a correction_request_id.'] },
		{ d: 4, title: 'The second answer is what Claude Code sees', focus: ['fu', 'up2', 'client'],
			flows: [{ path: 'fu2up2', color: C.req, label: 'follow-up' }, { path: 'up22c', color: C.res, label: 'second answer', at: 1.1, dur: 1.6 }],
			text: ['spawn_response_tee streams it to Claude Code and to its own parser and store updater. A ProxyToolCall', 'signal (proxy_tools) leaves the same way; a Clean signal just flushes the buffer to the client.'] }
	];
	const DH = 540;
	const panels = ({ T, steps }) => {
		const y = 450;
		const s2 = steps[1].at, s4 = steps[3].at, s6 = steps[5].at;
		const ans2 = 'Ran cargo test retry_ three times: 3 passed, 0 failed.';
		return `<path d="${chamfer(10, y - 6, 980, 84, 10)}" fill="${P.s1}" stroke="${P.line}"/><line x1="500" y1="${y}" x2="500" y2="${y + 72}" stroke="${P.line}"/>` +
			`<text x="26" y="${y + 14}" class="xs" style="letter-spacing:1.5px">HELD IN THE PROXY BUFFER</text><text x="516" y="${y + 14}" class="xs" style="letter-spacing:1.5px">WHAT CLAUDE CODE SEES</text>` +
			`<g opacity="0">${fade(T, [[s2 + 0.6, T - 0.45]])}<text x="26" y="${y + 38}" class="sm" style="fill:${P.t1}">Fixed the race in retry_test.rs.</text><text x="26" y="${y + 55}" class="sm" style="fill:${P.t1}">The suite <tspan fill="${P.red}" style="fill:${P.red}">should work</tspan> now.</text></g>` +
			`<g opacity="0">${fade(T, [[s4 + 0.6, T - 0.45]])}<line x1="24" y1="${y + 34}" x2="250" y2="${y + 34}" stroke="${P.red}" stroke-width="1.5"/><line x1="24" y1="${y + 51}" x2="210" y2="${y + 51}" stroke="${P.red}" stroke-width="1.5"/><text x="26" y="${y + 72}" class="xs" style="fill:${P.red}">discarded: never reaches Claude Code</text></g>` +
			`<g opacity="0">${fade(T, [[0.6, s6 + 2.4]])}<text x="516" y="${y + 38}" class="sm">waiting: nothing sent yet</text></g>` +
			`<g opacity="0">${fade(T, [[s6 + 2.6, T - 0.45]])}<text x="516" y="${y + 38}" class="sm" style="fill:${P.t1}">${esc(ans2)}</text><text x="516" y="${y + 58}" class="xs">the second upstream answer</text></g>` +
			'';
	};
	return diagramScene({
		W: 1000, DH, nodes, paths, steps, extra: panels,
		heading: 'When mike replaces the model\'s answer: a forbidden-word hit',
		title: 'mike weasel intercept, animated',
		desc: 'Animated diagram of the buffered forwarder. With weasel intercept armed, the SSE stream from Anthropic is buffered and scanned; a forbidden word produces InterceptSignal::Hit, the buffer is discarded, a correction request with a fresh RequestId is sent upstream, and the second answer is streamed to Claude Code.'
	});
}

/* ═══ Scene: two phases (REQUEST_FLOW two-phase DB write) ════════ */
function writesScene() {
	const LX = { client: 90, proxy: 300, up: 510, pg: 720, ui: 910 };
	const lane = (id, label, kind) => ({ x: LX[id] - 75, y: 8, w: 150, h: 48, kind, label, align: 'center', top: 29, c: 8 });
	const nodes = { client: lane('client', 'Claude Code', 'outside'), proxy: lane('proxy', 'mike proxy', 'mike'), up: lane('up', 'Anthropic', 'vendor'), pg: lane('pg', 'Postgres', 'store'), ui: lane('ui', 'Web UI', 'terminal') };
	const colOf = { req: 'mreq', res: 'mres', db: 'mdb', evt: 'mevt' };
	const arr = (a, b, y, text, kind) => ({ pts: [[LX[a] + (LX[b] > LX[a] ? 6 : -6), y], [LX[b] + (LX[b] > LX[a] ? -8 : 8), y]], text, marker: colOf[kind] });
	const loop = (x, y, text) => ({ pts: [[x + 6, y], [x + 48, y], [x + 48, y + 20], [x + 8, y + 20]], text, tpos: 0, tx: x + 56, tanchor: 'start', marker: 'mdb' });
	const paths = {
		e1: arr('client', 'proxy', 90, 'POST /v1/messages', 'req'),
		e2: loop(LX.proxy, 118, 'stored_body = inbound bytes'),
		e3: arr('proxy', 'pg', 178, 'Phase 1 · insert_request_initial → RequestId', 'db'),
		e4: arr('proxy', 'up', 220, 'forward', 'req'),
		e5: arr('up', 'proxy', 262, 'SSE chunks', 'res'),
		e6: arr('proxy', 'client', 296, 'tee', 'res'),
		e7: arr('proxy', 'pg', 336, 'stream_blocks · FK → stub', 'db'),
		e8: arr('proxy', 'pg', 378, 'ToolUseEnd → write_tool_use_row', 'db'),
		e9: arr('up', 'proxy', 420, 'message_stop', 'res'),
		e10: arr('proxy', 'pg', 462, 'Phase 2 · snapshot_terminal', 'db'),
		e11: loop(LX.pg, 488, 'create_instances_for_dispatches'),
		e12: arr('pg', 'ui', 548, 'NOTIFY → /api/ws', 'evt')
	};
	paths.e11.tx = LX.pg - 8; paths.e11.tanchor = 'end';
	const f = (path, color) => [{ path, color, dur: 0.9 }];
	const steps = [
		{ d: 1.8, title: 'Request arrives', focus: ['client', 'proxy'], flows: f('e1', C.req), text: 'A captured request needs a canonical session-id header; without one it is forwarded with no row at all.' },
		{ d: 1.8, title: 'Original bytes kept', focus: ['proxy'], flows: f('e2', C.db), text: 'engine::accept keeps the untouched inbound body before any stage runs.' },
		{ d: 2.2, title: 'Phase 1: stub insert, before the upstream call', focus: ['proxy', 'pg'], flows: f('e3', C.db), text: 'With persistence, insert_stub_and_mint_id establishes the durable RequestId; failed writes can leave gaps.' },
		{ d: 1.6, title: 'Forward upstream', focus: ['proxy', 'up'], flows: f('e4', C.req), text: 'Only now does engine::forward_to_upstream send.' },
		{ d: 1.6, title: 'SSE chunks arrive', focus: ['up', 'proxy'], flows: f('e5', C.res), text: 'The parser task updates the in-memory InterceptedRequest as events arrive.' },
		{ d: 1.6, title: 'Tee to the client', focus: ['proxy', 'client'], flows: f('e6', C.res), text: 'On the plain path Claude Code gets each chunk as it arrives.' },
		{ d: 1.8, title: 'Canonical stream blocks', focus: ['proxy', 'pg'], flows: f('e7', C.db), text: 'Canonical responses.stream_blocks writes land between the phases; they reference the durable request.' },
		{ d: 1.8, title: 'Tool calls, as each one finishes', focus: ['proxy', 'pg'], flows: f('e8', C.db), text: 'write_tool_use_row fires per ToolUseEnd event, before the terminal snapshot.' },
		{ d: 1.6, title: 'Stream ends', focus: ['up', 'proxy'], flows: f('e9', C.res), text: 'Non-streaming responses reach the same point through complete_non_streaming.' },
		{ d: 2.2, title: 'Phase 2: terminal snapshot', focus: ['proxy', 'pg'], flows: f('e10', C.db), text: 'The terminal snapshot persists response and diagnostics, plus lifecycle/projection updates; not one SQL write.' },
		{ d: 2, title: 'Agent instances (primary turns)', focus: ['pg'], flows: f('e11', C.db), text: 'Creates agent_instances rows and back-fills sub-agent requests. Soft-fails; never breaks the hot path.' },
		{ d: 3, title: 'The UI is told', focus: ['pg', 'ui'], flows: f('e12', C.evt), text: 'NOTIFY proxy.row_changed → db_listener → ProxyEvent::RowChanged → every /api/ws subscriber.' }
	];
	const marker = (id, color) => `<marker id="${id}" viewBox="0 0 10 10" refX="9" refY="5" markerWidth="7" markerHeight="7" orient="auto-start-reverse"><path d="M0,0 L10,5 L0,10 z" fill="${color}"/></marker>`;
	const defs = `<defs>${marker('mreq', C.req)}${marker('mres', C.res)}${marker('mdb', C.db)}${marker('mevt', C.evt)}</defs>`;
	const under = () => Object.values(LX).map((x) => `<path d="M${x},56 V575" stroke="${P.line}" stroke-dasharray="2 5" fill="none"/>`).join('');
	return diagramScene({
		W: 1000, DH: 585, nodes, paths, steps, defs, under,
		heading: 'Two capture phases, with incremental writes between them',
		title: 'mike two-phase capture write, animated',
		desc: 'Animated sequence diagram. The proxy keeps the inbound bytes, attempts a Phase-1 stub row before forwarding, writes canonical stream blocks and tool-use rows as the SSE stream arrives, writes a Phase-2 terminal snapshot at message_stop, creates agent instances on primary turns, and a NOTIFY updates the web UI.'
	});
}

/* ═══ Scene: boot and shutdown (STARTUP_FLOW) ═══════════════════ */
function bootScene() {
	const W = 1000, DH = 650, H = HEAD + DH + 96;
	const steps = [
		{ d: 1.6, title: '.env before anything', log: [['load_dotenv_preserving_db_url()', 'first statement in main(), ahead of clap']], text: 'main() loads .env before Cli::parse(), keeping an already-set database URL.' },
		{ d: 2.2, title: 'Dispatch the subcommand', log: [['Cli::parse()', '→ serve'], [null, 'bare mike: TTY → TUI · no TTY → serve'], [null, 'spawn · claude · print-config → short-circuit']], text: 'A bare mike opens the TUI on a TTY and serves headless otherwise. mike serve always serves.' },
		{ d: 2, title: 'Config and tracing', log: [['config::load', 'defaults < MIKE_* env < CLI flags'], ['init_tracing_and_log_startup', 'json | pretty · OTel · daily file']], text: 'Boot config is a figment chain. The runtime source of truth for toggles is /settings.' },
		{ d: 1.6, title: 'AppState::new', log: [['AppState::new', 'pg pool · store · channels · CancellationToken']], text: 'Initializes persistence when configured and builds the in-memory store, channels, task tracker and shutdown token.' },
		{ d: 3.4, title: 'Startup sequence, before any port binds', log: [['run_crash_recovery', 'close stale Pending rows'], ['record_build_identity', 'build + code defaults'], ['spawn_bundle_retry', 'when publisher is present'], ['run_startup_session_sweep', 'skipped: sweep_at_startup = false', 'w'], ['run_mailbox_reload', 'mailbox → reminder queue'], ['run_composable_session_warmup', 'warm composable_session_index'], ['emit_startup_banner', 'byte budgets · pool dims']], text: ['Loopback warnings, crash recovery, build identity, bundle retry, session sweep, mailbox reload and warmup.', 'Recovery is attempted before listeners bind; a recovery failure is logged and boot continues.'] },
		{ d: 1.6, title: 'LISTEN for row changes', log: [['db_listener::spawn_listener', 'LISTEN proxy.row_changed']], text: 'This task later turns every capture write into a live /api/ws event.' },
		{ d: 3.6, title: 'Eighteen worker entry points', log: [['spawn_background_workers', '18 calls; spawn-time or work gates']], workers: true, text: ['Some gates prevent spawning; others disable work inside a running task. Defaults assume dependencies.', 'OpenRouter paths are opt-in; embedding and database destinations are configured separately.'] },
		{ d: 1.8, title: 'Observability tasks', log: [['spawn_observability_tasks', 'pruner · gauges · 5s ticker · RSS cap']], text: 'The cap manager sheds load in priority order past the RSS high-watermark; with rss_ceiling_bytes = 0 it only observes.' },
		{ d: 2.6, title: 'Bind the listeners', log: [['listeners::bind_and_log', ':9988 · :9989 · :9990']], ports: true, text: ':9988 and :9989 always bind; :9990 only when spa_enabled (start-proxy.sh hands it to vite dev instead).' },
		{ d: 2, title: 'Signal handler and hard backstop', log: [['install_signal_handler', 'SIGINT · SIGTERM · SIGHUP'], ['spawn_hard_deadline_backstop', 'SHUTDOWN_HARD_DEADLINE_SECS = 60']], text: 'SIGHUP counts too: a closed controlling terminal shuts mike down cleanly.' },
		{ d: 2, title: 'Serving', log: [['serving', 'axum::serve × 2 + SPA future, try_join!']], text: 'The proxy and inspect binds each run a real axum::serve; the SPA slot is a no-op future when disabled.' },
		{ d: 4, title: 'SIGTERM: the drain race', log: [['SIGTERM', '→ cancel token', 'x'], ['race_serve_against_drain_deadline', '+3s shutdown starts · 10s drain deadline'], ['wait_for_observability_tasks', '≤ 30s each']], shutdown: true, text: ['Axum begins graceful shutdown 3s after cancellation; the serving join has a 10s deadline from cancellation.', 'Task waits are capped at 30s each; the process has a 60s hard backstop. Timing here is illustrative.'] },
		{ d: 2.6, title: 'Exit', log: [['flush_otel', 'no-op unless otel_endpoint was set'], ['exit 0', 'hard backstop aborted']], exit: true, text: 'OTel is flushed last. The next boot starts with crash recovery, in case this exit was not clean.' }
	];
	const { T, caps } = timeline(steps);
	const end = T - 0.45;
	// log panel
	let logSvg = `<path d="${chamfer(20, 10, 590, 536, 10)}" fill="${P.bg}" stroke="${P.line}"/>`;
	let row = 0;
	const LH = 15;
	let shutdownAt = 0, workersAt = 0, portsAt = 0, exitAt = 0;
	steps.forEach((st) => {
		if (st.workers) workersAt = st.at;
		if (st.ports) portsAt = st.at;
		if (st.shutdown) shutdownAt = st.at;
		if (st.exit) exitAt = st.at;
		st.log.forEach(([fn, c, tone], k) => {
			const y = 32 + row * LH;
			const t = st.at + 0.15 + k * 0.32;
			const fnCol = tone === 'x' ? P.red : '#5fe0a0';
			const cCol = tone === 'w' ? P.yellow : P.t3;
			logSvg += `<g opacity="0">${fade(T, [[t, end]], 0.08)}<text x="34" y="${y}" class="xs" style="fill:${P.lineStrong}">›</text>` +
				(fn ? `<text x="48" y="${y}" class="xs" style="fill:${fnCol}">${esc(fn)}<tspan dx="8" style="fill:${cCol}">${esc(c)}</tspan></text>` : `<text x="48" y="${y}" class="xs" style="fill:${cCol}">${esc(c)}</text>`) + '</g>';
			row++;
		});
	});
	// ports
	const px = 630;
	let right = `<text x="${px}" y="22" class="xs" style="letter-spacing:1.5px">LISTENERS</text>`;
	[':9988 proxy', ':9989 inspect', ':9990 SPA (spa_enabled)'].forEach((lbl, i) => {
		const y = 32 + i * 34;
		const on = portsAt + 0.3 + i * 0.35;
		right += `<path d="${chamfer(px, y, 350, 28, 6)}" fill="${P.s2}" stroke="${P.line}"/><circle cx="${px + 16}" cy="${y + 14}" r="5" fill="${P.line}"/>` +
			`<circle cx="${px + 16}" cy="${y + 14}" r="5" fill="${P.green}" class="glow" style="color:${P.green}" opacity="0">${fade(T, [[on, shutdownAt + 1.6]])}</circle>` +
			`<circle cx="${px + 16}" cy="${y + 14}" r="5" fill="${P.red}" opacity="0">${fade(T, [[shutdownAt + 1.7, end]])}</circle>` +
			`<text x="${px + 32}" y="${y + 18}" class="sm">${esc(lbl)}</text>`;
	});
	// workers
	const WK = [['lesson_worker', 1], ['resync', 1], ['embed_backfill', 1], ['semantic', 1], ['assembled_body_backfill', 1], ['judge_worker', 0, 1], ['dossier_builder', 0], ['steerer_fleet', 0, 1], ['quiescence', 0, 1], ['session_judge', 0, 1], ['mailbox_reaper', 1], ['work_registry', 1], ['eval_metrics', 1], ['cross_session', 1], ['channel_link_sweep', 1], ['remote_control_repush', 1], ['workspace_reaper', 1], ['retention_sweep', 1]];
	right += `<text x="${px}" y="148" class="xs" style="letter-spacing:1.5px">WORKERS · DEFAULT WORK GATES</text>`;
	WK.forEach(([n, on, eg], i) => {
		const cx = px + (i % 2) * 177, cy = 158 + Math.floor(i / 2) * 39;
		const t = workersAt + 0.3 + i * 0.2;
		right += `<g opacity="0.3"><path d="${chamfer(cx, cy, 172, 34, 6)}" fill="${P.s2}" stroke="${P.line}"/><text x="${cx + 8}" y="${cy + 14}" class="xs" style="fill:${P.t2}">${esc(n)}</text></g>` +
			`<g opacity="0">${fade(T, [[t, exitAt + 0.3]])}<path d="${chamfer(cx, cy, 172, 34, 6)}" fill="${P.s2}" stroke="${on ? '#2b7a52' : P.line}"/><text x="${cx + 8}" y="${cy + 14}" class="xs" style="fill:${P.t1}">${esc(n)}</text>` +
			`<text x="${cx + 8}" y="${cy + 27}" class="xs" style="fill:${on ? P.green : P.t3};letter-spacing:1px">${on ? 'ON / GATED' : 'OFF'}</text>${eg ? `<text x="${cx + 164}" y="${cy + 27}" text-anchor="end" class="xs" style="fill:${P.yellow}">OpenRouter</text>` : ''}</g>`;
	});
	// shutdown bar
	const by = 580, bx = 20, bw = 960;
	let bar = `<text x="${bx}" y="${by - 10}" class="xs" style="letter-spacing:1.5px">SHUTDOWN DEADLINES, FROM THE CANCEL SIGNAL</text><rect x="${bx}" y="${by}" width="${bw}" height="26" fill="${P.s2}" stroke="${P.line}"/>`;
	bar += `<rect x="${bx}" y="${by}" width="0" height="26" fill="${P.yellow}" opacity="0.35"><animate attributeName="width" dur="${T}s" repeatCount="indefinite" values="0;0;${bw * 0.07};${bw * 0.07};0" keyTimes="0;${n3((shutdownAt + 0.4) / T)};${n3((shutdownAt + 2.8) / T)};${n3(end / T)};1"/></rect>`;
	[[3, '+3s drain starts'], [10, '10s GRACEFUL_DRAIN_SECS'], [40, 'up to 30s task waits'], [60, '60s hard exit']].forEach(([s, lbl]) => {
		const x = bx + (bw * s) / 60;
		const hard = s === 60;
		bar += `<line x1="${x}" y1="${by - 2}" x2="${x}" y2="${by + 30}" stroke="${hard ? P.red : P.lineStrong}" stroke-dasharray="3 3"/><text x="${hard ? x - 4 : x + 4}" y="${by + 42}" text-anchor="${hard ? 'end' : 'start'}" class="xs" style="fill:${hard ? P.red : P.t3}">${esc(lbl)}</text>`;
	});
	bar += `<g opacity="0">${fade(T, [[shutdownAt + 2.9, end]])}<line x1="${bx + bw * 0.07}" y1="${by - 4}" x2="${bx + bw * 0.07}" y2="${by + 30}" stroke="${P.green}" stroke-width="2"/><text x="${bx + bw * 0.07 + 4}" y="${by - 6}" class="xs" style="fill:${P.green}">returned</text></g>`;
	const body = `<g transform="translate(0,${HEAD})">${logSvg}${right}${bar}</g>` + captionSvg(T, caps, W, HEAD + DH);
	return {
		svg: frame({
			W, H, heading: 'From mike serve to exit',
			title: 'mike startup and shutdown, animated',
			desc: 'Animated boot log of mike serve: .env loading, subcommand dispatch, config and tracing, AppState::new, the pre-bind startup sequence, the Postgres LISTEN task, eighteen background worker entry points with their default work gates, observability tasks, binding ports 9988, 9989 and 9990, the signal handler, serving, and a SIGTERM shutdown with graceful shutdown starting at 3s, a 10s serve deadline and a 60s hard backstop.'
		}, body), T
	};
}

const scenes = {
	'mike-overview': mapScene(false),
	'system-map': mapScene(true),
	'mutation-stages': stagesScene(),
	'answer-replaced': replaceScene(),
	'two-phase-write': writesScene(),
	'boot-and-shutdown': bootScene()
};
for (const [name, { svg, T }] of Object.entries(scenes)) {
	writeFileSync(new URL(`${name}.svg`, OUT), svg);
	console.log(`${name}.svg  ${(svg.length / 1024).toFixed(1)} KB  loop ${T}s`);
}
