// One function per picture: drive the page to the state the guide describes, then `shoot` clips
// the part its page talks about. The pictures are read on temet.ai at about 775 px wide on a
// desktop and 358 px on a phone, so each one is a close crop, not a full screen:
//   - VIEW is the browser window. The default is a 560 px phone-width window: the web UI folds
//     its left menu away and sets the text in one column, so a clip of a few hundred pixels reads
//     at full size. A picture that needs the desktop layout (the menu, the wide request list, a
//     canvas) names a wider window.
//   - CLIPS picks the rectangle to keep (page coordinates, after the shot function ran). With no
//     entry the page's main area from the top bar down is kept. A clip is at most 800 CSS px wide
//     so that at scale 2 the PNG stays within 1600 px.
//   - The producer (scripts/guide-shots) calls `shoot` for each name.
import { execFileSync } from 'node:child_process';
import { join } from 'node:path';
import { DEMO } from './guide-shots-seed.mjs';
import { ansiToSvg } from './guide-shots-term.mjs';

const settle = async (page, ms = 900) => { await page.waitForLoadState('networkidle').catch(() => {}); await page.waitForTimeout(ms); };
const go = async (page, ctx, path) => { await page.goto(`${ctx.spa}${path}`); await settle(page); };
const session = `/sessions/${DEMO.session}`;

const requestIds = async (ctx) => (await (await fetch(`${ctx.inspect}/api/sessions/${DEMO.session}/requests`)).json()).requests.map((r) => r.id).sort((x, y) => x - y);

// Page coordinates of an element (the window may have scrolled).
async function box(page, locator) {
  const b = await locator.first().boundingBox();
  if (!b) throw new Error('element not on the page');
  const s = await page.evaluate(() => ({ x: window.scrollX, y: window.scrollY }));
  return { x: b.x + s.x, y: b.y + s.y, width: b.width, height: b.height };
}
const main = (page) => box(page, page.locator('#main-content'));
// The rectangle between two elements' tops/bottoms, over the main area's width.
const band = async (page, { from, to, toEdge = 'bottom', pad = 0, x, width }) => {
  const m = await main(page);
  const a = from ? (await box(page, from)).y : m.y;
  const t = to ? await box(page, to) : null;
  const end = t ? (toEdge === 'top' ? t.y : t.y + t.height) : m.y + m.height;
  return { x: x ?? m.x, y: Math.max(0, a - pad), width: width ?? Math.min(m.width, 800), height: end - a + pad * 2 };
};

export const SHOTS = {
  // The only picture about the menu: the desktop layout, the menu beside the first screen.
  // The menu fades in after the page loads; wait for it so it is not caught half drawn.
  home: async (page, ctx) => { await go(page, ctx, '/'); await page.waitForTimeout(2500); },
  sessions: (page, ctx) => go(page, ctx, '/sessions'),
  // The README's first picture: the guide's search, the correction turn open with its tool call
  // and Claude's reply.
  'session-timeline': async (page, ctx) => {
    await go(page, ctx, `${session}/requests/${(await requestIds(ctx))[2]}`);
    await page.getByPlaceholder(/Search prompts/).fill('Homebrew');
    await settle(page, 700);
  },
  request: async (page, ctx) => {
    await go(page, ctx, `${session}/requests/${(await requestIds(ctx))[0]}?drawer=request`);
    await page.getByText('Diff', { exact: true }).first().click();
    await settle(page, 900);
  },
  projects: (page, ctx) => go(page, ctx, `/projects/${encodeURIComponent(DEMO.folder)}`),
  'flagged-response': async (page, ctx) => { await go(page, ctx, '/analysis'); await page.locator('.an-badge', { hasText: /justifying shortcuts/i }).click(); await settle(page, 500); await page.getByText('5b6c2a1e').first().click(); await settle(page, 700); },
  prompts: (page, ctx) => go(page, ctx, '/prompts'),
  memory: (page, ctx) => go(page, ctx, '/memory'),
  steering: async (page, ctx) => { await go(page, ctx, '/steering'); await page.getByRole('tab', { name: /fleet/i }).click().catch(() => page.getByText(/^Fleet/).first().click()); await settle(page, 600); },
  judge: (page, ctx) => go(page, ctx, '/sessions/labelling'),
  'filter-workbench': async (page, ctx) => { await go(page, ctx, '/filter/workbench'); await page.getByText('Claude Code 2.1.0').first().click(); await settle(page); },
  'compression-review': (page, ctx) => go(page, ctx, '/compression-review'),
  comms: async (page, ctx) => { await go(page, ctx, '/comms'); await page.getByText(DEMO.folder).first().click(); await settle(page, 600); await page.getByText('Install page: which method first?').first().click().catch(() => {}); await settle(page, 600); },
  fleet: (page, ctx) => go(page, ctx, '/fleet'),
  settings: (page, ctx) => go(page, ctx, '/settings'),
  'settings-setup': (page, ctx) => go(page, ctx, '/settings/setup'),
  // The terminal app in a real pty (tmux), drawn to an SVG for the producer to rasterise. Fewer,
  // larger cells than a full-width terminal: 96 columns at 16 px is 1536 px wide, so the letters
  // are about one and a half times the size they were.
  terminal: async (page, ctx) => {
    const sock = `guide-shots-${process.pid}`;
    const tmux = (...a) => execFileSync('tmux', ['-L', sock, ...a], { encoding: 'utf8' });
    const pause = (ms) => new Promise((r) => setTimeout(r, ms));
    const env = `HOME=${ctx.work} XDG_CONFIG_HOME=${ctx.work}/cfg XDG_STATE_HOME=${ctx.work}/state PATH=${ctx.work}/bin:/usr/bin:/bin TERM=xterm-256color LANG=C.UTF-8`;
    try {
      tmux('new-session', '-d', '-x', String(TERM.cols), '-y', String(TERM.rows), '-s', 't', `env -i ${env} ${ctx.work}/bin/mike --config ${join(ctx.work, 'config.toml')}`);
      await pause(4000);
      tmux('send-keys', '-t', 't', 'Down', 'Down', 'Down');
      await pause(800);
      tmux('send-keys', '-t', 't', 'Enter');
      await pause(3000);
      return { svg: ansiToSvg(tmux('capture-pane', '-e', '-p', '-t', 't'), TERM.cols, TERM.rows, TERM.cw, TERM.ch) };
    } finally {
      try { tmux('kill-server'); } catch {}
    }
  },
};

