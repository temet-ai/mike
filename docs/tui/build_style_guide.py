#!/usr/bin/env python3
"""Generate docs/tui/STYLE_GUIDE.html — designed rendition of the TUI design system.
Terminal mockups are built from segment lists with width assertions so box-drawing
alignment is verified at build time, not by eye."""
import html as H
import sys
from pathlib import Path

OUT = str(Path(__file__).resolve().parent / "STYLE_GUIDE.html")
# Optional argv[1]: also emit the Claude-artifact fragment (no doctype/head/body wrapper).
OUT_ARTIFACT = sys.argv[1] if len(sys.argv) > 1 else None

import base64 as _b64
from pathlib import Path as _P
_FONT = _b64.b64encode((_P(__file__).resolve().parent / "assets/term-mono.woff2").read_bytes()).decode()
FONT_RULES = (
    "/* Display rules for terminal mockups: embedded mono font (subset DejaVu Sans Mono),\n"
    "   line-height 1.0 so borders join, text-size-adjust off, fit-to-viewport font-size\n"
    "   ((100vw - chrome) / (cols x 0.602em)) so a screen never clips or scrolls sideways. */\n"
    "@font-face{font-family:'TermMono';src:url(data:font/woff2;base64," + _FONT + ") format('woff2');font-display:block}\n"
    "html{-webkit-text-size-adjust:100%;text-size-adjust:100%}\n"
)

# ---------- mockup machinery ----------

def esc(t):
    return H.escape(t, quote=False)

def render_line(segs, width=None):
    """segs: list of (cls_or_None, text). Returns (visible_len, html)."""
    vis = "".join(t for _, t in segs)
    out = "".join(
        (f'<span class="{c}">{esc(t)}</span>' if c else esc(t)) for c, t in segs
    )
    if width is not None:
        pad = width - len(vis)
        assert pad >= 0, f"line overflow by {-pad}: {vis!r}"
        out += " " * pad
        vis += " " * pad
    return len(vis), out

def boxline(border_cls, title, width):
    """Top border with embedded title: ┌─ title ────┐"""
    if title:
        head = "┌─ "
        tail_len = width - len(head) - len(title) - 2  # trailing space + ┐
        assert tail_len >= 0
        _, th = render_line([("tt", title)])
        return (
            f'<span class="{border_cls}">{esc(head)}</span>'
            + th
            + f'<span class="{border_cls}">{esc(" " + "─" * tail_len + "┐")}</span>'
        )
    return f'<span class="{border_cls}">{esc("┌" + "─" * (width - 2) + "┐")}</span>'

def botline(border_cls, width):
    return f'<span class="{border_cls}">{esc("└" + "─" * (width - 2) + "┘")}</span>'

def panel(border_cls, title, width, rows, height):
    """Full panel as list of html strings, each exactly `width` visible chars."""
    inner = width - 2
    out = [boxline(border_cls, title, width)]
    body = list(rows)
    while len(body) < height - 2:
        body.append([])
    assert len(body) == height - 2, f"panel {title}: {len(body)} rows > {height-2}"
    for segs in body:
        vl, h = render_line(segs, inner)
        out.append(
            f'<span class="{border_cls}">│</span>' + h + f'<span class="{border_cls}">│</span>'
        )
    out.append(botline(border_cls, width))
    assert len(out) == height
    return out

# ---------- hero mockup: universal layout, labelled mike application example at 80x24 ----------

def sgt(name):
    return [("sc", name[0]), ("tt", name[1:])]

def sg_toprow(bcls, width, title_segs):
    tlen = sum(len(t) for _, t in title_segs)
    tail = width - 3 - tlen - 2
    assert tail >= 0, f"title too long for {width}"
    _, th = render_line(title_segs)
    return (f'<span class="{bcls}">{esc("┌─ ")}</span>' + th
            + f'<span class="{bcls}">{esc(" " + "─" * tail + "┐")}</span>')

def sg_panel(bcls, title_segs, width, rows, height):
    inner = width - 2
    out = [sg_toprow(bcls, width, title_segs)]
    body = list(rows)
    while len(body) < height - 2:
        body.append([])
    assert len(body) == height - 2, f"panel too tall: {len(body)} > {height-2}"
    for r in body:
        cls = None
        if isinstance(r, tuple):
            cls, r = r
        _, h = render_line(r, inner)
        if cls:
            h = f'<span class="{cls}">{h}</span>'
        out.append(f'<span class="{bcls}">│</span>' + h + f'<span class="{bcls}">│</span>')
    out.append(botline(bcls, width))
    assert len(out) == height
    return out

PH = 22

hero_nav = sg_panel("bd", sgt("nav"), 12, [
    [(None, " traffic")],
    ("act", [(None, " sessions")]),
    [(None, " steering")],
    [(None, " memory")],
    [(None, " settings")],
], PH)

hero_mid = sg_panel("bdf", sgt("sessions"), 34, [
    [("mut", " name      branch   state  turns")],
    ("act", [(None, " opus-48   main     live     142")]),
    [(None, " sonnet-5  review   "), ("ok", "live"), (None, "      "), ("num", "89")],
    [(None, " haiku-45  fleet-3  "), ("warn", "busy"), (None, "      "), ("num", "57")],
    [(None, " sonnet-4  scratch  "), ("mut", "idle"), (None, "       "), ("num", "3")],
    [],
    [("mut", " traffic")],
    [("mut", " rx "), ("cy", "▁▂▄▆██▆▄▂▁▂▄▆█▆▄▂▁"), (None, "  "), ("num", "1.2 MB/s")],
    [("mut", " tx "), ("pk", "▁▁▂▄▆▆▄▂▁▁▁▂▄▆▄▂▁▁"), (None, "  "), ("num", "312 kB/s")],
    [],
    [("mut", " requests")],
    [("mut", " 09:41:07"), (None, " POST /v1/messages "), ("ok", "200")],
    [("mut", " 09:41:12"), (None, " GET  /api/turns   "), ("ok", "200")],
    [("mut", " 09:41:15"), (None, " POST /v1/messages "), ("err", "529")],
    [],
    [("mut", " 142 req · 0 dropped")],
], PH)

