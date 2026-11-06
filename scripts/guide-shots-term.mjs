// Turn a tmux pane (captured with colours) into a PNG: parse the SGR codes, draw one SVG
// text run per coloured span on a cell grid, and let rsvg-convert rasterise it.
import { execFileSync } from 'node:child_process';

const PALETTE = ['#1d1f21', '#cc6666', '#b5bd68', '#f0c674', '#81a2be', '#b294bb', '#8abeb7', '#c5c8c6',
  '#666666', '#d54e53', '#b9ca4a', '#e7c547', '#7aa6da', '#c397d8', '#70c0b1', '#eaeaea'];
const BG = '#0f1117', FG = '#d6d8dc';

function color256(n) {
  if (n < 16) return PALETTE[n];
  if (n >= 232) { const v = 8 + (n - 232) * 10; return `rgb(${v},${v},${v})`; }
  const i = n - 16, level = (x) => (x === 0 ? 0 : 55 + x * 40);
  return `rgb(${level(Math.floor(i / 36))},${level(Math.floor(i / 6) % 6)},${level(i % 6)})`;
}

function parse(ansi, cols) {
  const rows = [];
  for (const line of ansi.split('\n')) {
    const cells = [];
    let st = { fg: null, bg: null, bold: false, reverse: false };
    const re = /\x1b\[([0-9;:]*)m|\x1b\[[0-9;?]*[A-Za-z]|([^\x1b])/g;
    let m;
    while ((m = re.exec(line))) {
      if (m[2] !== undefined) { cells.push({ ch: m[2], ...st }); continue; }
      if (m[1] === undefined) continue;
      const p = m[1].split(/[;:]/).map((x) => (x === '' ? 0 : Number(x)));
      for (let i = 0; i < p.length; i++) {
        const c = p[i];
        if (c === 0) st = { fg: null, bg: null, bold: false, reverse: false };
        else if (c === 1) st.bold = true; else if (c === 22) st.bold = false;
        else if (c === 7) st.reverse = true; else if (c === 27) st.reverse = false;
        else if (c >= 30 && c <= 37) st.fg = PALETTE[c - 30]; else if (c >= 90 && c <= 97) st.fg = PALETTE[c - 90 + 8];
        else if (c >= 40 && c <= 47) st.bg = PALETTE[c - 40]; else if (c >= 100 && c <= 107) st.bg = PALETTE[c - 100 + 8];
        else if (c === 39) st.fg = null; else if (c === 49) st.bg = null;
        else if ((c === 38 || c === 48) && p[i + 1] === 5) { const v = color256(p[i + 2]); if (c === 38) st.fg = v; else st.bg = v; i += 2; }
        else if ((c === 38 || c === 48) && p[i + 1] === 2) { const v = `rgb(${p[i + 2]},${p[i + 3]},${p[i + 4]})`; if (c === 38) st.fg = v; else st.bg = v; i += 4; }
      }
    }
    while (cells.length < cols) cells.push({ ch: ' ', fg: null, bg: null, bold: false, reverse: false });
    rows.push(cells);
  }
  return rows;
}

const esc = (s) => s.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;');

export function ansiToSvg(ansi, cols, rowsWanted, cw = 10, ch = 21) {
  const rows = parse(ansi.replace(/\n$/, ''), cols).slice(0, rowsWanted);
  const width = cols * cw, height = rows.length * ch;
  let out = `<svg xmlns="http://www.w3.org/2000/svg" width="${width}" height="${height}" viewBox="0 0 ${width} ${height}"><rect width="100%" height="100%" fill="${BG}"/>`;
  rows.forEach((cells, y) => {
    let x = 0;
    while (x < cells.length) {
      const c = cells[x];
      const fg = (c.reverse ? c.bg ?? BG : c.fg ?? FG), bg = c.reverse ? c.fg ?? FG : c.bg;
      let end = x + 1;
      while (end < cells.length && cells[end].fg === c.fg && cells[end].bg === c.bg && cells[end].bold === c.bold && cells[end].reverse === c.reverse) end++;
      if (bg) out += `<rect x="${x * cw}" y="${y * ch}" width="${(end - x) * cw}" height="${ch}" fill="${bg}"/>`;
      const text = cells.slice(x, end).map((q) => q.ch).join('');
      if (text.trim()) out += `<text x="${x * cw}" y="${y * ch + ch - 6}" fill="${fg}" font-family="DejaVu Sans Mono, monospace" font-size="${Math.min(ch - 5, Math.round(cw / 0.6))}" ${c.bold ? 'font-weight="bold"' : ''} xml:space="preserve" textLength="${(end - x) * cw}" lengthAdjust="spacingAndGlyphs">${esc(text)}</text>`;
      x = end;
    }
  });
  return `${out}</svg>`;
}

export function svgToPng(svg, file) {
  execFileSync('rsvg-convert', ['-o', file], { input: svg });
}