export const TERM = { cols: 96, rows: 20, cw: 16, ch: 34 };

// The browser window of each picture; anything not listed is a 560 px phone-width window.
const DEFAULT_VIEW = { width: 560, height: 1400, scale: 2 };
export const VIEW = {
  home: { width: 800, height: 900 },
  // The correction's prompt is cut short in a narrower list, so this one keeps a wide window
  // and a scale of 1.75 (a 900 px clip stays within 1600 px).
  'session-timeline': { width: 1200, scale: 1.75 },
  'filter-workbench': { width: 760, height: 900 },
  fleet: { width: 1100, height: 800 },
};

// Rectangles to keep, in page coordinates. Nothing here is a fixed pixel offset from a guess:
// each is bounded by elements of the page, so it follows the layout.
export const CLIPS = {
  home: async () => ({ x: 0, y: 0, width: 800, height: 720 }),
  sessions: (page) => band(page, { to: page.getByText('LOAD OLDER SESSIONS'), pad: 8 }),
  'session-timeline': (page) => band(page, { from: page.getByText(/^requests$/i), to: page.getByText('Homebrew is first now'), pad: 14, x: 276, width: 900 }),
  request: async (page) => box(page, page.getByRole('dialog')),
  projects: (page) => band(page, { to: page.getByText('turn:1'), pad: 8 }),
  'flagged-response': (page) => band(page, { from: page.locator('section.an-panel').first(), pad: 8 }),
  prompts: (page) => band(page, { from: page.locator('.pr-panel'), pad: 8 }),
  memory: (page) => band(page, { to: page.locator('.rs-strip'), pad: 8 }),
  steering: (page) => band(page, { from: page.getByText('STEERER FLEET'), to: page.getByText('Memory keeper', { exact: true }), toEdge: 'top' }),
  judge: (page) => band(page, { to: page.locator('.page-strip'), pad: 8 }),
  // The outline with the one edit in it; the edits list beside it repeats the same line.
  'filter-workbench': (page) => band(page, { from: page.getByText('Billing header', { exact: true }), to: page.getByText(/most common path first/), pad: 14, x: 0, width: 540 }),
  'compression-review': (page) => band(page, { to: page.getByText('SAVE REVIEW'), pad: 8 }),
  comms: (page) => band(page, {}),
  // The canvas down to the second session, without the inspector beside it.
  fleet: async (page) => { const c = await box(page, page.locator('.fleet-canvas-wrap')); const n = await box(page, page.getByText('8e1f7d52', { exact: true })); return { x: c.x, y: c.y, width: c.width, height: n.y + n.height + 40 - c.y }; },
  settings: (page) => band(page, { from: page.locator('#capability-status'), to: page.getByText(/Enable embedding.enabled at/), pad: 14 }),
  'settings-setup': (page) => band(page, { from: page.getByText('postgres version', { exact: true }), to: page.locator('#setup-database'), pad: 8 }),
};

// Photograph one picture into `file`. The producer and a developer run both come through here.
export async function shoot(browser, name, ctx, file, svgToPng) {
  const view = { ...DEFAULT_VIEW, ...(VIEW[name] ?? {}) };
  const page = await browser.newPage({ viewport: { width: view.width, height: view.height }, deviceScaleFactor: view.scale ?? 2 });
  try {
    const made = await SHOTS[name](page, ctx);
    if (made?.svg) { svgToPng(made.svg, file); return; }
    await page.evaluate(() => window.scrollTo(0, 0));
    const clip = CLIPS[name] ? await CLIPS[name](page, ctx) : undefined;
    // A full-page capture resizes the window, and the menu then fades out while it is taken;
    // a clip that fits the window is taken as it stands.
    const fits = clip && clip.y + clip.height <= view.height;
    await page.screenshot({ path: file, fullPage: !fits, ...(clip ? { clip } : {}) });
  } finally { await page.close(); }
}