hero_det = sg_panel("bd", sgt("opus-48"), 34, [
    [("mut", " model    "), ("cy", "opus-48")],
    [("mut", " branch   "), (None, "main")],
    [("mut", " state    "), ("ok", "live")],
    [("mut", " turns    "), ("num", "142")],
    [("mut", " tokens   "), ("num", "1.8M")],
    [("mut", " uptime   "), ("num", "3h 12m")],
    [],
    [("mut", " last request")],
    [(None, "  in    "), ("num", "12.4 kB")],
    [(None, "  out   "), ("num", "3.1 kB")],
    [(None, "  ttfb  "), ("num", "412 ms")],
    [],
    [("mut", " injections")],
    [(None, "  recall     "), ("num", "2")],
    [(None, "  reminder   "), ("num", "1")],
    [(None, "  weasel     "), ("mut", "—")],
], PH)

hero_rows = []
_, hdr = render_line([
    (None, " "), ("emp", "Mike"), ("mut", " / "), (None, "sessions"),
    ("mut", " / "), ("emp", "opus-48"),
], 80)
hero_rows.append(f'<span class="appbar">{hdr}</span>')
for l, c, r in zip(hero_nav, hero_mid, hero_det):
    hero_rows.append(l + c + r)
_, fb = render_line([
    (None, " "), ("key", "j/k"), ("mut", " navigate · "), ("key", "tab"), ("mut", " panel · "),
    ("key", "enter"), ("mut", " open · "), ("key", "n s o"), ("mut", " jump · "),
    ("key", "?"), ("mut", " help · "), ("key", "q"), ("mut", " quit"),
], 80)
hero_rows.append(fb)
assert len(hero_rows) == 24
HERO = chr(10).join(hero_rows)

# ---------- color-tier demo (3 renditions of the same content) ----------

def tier(lines):
    rows = []
    for segs in lines:
        _, h = render_line(segs, 34)
        rows.append(h)
    return "\n".join(rows)

tier_mono = tier([
    [("emp", " api-9988"), (None, "  forwarding  "), (None, "142 req")],
    [(None, " steerer "), (None, "  injecting   "), (None, "  3 rules")],
    [("mut", " weasel  "), ("mut", "  idle        "), ("mut", "  —")],
])
tier_16 = tier([
    [(None, " api-9988  "), ("a16g", "forwarding"), (None, "  142 req")],
    [(None, " steerer   "), ("a16y", "injecting"), (None, "     3 rules")],
    [("a16k", " weasel    idle          —")],
])
tier_true = tier([
    [(None, " api-9988  "), ("ok", "forwarding"), (None, "  "), ("num", "142 req")],
    [(None, " steerer   "), ("warn", "injecting"), (None, "   "), ("num", "  3 rules")],
    [("mut", " weasel    idle          —")],
])

# ---------- static page ----------

CSS = FONT_RULES + """
/* Published SilkCircuit Neon tokens (lua/silkcircuit/variants.lua) — nothing invented. */
:root{
  --bg-base:#0a0812; --bg-surface:#12101a; --bg-overlay:#1a162a; --bg-selection:#44475a;
  --text-primary:#f8f8f2; --text-lavender:#e9d5ff; --text-muted:#6272a4; --text-emphasis:#ffffff;
  --accent-primary:#e135ff; --accent-muted:#9580ff; --accent-secondary:#80ffea; --cyan-bright:#5fffff;
  --accent-tertiary:#ff6ac1; --pink:#ff00ff; --hot-pink:#ff79c6;
  --success:#50fa7b; --warning:#f1fa8c; --error:#ff6363; --info:#82aaff; --number:#ffb86c;
  --line:rgba(98,114,164,.28); --line-strong:#44475a;
}
*{box-sizing:border-box}
body{margin:0;background:var(--bg-base);color:var(--text-primary);
  font-family:"JetBrains Mono","Fira Code","Cascadia Code",ui-monospace,SFMono-Regular,Menlo,Consolas,monospace;
  font-size:14px;line-height:1.55;}
.wrap{max-width:1120px;margin:0 auto;padding:0 24px 96px;}
a{color:var(--accent-secondary);text-decoration:none}
a:hover{text-decoration:underline}
a:focus-visible,nav a:focus-visible{outline:1px solid var(--accent-primary);outline-offset:2px}
table,pre,.num{font-variant-numeric:tabular-nums}
h1,h2,h3,h4{text-wrap:balance}
/* top bar */
.topbar{position:sticky;top:0;z-index:10;background:rgba(10,8,18,.92);backdrop-filter:blur(6px);
  border-bottom:1px solid var(--line);}
.topbar .in{max-width:1120px;margin:0 auto;padding:10px 24px;display:flex;gap:18px;align-items:baseline;flex-wrap:wrap;}
.brand{color:var(--text-emphasis);font-weight:700}
.brand .p{color:var(--accent-primary);text-shadow:0 0 10px rgba(225,53,255,.5)}
.topbar nav{display:flex;gap:14px;flex-wrap:wrap;font-size:12px}
.topbar nav a{color:var(--text-muted)}
.topbar nav a:hover{color:var(--accent-secondary);text-decoration:none}

header.masthead{padding:56px 0 8px}
h1{font-size:30px;margin:0 0 6px;color:var(--text-emphasis)}
h1 .p{color:var(--accent-primary);text-shadow:0 0 18px rgba(225,53,255,.65),0 0 42px rgba(225,53,255,.3)}
.sub{color:var(--text-muted);max-width:78ch}
.badges{margin:14px 0 0;display:flex;gap:8px;flex-wrap:wrap}
.badge{font-size:11px;padding:2px 9px;border:1px solid var(--line);color:var(--text-muted)}
.badge.on{border-color:var(--accent-primary);color:var(--accent-primary)}
.badge.cy{border-color:var(--accent-secondary);color:var(--accent-secondary)}

section{margin:44px 0}
h2{font-size:20px;color:var(--text-emphasis);margin:0 0 4px;padding-left:12px;
  border-left:3px solid var(--accent-primary);text-shadow:0 0 16px rgba(225,53,255,.35)}
h2 .rid{margin-left:10px}
.lede{color:var(--text-lavender);margin:0 0 18px;padding-left:15px;max-width:88ch}
.lede code{color:var(--accent-secondary)}
h3{font-size:14px;color:var(--accent-secondary);margin:22px 0 8px;text-shadow:0 0 10px rgba(128,255,234,.35)}

.rid{display:inline-block;font-size:10.5px;letter-spacing:.4px;color:var(--accent-secondary);
  border:1px solid rgba(128,255,234,.35);padding:0 6px;vertical-align:2px}
.card{background:var(--bg-surface);border:1px solid var(--line);padding:18px 20px}
.grid{display:grid;gap:14px}
.g2{grid-template-columns:repeat(auto-fit,minmax(320px,1fr))}
.g3{grid-template-columns:repeat(auto-fit,minmax(250px,1fr))}
.g4{grid-template-columns:repeat(auto-fit,minmax(210px,1fr))}

/* terminal frames */
.term{background:var(--bg-base);border:1px solid var(--line);padding:0;overflow:hidden}
.term .bar{display:flex;gap:6px;align-items:center;padding:8px 12px;background:var(--bg-surface);
  border-bottom:1px solid var(--line);font-size:11px;color:var(--text-muted)}
.term pre{margin:0;padding:14px 16px;overflow-x:auto;line-height:1.0;color:var(--text-primary);
  font-size:min(16px,calc((min(100vw,1120px) - 82px)/48.2));
  font-family:'TermMono','JetBrains Mono',ui-monospace,SFMono-Regular,Menlo,Consolas,monospace}
.caption{font-size:12px;color:var(--text-muted);margin-top:8px}

/* mockup span classes */
.bd{color:var(--text-muted)}.bdf{color:var(--accent-primary)}
.tt{color:var(--text-emphasis);font-weight:700}
.mut{color:var(--text-muted)}.emp{color:var(--text-emphasis);font-weight:700}
.ok{color:var(--success)}.warn{color:var(--warning)}.err{color:var(--error)}
.num{color:var(--number)}.cy{color:var(--accent-secondary)}.pk{color:var(--hot-pink)}
.sel{color:var(--accent-primary);font-weight:700;text-shadow:0 0 8px rgba(225,53,255,.55)}
.key{color:var(--accent-secondary);font-weight:700;text-shadow:0 0 8px rgba(128,255,234,.5)}
.hdrbar{background:var(--bg-surface)}
.appbar{background:var(--bg-surface)}
.act{background:var(--bg-selection);color:var(--text-emphasis)}
.sc{color:var(--accent-secondary);font-weight:700;text-decoration:underline}
.a16g{color:#00d700}.a16y{color:#d7d700}.a16k{color:#808080}

/* palette */
.swatches{display:grid;gap:12px;grid-template-columns:repeat(auto-fill,minmax(215px,1fr))}
.sw{background:var(--bg-surface);border:1px solid var(--line);overflow:hidden}
.sw .chip{height:56px;display:flex;align-items:flex-end;padding:8px 10px;font-size:11px;font-weight:700}
.sw .meta{padding:10px 12px;font-size:12px}
.sw .slot{color:var(--text-emphasis);font-weight:700}
.sw .hex{color:var(--text-muted)}
.sw .role{color:var(--text-muted);font-size:11px;margin-top:4px;line-height:1.4}
.sw .fb{float:right;color:var(--accent-secondary);font-size:10.5px;border:1px solid rgba(128,255,234,.3);
  padding:0 5px}

/* variants */
.variants{display:grid;gap:12px;grid-template-columns:repeat(auto-fit,minmax(190px,1fr))}
.variant{border:1px solid var(--line);overflow:hidden;background:var(--bg-surface)}
.variant .vhead{padding:8px 12px;font-size:12px;color:var(--text-emphasis);border-bottom:1px solid var(--line);
  display:flex;justify-content:space-between;align-items:baseline}
.variant .vhead small{color:var(--text-muted);font-size:10px}
.variant pre{margin:0;padding:12px;font-size:12px;line-height:1.5;background:var(--bg-base);
  font-family:'TermMono','JetBrains Mono',ui-monospace,SFMono-Regular,Menlo,Consolas,monospace}
.vdefault{color:var(--accent-primary);font-size:10px;border:1px solid rgba(225,53,255,.4);padding:0 5px}

/* principles */
.pr{background:var(--bg-surface);border:1px solid var(--line);padding:16px 18px}
.pr h4{margin:0 0 6px;font-size:13.5px;color:var(--text-emphasis)}
.pr p{margin:0;font-size:12px;color:var(--text-muted);line-height:1.55}
.pr .q{color:var(--hot-pink)}

/* keyboard */
kbd{display:inline-block;background:var(--bg-overlay);border:1px solid var(--line-strong);border-bottom-width:3px;
  padding:1px 7px;font-size:12px;color:var(--accent-secondary);font-family:inherit}
table{width:100%;border-collapse:collapse;font-size:12.5px}
th{text-align:left;color:var(--text-muted);font-weight:400;font-size:11px;letter-spacing:.5px;
  text-transform:uppercase;border-bottom:1px solid var(--line);padding:6px 10px}
td{border-bottom:1px solid rgba(44,39,64,.6);padding:8px 10px;vertical-align:top}
td.c-l{color:var(--accent-primary);font-weight:700;white-space:nowrap}

/* pipeline */
.pipe{display:flex;gap:0;align-items:stretch;flex-wrap:wrap}
.pipe .stage{flex:1;min-width:170px;background:var(--bg-surface);border:1px solid var(--line);
  padding:12px 14px;margin:0 18px 12px 0;position:relative}
.pipe .stage:after{content:"\\2192";position:absolute;right:-16px;top:42%;color:var(--accent-primary);font-weight:700}
.pipe .stage:last-child:after{content:""}
.pipe h4{margin:0 0 4px;font-size:12.5px;color:var(--accent-secondary)}
.pipe p{margin:0;font-size:11px;color:var(--text-muted)}
.pipe code{color:var(--text-primary);font-size:11px}
.gradbar{height:10px;margin-top:8px;
  background:linear-gradient(90deg,#e135ff,#ff00ff,#ff6ac1,#80ffea);box-shadow:0 0 14px rgba(225,53,255,.35)}

/* state chips */
.states{display:flex;gap:10px;flex-wrap:wrap}
.state{display:flex;gap:8px;align-items:center;background:var(--bg-surface);border:1px solid var(--line);
  padding:7px 12px;font-size:12px}
.state .g{font-size:13px}

/* glyphs */
.glyphs{display:grid;gap:10px;grid-template-columns:repeat(auto-fill,minmax(150px,1fr))}
.gl{background:var(--bg-surface);border:1px solid var(--line);padding:10px 12px;
  display:flex;gap:12px;align-items:center}
.gl .g{font-size:20px;width:28px;text-align:center}
.gl .m{font-size:11px;color:var(--text-muted);line-height:1.4}
.gl .m b{color:var(--text-primary);display:block;font-size:12px}

/* wrong-right */
.wr{display:grid;gap:14px;grid-template-columns:1fr 1fr}
.wr .side{border:1px solid var(--line);overflow:hidden;background:var(--bg-surface)}
.wr .side .tag{font-size:10.5px;letter-spacing:1px;padding:5px 12px;font-weight:700}
.wr .wrong .tag{color:var(--error);background:rgba(255,99,99,.08);border-bottom:1px solid rgba(255,99,99,.25)}
.wr .right .tag{color:var(--success);background:rgba(80,250,123,.07);border-bottom:1px solid rgba(80,250,123,.25)}
.wr pre{margin:0;padding:12px 14px;font-size:12px;line-height:1.45;overflow-x:auto}
.wr .why{font-size:11px;color:var(--text-muted);padding:0 14px 12px}

/* checklist */
.check li{list-style:none;margin:7px 0;color:var(--text-primary);font-size:13px}
.check li:before{content:"—";color:var(--accent-primary);margin-right:10px}
.check{padding-left:6px;margin:0}
.check .rid{margin-left:8px}

/* loop */
.loop{display:flex;gap:8px;align-items:center;flex-wrap:wrap;font-size:13px}
.loop .st{background:var(--bg-surface);border:1px solid var(--line);padding:5px 16px;
  color:var(--text-emphasis)}
.loop .st.hot{border-color:var(--accent-primary);color:var(--accent-primary)}
.loop .ar{color:var(--accent-secondary)}

footer{margin-top:70px;border-top:1px solid var(--line);padding-top:18px;font-size:11.5px;color:var(--text-muted)}
.mono-demo{filter:grayscale(1)}
"""

def sw(slot, hexv, fb, role, dark_label=False):
    lbl = "#1a1a1a" if dark_label else "rgba(255,255,255,.85)"
    return f'''<div class="sw">
  <div class="chip" style="background:{hexv};color:{lbl}">{hexv}</div>
  <div class="meta"><span class="fb">256: {fb}</span><span class="slot">{slot}</span>
  <div class="role">{role}</div></div></div>'''

SWATCHES = "".join([
    sw("bg.base", "#0a0812", "232", "bg_dark — the app canvas"),
    sw("bg.surface", "#12101a", "233", "bg — panels, cards"),
    sw("bg.overlay", "#1a162a", "235", "bg_highlight — modals, overlays"),
    sw("bg.selection", "#44475a", "239", "selection / bg_visual — selected rows, visual ranges"),
    sw("text.primary", "#f8f8f2", "255", "fg — main body text", dark_label=True),
    sw("text.secondary", "#e9d5ff", "189", "fg_dark — lavender; ledes, secondary emphasis", dark_label=True),
    sw("text.muted", "#6272a4", "61", "fg_gutter — metadata, labels, timestamps"),
    sw("text.emphasis", "#ffffff", "231", "fg_light + bold — headers, focused items", dark_label=True),
    sw("accent.primary", "#e135ff", "171", "purple — brand, focus states, keywords"),
    sw("accent.muted", "#9580ff", "105", "purple_dark — non-focused accent structure"),
    sw("accent.secondary", "#80ffea", "122", "cyan — interaction, paths, links · bright #5fffff / 87", dark_label=True),
    sw("accent.tertiary", "#ff6ac1", "205", "coral — accents, hashes · #ff79c6 strings · #ff00ff pink for glow moments"),
    sw("status.success", "#50fa7b", "84", "green — confirmations, additions, online", dark_label=True),
    sw("status.warning", "#f1fa8c", "228", "yellow — warnings, attention · diagnostics #ecc48d", dark_label=True),
    sw("status.error", "#ff6363", "203", "red_error — danger, offline · diagnostics #ff5874"),
    sw("status.info", "#82aaff", "111", "info / blue — informational", dark_label=True),
    sw("data.number", "#ffb86c", "215", "Warm Orange (README) — numbers, counts, constants", dark_label=True),
    '''<div class="sw"><div class="chip" style="background:linear-gradient(90deg,#e135ff,#ff00ff,#80ffea);color:#1a1a1a">glow_*</div>
  <div class="meta"><span class="fb">171·201·122</span><span class="slot">glow.purple/pink/cyan</span>
  <div class="role">glow_purple #e135ff · glow_pink #ff00ff · glow_cyan #80ffea — static glow emphasis</div></div></div>''',
])

VAR_DATA = [
    ("Neon", True, False, dict(bg="#12101a", fg="#f8f8f2", pu="#e135ff", cy="#80ffea", gr="#50fa7b", mu="#6272a4", ye="#f1fa8c"), "bg #12101a"),
    ("Vibrant", False, False, dict(bg="#0f0c1a", fg="#f8f8f2", pu="#ff00ff", cy="#00ffcc", gr="#00ff66", mu="#6272a4", ye="#ffcc00"), "bg #0f0c1a"),
    ("Soft", False, False, dict(bg="#1a1626", fg="#f8f8f2", pu="#e892ff", cy="#99ffee", gr="#66ff99", mu="#6272a4", ye="#ffe699"), "bg #1a1626"),
    ("Glow", False, True, dict(bg="#0a0816", fg="#ffffff", pu="#ff00ff", cy="#00ffff", gr="#00ff00", mu="#666666", ye="#ffff00"), "bg #0a0816 · OLED"),
    ("Dawn", False, False, dict(bg="#faf8ff", fg="#2b2540", pu="#7e2bd5", cy="#007f8e", gr="#2d8659", mu="#8e84a8", ye="#a88600"), "bg #faf8ff · light"),
]

def variant(name, default, glow, c, note):
    d = ' <span class="vdefault">default</span>' if default else ""
    sh = "text-shadow:0 0 7px currentColor;" if glow else ""
    def sp(color, txt, bold=False):
        w = "font-weight:700;" if bold else ""
        return f'<span style="color:{color};{sh}{w}">{txt}</span>'
    sample = (sp(c["fg"], "steerer") + "  " + sp(c["gr"], "active") + "  "
              + sp(c["ye"], "3") + " " + sp(c["mu"], "rules") + " " + sp(c["mu"], "·") + " "
              + sp(c["pu"], "settings", True))
    return f'''<div class="variant"><div class="vhead"><span>{name}{d}</span><small>{note}</small></div>
<pre style="background:{c["bg"]}">{sample}</pre></div>'''

VARIANTS = "".join(variant(*v) for v in VAR_DATA)

def pr(rid, title, body):
    return f'<div class="pr"><h4><span class="rid">{rid}</span> {title}</h4><p>{body}</p></div>'

P1 = pr("TUI-P1", "Spatial consistency is the foundation",
       "Panels don't move. Tabs stay in order. The user builds a mental map in the first minute and navigates by location memory after that. <span class='q'>Every shuffle resets their spatial model to zero.</span>")
P2 = pr("TUI-P2", "Keyboard-first, mouse-optional",
       "Every feature reachable without a mouse; mouse support not an afterthought. Power users are keyboard people, beginners will click. Support both; optimize for keys.")
P3 = pr("TUI-P3", "Progressive disclosure, three tiers",
       "Help bar with the 3–5 most important keys (always visible) → <kbd>?</kbd> overlay with the full reference (on demand) → complete docs. <span class='q'>Nobody reads a manual to get started.</span>")
P4 = pr("TUI-P4", "Semantic color",
       "Green = success, red = danger, yellow = caution. Color <em>reinforces</em> hierarchy already built by layout, typography, symbols. <span class='q'>If stripping all color makes the app unusable, the design is broken.</span>")
P5 = pr("TUI-P5", "Async everything",
       "Never freeze the UI. File I/O, network, DB — background tasks with progress indicators. <kbd>Esc</kbd> always returns a responsive interface. <span class='q'>A TUI that hangs is a TUI that gets killed.</span>")
P6 = pr("TUI-P6", "Contextual intelligence",
       "Keybindings change as focus moves. The help bar shows only the commands available right now, and <kbd>?</kbd> matches it. The UI earns trust by being accurate about what's possible.")
B1 = pr("TUI-B1", "AI-friendly UI",
   "AI agents build, drive, and verify this TUI, and they are blind by default. Every state change flows through the event system (TUI-A2) so an agent can reason about it; every feature is reachable by keys (TUI-P2); the CLI face exposes the same data scriptably (TUI-A3); the rendered screen is verifiable in a real terminal (TUI-V1). <span class='q'>design → build → run → see → fix → repeat.</span>")
P7 = pr("TUI-P7", "Design in layers",
       "Monochrome: usable? → 16 ANSI: hierarchy readable? → truecolor: beautiful? Each tier independently usable. Works over a monochrome SSH session, stunning in Ghostty. <span class='q'>A discipline, not a tradeoff.</span>")


GLYPHS = "".join([
    f'<div class="gl"><div class="g" style="color:{c}">{g}</div><div class="m"><b>{n}</b>{m}</div></div>'
    for g, c, n, m in [
        ("·", "var(--text-muted)", "separator", "inline metadata divider"),
        ("—", "var(--text-muted)", "null", "no value, never blank"),
        ("▉", "var(--success)", "bar cell", "fractional blocks ▏▎▍▌▋▊▉█"),
        ("▅", "var(--accent-secondary)", "sparkline cell", "block sparklines ▁▂▃▄▅▆▇█"),
    ]
])

WR_TABLE_ROWS = "".join([
    f"<tr><td style='color:var(--error)'>{w}</td><td style='color:var(--success)'>{r}</td><td class='c-l'>{rid}</td></tr>"
    for w, r, rid in [
        ("Rearranging panels when data changes", "Fixed positions; empty panel shows its empty state", "TUI-P1"),
        ("Blocking the render loop on a DB query", "Background task + progress, <kbd>Esc</kbd> stays responsive", "TUI-P5"),
        ("Style::default().fg(Color::Rgb(…)) at a call site", "Style resolved from a semantic slot", "TUI-C4"),
        ("Shipping only a truecolor look", "16-color fallback per token, NO_COLOR honored", "TUI-C2/C3"),
        ("Polling the inspect API on a tick", "Push via watch/broadcast channel", "TUI-A1"),
        ("&ldquo;Tests pass&rdquo; as TUI completion claim", "Rendered-screen evidence from a real terminal", "TUI-V1"),
        ("Pill badges, rounded cards, window dots", "Square corners, box borders, bracketed tokens", "TUI-G5"),
    ]
])

CONTENT = f"""<title>TUI Design</title>
<style>{CSS}</style>

<div class="topbar"><div class="in">
  <span class="brand">TUI <span class="p">Design</span></span>
  <nav>
    <a href="#layout">layout</a><a href="#panels">panels</a><a href="#palette">colors</a>
    <a href="#animation">animation</a><a href="#navhelp">navigation &amp; help</a>
    <a href="#best">best practices</a><a href="#checklist">checklist</a><a href="#wr">wrong/right</a>
  </nav>
</div></div>

<div class="wrap">
<header class="masthead">
  <h1>TUI Design</h1>
  <p class="sub">The designed rendition of <a href="STYLE_GUIDE.md">STYLE_GUIDE.md</a> — same numbered,
  binding rules (TUI-L/P/K/C/G/A/V), rendered instead of described. Codified from
  <em>&ldquo;The Terminal Renaissance&rdquo;</em> (hyperbliss.tech, 2026-04-04) and the
  SilkCircuit design language. TUI Design is portable and application-neutral; application
  screen specs are layered on top. The terminal system does not derive from SPA web tokens.</p>
  <div class="badges">
    <span class="badge on">status: active</span>
    <span class="badge cy">palette: SilkCircuit Neon</span>
    <span class="badge">floor: 80×24</span>
    <span class="badge">usable at 16 colors · beautiful at truecolor</span>
  </div>
</header>

<section id="hero">
  <h2>The design language at a glance</h2>
  <p class="lede">The universal layout (TUI-L1, <a href="LAYOUT.md">LAYOUT.md</a>) shown
  with a labelled mike application example at 80×24:
  app bar with the navigation path, content-sized nav, panels titled by what they show
  (<code>sessions</code>, <code>opus-48</code>) with shortcut letters from the names, focused
  border in <code>accent.primary</code>, full-width row selection with no markers, state as
  text in semantic color, block sparklines for data (TUI-G2), help bar with navigation
  commands only.</p>
  <div class="term">
    <div class="bar">Mike / sessions / opus-48 — 80×24 — SilkCircuit Neon</div>
    <pre>{HERO}</pre>
  </div>
  <div class="caption">Every element on this screen resolves to a semantic slot — nothing is a raw hex value (TUI-C4).</div>
</section>

<section id="layout">
  <h2>Layout — one design <span class="rid">TUI-L1</span></h2>
  <p class="lede"><strong>Every application uses one universal layout</strong> (operator decision
  2026-07-23), defined in <a href="LAYOUT.md">docs/tui/LAYOUT.md</a> and rendered at four sizes
  in the layout artifact: app bar with the navigation path, content-sized nav, middle and
  detail/preview splitting the rest equally with contextual titles, help bar with navigation
  commands only. Narrow real estate stacks right items below left items; when stacking is
  impossible the panel's view becomes a modal. There is no per-screen layout choice. The
  floor is 80×24 (TUI-L2); multi-mode apps keep one frame (TUI-L3).</p>
  <div class="grid g2">{P1}</div>
</section>

<section id="panels">
  <h2>Panels <span class="rid">TUI-PN</span></h2>
  <p class="lede">What lives inside a panel. <strong>This is the next design phase; nothing
  below is defined yet.</strong> Panel-internal table and row structure, cell padding, gaps,
  banding, spacing, and alignment will be developed in the future generic
  <code>tui-kit</code> repository and made enforceable there. Do not infer those rules from
  the illustrative mike mockups.</p>
  <div class="grid g3">
    <div class="pr"><h4>Tables</h4><p>—</p></div>
    <div class="pr"><h4>Data</h4><p>—</p></div>
    <div class="pr"><h4>Graphics</h4><p>—</p></div>
    <div class="pr"><h4>Toggles</h4><p>—</p></div>
    <div class="pr"><h4>Views</h4><p>—</p></div>
    <div class="pr"><h4>Logs</h4><p>—</p></div>
    <div class="pr"><h4>Modals</h4><p>—</p></div>
    <div class="pr"><h4>Inputs</h4><p>—</p></div>
    <div class="pr"><h4>Form options</h4><p>—</p></div>
  </div>
</section>

<section id="palette">
  <h2>Palette <span class="rid">TUI-C4a</span></h2>
  <p class="lede">Colors are defined by function, not appearance (TUI-C4): widget code references
  slots, never values. Every value here is a <strong>published SilkCircuit Neon token</strong>
  (<code>lua/silkcircuit/variants.lua</code>) — the role text names the exact token. The Neon
  backgrounds are purple-tinted (<code>#0a0812 → #12101a → #1a162a</code>), primary text is warm
  white <code>#f8f8f2</code>, secondary text is lavender, muted text is blue-violet — neutral
  greys here would reproduce a generic dark theme and are Wrong. Each slot carries its nearest
  xterm-256 fallback (TUI-C2).</p>
  <div class="swatches">{SWATCHES}</div>
</section>

<section id="variants">
  <h2>Intensity variants <span class="rid">TUI-C4b</span></h2>
  <p class="lede">Same semantic slots, five energies — each a <strong>full published palette</strong>
  in <code>variants.lua</code>, rendered below with its real values (no approximations). Wanting
  the app dimmer or brighter means selecting a <em>variant</em> through the theme pipeline
  (TUI-C5) — never hand-adjusting slot values on one screen. &ldquo;The design language is the
  mapping from meaning to color, not the colors themselves.&rdquo;</p>
  <div class="variants">{VARIANTS}</div>
</section>

<section id="color">
  <h2>Color tiers &amp; detection <span class="rid">TUI-C1–C3</span></h2>
  <p class="lede">The golden rule: <strong>usable at 16 colors, beautiful at truecolor.</strong>
  The same three lines rendered per tier — hierarchy must survive every one of them (TUI-P7).</p>
  <div class="grid g3">
    <div><div class="term"><div class="bar">tier 0 — monochrome (bold/dim only)</div><pre class="mono-demo">{tier_mono}</pre></div></div>
    <div><div class="term"><div class="bar">tier 1 — 16 ANSI (theme-coherent)</div><pre>{tier_16}</pre></div></div>
    <div><div class="term"><div class="bar">tier 2 — truecolor (SilkCircuit)</div><pre>{tier_true}</pre></div></div>
  </div>
  <div class="grid g3" style="margin-top:14px">
    <div class="card"><h3 style="margin-top:0">$COLORTERM</h3><p class="lede" style="padding:0;margin:0"><code>truecolor</code> / <code>24bit</code> → tier 2 enabled</p></div>
    <div class="card"><h3 style="margin-top:0">$TERM</h3><p class="lede" style="padding:0;margin:0"><code>*-256color</code> → tier 1+ with fixed fallbacks</p></div>
    <div class="card"><h3 style="margin-top:0">$NO_COLOR</h3><p class="lede" style="padding:0;margin:0">set → strip <em>all</em> color, unconditionally. Professional courtesy, not just accessibility.</p></div>
  </div>
  <div class="grid g2" style="margin-top:14px">{P4}{P7}</div>
</section>

<section id="theming">
  <h2>Theming is infrastructure <span class="rid">TUI-C5 · TUI-C6</span></h2>
  <p class="lede">&ldquo;Hex codes in source files is a phase, not a strategy.&rdquo; Each layer
  references only the one below; themes are validated by contract tests — a theme that
  half-resolves is a build failure, not a runtime surprise.</p>
  <div class="pipe">
    <div class="stage"><h4>palette</h4><p>raw colors</p><code>electric_purple = #e135ff</code></div>
    <div class="stage"><h4>tokens</h4><p>semantic names → palette</p><code>accent.primary → electric_purple</code></div>
    <div class="stage"><h4>styles</h4><p>fg + bg + modifiers</p><code>focused_border = accent.primary + bold</code></div>
    <div class="stage"><h4>gradients</h4><p>multi-stop interpolation</p><div class="gradbar"></div></div>
  </div>
  <h3>State machines get a visual language (TUI-C6)</h3>
  <p class="lede">Each state maps to a color <em>and</em> a glyph <em>and</em> a style —
  consistently, everywhere it appears. The screen looks different when things are on fire.</p>
  <div class="states">
    <div class="state"><span style="color:var(--error);font-weight:700">hot</span>&nbsp;— needs attention now</div>
    <div class="state"><span style="color:var(--warning);font-weight:700">waiting</span>&nbsp;— blocked on something</div>
    <div class="state"><span style="color:var(--success);font-weight:700">ready</span>&nbsp;— good to go</div>
    <div class="state"><span style="color:var(--text-muted);font-weight:700">dormant</span>&nbsp;— stale</div>
    <div class="state"><span style="color:var(--accent-secondary);font-weight:700">pending</span>&nbsp;— awaiting action</div>
  </div>
</section>

<section id="glyphs">
  <h2>Glyph vocabulary <span class="rid">TUI-G1–G5</span></h2>
  <p class="lede">A documented vocabulary: every glyph a screen uses is listed with exactly one
  meaning, and one meaning never has two glyphs — consistency comes from the table, and additions
  extend it deliberately. Glyphs stay within the unambiguous-width set (double-width and ZWJ
  sequences break cell alignment — an engineering constraint, not taste). Sub-cell precision (fractional blocks, braille + halfblock charts)
  is for data display only, never decoration (TUI-G2). Border weight signals hierarchy — never
  uniform default borders everywhere (TUI-G3). Inspection content is never truncated: scroll or
  paginate (TUI-G4). And the geometry is the cell grid's: square corners everywhere the design language appears — no pills, no rounded chrome, no desktop window dressing, on this page included (TUI-G5).</p>
  <div class="glyphs">{GLYPHS}</div>
  <h3>Sub-cell data display (TUI-G2)</h3>
  <div class="term"><pre> <span class="mut">cpu </span><span class="ok">▉▉▉▉▉▉▍</span><span class="mut">        </span><span class="num">68%</span>   <span class="mut">rx </span><span class="cy">▁▂▄▆██▆▄▂▁▂▄▆█▆▄▂▁</span>  <span class="num">1.2 MB/s</span>   <span class="mut">borders: </span>─ light  <span class="emp">━ heavy</span>  ═ double</pre></div>
</section>

<section id="animation">
  <h2>Animation</h2>
  <p class="lede"><strong>Reserved.</strong> No rules are defined.</p>
</section>

<section id="navhelp">
  <h2>Navigation and help <span class="rid">TUI-K · TUI-P2/P3/P6</span></h2>
  <p class="lede">Vim keys are the terminal lingua franca — six keystrokes handle 80% of
  navigation. Four layers, each invisible until the user reaches for it: progressive disclosure
  applied to the keyboard.</p>
  <div class="card">
  <table>
    <tr><th>layer</th><th>keys</th><th>discoverability</th></tr>
    <tr><td class="c-l">L0 universal</td><td><kbd>↑</kbd><kbd>↓</kbd><kbd>←</kbd><kbd>→</kbd> <kbd>Enter</kbd> <kbd>Esc</kbd> <kbd>q</kbd></td><td>help bar — anyone can use this</td></tr>
    <tr><td class="c-l">L1 vim navigation</td><td><kbd>j</kbd><kbd>k</kbd><kbd>h</kbd><kbd>l</kbd> <kbd>/</kbd> <kbd>?</kbd> <kbd>:</kbd> <kbd>g</kbd><kbd>G</kbd></td><td>help bar — terminal natives expect this</td></tr>
    <tr><td class="c-l">L2 actions</td><td>single mnemonics: <kbd>d</kbd> delete · <kbd>s</kbd> stage · <kbd>r</kbd> refresh</td><td><kbd>?</kbd> help overlay; same action = same key on every screen</td></tr>
    <tr><td class="c-l">L3 power</td><td>composed commands · macros · configuration</td><td>documentation only — the ceiling for experts</td></tr>
  </table>
  <h3>The help bar (TUI-P3: 3–5 keys, never a wall)</h3>
  <div class="term"><pre> <span class="key">j/k</span> <span class="mut">navigate  ·  </span><span class="key">Enter</span> <span class="mut">open  ·  </span><span class="key">/</span> <span class="mut">search  ·  </span><span class="key">?</span> <span class="mut">help  ·  </span><span class="key">q</span> <span class="mut">quit</span></pre></div>
  </div>
  <div class="grid g3" style="margin-top:14px">{P2}{P3}{P6}</div>
</section>

<section id="best">
  <h2>Best practices for good UI <span class="rid">TUI-P5 · TUI-A · TUI-B · TUI-V</span></h2>
  <p class="lede">The practices that keep the TUI honest: never block, push don't poll,
  show processing while it happens, and keep the UI drivable and verifiable by AI agents.</p>
  <div class="grid g2">{P5}{B1}</div>
  <h3>Architecture with design consequences (TUI-A)</h3>
  <div class="grid g3">
    <div class="pr"><h4><span class="rid">TUI-A1</span> Never poll; push</h4>
      <p>The UI is told when something changes (watch/broadcast channels) — it never asks
      &ldquo;has anything changed?&rdquo; on a timer. The responsiveness difference is visceral.</p></div>
    <div class="pr"><h4><span class="rid">TUI-A2</span> events → state → render</h4>
      <p>Pure reducers: <code>(state, event) → (new state, side effects)</code>, no I/O inside.
      Application events and agent output flow through the same event system as keystrokes. Testable,
      debuggable, auditable.</p></div>
    <div class="pr"><h4><span class="rid">TUI-A3</span> Know when not to be a TUI</h4>
      <p>One core, two faces: scriptable CLI (plain/JSON, pipe-composable, zero decoration) and
      the interactive TUI. When stdout is a pipe: no color and no boxes.</p></div>
  </div>
  <h3>Verification (TUI-V)</h3>
  <p class="lede">An agent building a TUI is blind by default — stdout text can't verify that
  borders line up or that <kbd>j</kbd>/<kbd>k</kbd> navigates. Every TUI change is verified by
  running the app in a real terminal and reading the rendered screen (pty harness, snapshot
  tests). A passing unit test is not a substitute for seeing the screen. Verify per layer:
  snapshot monochrome, 16-color, truecolor × 80×24 and the wider reference size.</p>
  <div class="loop">
    <span class="st">design</span><span class="ar">→</span>
    <span class="st">build</span><span class="ar">→</span>
    <span class="st hot">run</span><span class="ar">→</span>
    <span class="st hot">see</span><span class="ar">→</span>
    <span class="st">fix</span><span class="ar">→</span>
    <span class="st">repeat</span>
  </div>
</section>

<section id="checklist">
  <h2>Screen-spec checklist</h2>
  <p class="lede">Every screen spec under <code>artifacts/</code> — and every review of one — checks:</p>
  <div class="card"><ul class="check">
    <li>Uses the universal layout — app bar path, nav/middle/detail-preview with contextual
    titles, navigation-only help bar in the final row <span class="rid">TUI-L1 · LAYOUT.md</span></li>
    <li>Exact 80×24 mockup + wider reference; narrow-width behavior stated <span class="rid">TUI-L2</span></li>
    <li>Keymap listed by layer L0–L3; help bar shows 3–5 keys; <kbd>?</kbd> overlay defined <span class="rid">TUI-K2 · TUI-P3</span></li>
    <li>All colors as semantic slots with 256-color fallback; no raw values <span class="rid">TUI-C2 · TUI-C4</span></li>
    <li>Usable-when-stripped: hierarchy holds in monochrome <span class="rid">TUI-P4 · TUI-P7</span></li>
    <li>Every glyph used is in the documented vocabulary <span class="rid">TUI-G1</span></li>
    <li>Loading/empty/error/populated states specified; nothing blocks the event loop <span class="rid">TUI-P5</span></li>
    <li>Focus-dependent keybindings enumerated; help bar matches the current navigation
    context and carries no status <span class="rid">TUI-P6</span></li>
    <li>No truncation of inspection content — scroll/paginate <span class="rid">TUI-G4</span></li>
  </ul></div>
</section>

<section id="wr">
  <h2>Wrong / Right — the drift-prone patterns</h2>
  <div class="wr">
    <div class="side wrong"><div class="tag">WRONG — help bar as a wall</div>
<pre><span class="mut"> j:down k:up h:left l:right gg:top G:bottom /:search n:next N:prev ?:help d:del
 s:stage u:unstage r:refresh c:commit p:push P:pull b:branch t:tag q:quit …</span></pre>
      <div class="why">Every binding on screen at once — the wall of shortcuts TUI-P3 exists to prevent.</div></div>
    <div class="side right"><div class="tag">RIGHT — 3–5 keys, rest behind ?</div>
<pre> <span class="key">j/k</span> <span class="mut">navigate ·</span> <span class="key">Enter</span> <span class="mut">open ·</span> <span class="key">/</span> <span class="mut">search ·</span> <span class="key">?</span> <span class="mut">help ·</span> <span class="key">q</span> <span class="mut">quit</span></pre>
      <div class="why">Beginners see the floor; the <kbd>?</kbd> overlay holds the ceiling.</div></div>
    <div class="side wrong"><div class="tag">WRONG — color as the only signal</div>
<pre> POST /v1/messages   <span class="err">529</span>
 POST /v1/messages   <span class="ok">200</span></pre>
      <div class="why">Strip the color and the failure disappears — broken under TUI-P4 / NO_COLOR.</div></div>
    <div class="side right"><div class="tag">RIGHT — text + color</div>
<pre> POST /v1/messages   <span class="err">529 overloaded</span>
 POST /v1/messages   <span class="ok">200 ok</span></pre>
      <div class="why">The word carries the state; color reinforces it. Hierarchy holds in monochrome.</div></div>
  </div>
  <div class="card" style="margin-top:14px">
  <table><tr><th>wrong</th><th>right</th><th>rule</th></tr>{WR_TABLE_ROWS}</table>
  </div>
</section>

<footer>
  Sources: Stefanie Jane, <em>The Terminal Renaissance</em> (hyperbliss.tech, 2026-04-04) ·
  SilkCircuit design language (github.com/hyperb1iss/silkcircuit) · operator decision 2026-07-23.
  Normative text: <a href="STYLE_GUIDE.md">docs/tui/STYLE_GUIDE.md</a> — rule IDs match. This page is
  self-contained: no external fonts, scripts, or images.
</footer>
</div>
"""

# Full-document assembly for the repo copy: title+style belong in <head>.
head_end = CONTENT.index("</style>") + len("</style>")
FULL = (
    '<!DOCTYPE html>\n<html lang="en"><head>\n<meta charset="utf-8">\n'
    '<meta name="viewport" content="width=device-width,initial-scale=1">\n'
    + CONTENT[:head_end]
    + "\n</head><body>\n"
    + CONTENT[head_end:]
    + "\n</body></html>\n"
)

with open(OUT, "w") as f:
    f.write(FULL)
print(f"wrote {OUT} ({len(FULL)} bytes)")
if OUT_ARTIFACT:
    with open(OUT_ARTIFACT, "w") as f:
        f.write(CONTENT)
    print(f"wrote artifact fragment {OUT_ARTIFACT} ({len(CONTENT)} bytes)")
