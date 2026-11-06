#!/usr/bin/env python3
"""Generate docs/tui/LAYOUT.html — the layout rendered at three sizes.
Every mockup line carries a width assertion so alignment is verified at build time."""
import html as H
import sys
from pathlib import Path

OUT = str(Path(__file__).resolve().parent / "LAYOUT.html")
OUT_ARTIFACT = sys.argv[1] if len(sys.argv) > 1 else None

import base64 as _b64
_FONT = _b64.b64encode((Path(__file__).resolve().parent / "assets/term-mono.woff2").read_bytes()).decode()
FONT_RULES = (
    "/* Display rules for terminal mockups: embedded mono font (subset DejaVu Sans Mono),\n"
    "   line-height 1.0 so borders join, text-size-adjust off, fit-to-viewport font-size\n"
    "   ((100vw - chrome) / (cols x 0.602em)) so a screen never clips or scrolls sideways. */\n"
    "@font-face{font-family:'TermMono';src:url(data:font/woff2;base64," + _FONT + ") format('woff2');font-display:block}\n"
    "html{-webkit-text-size-adjust:100%;text-size-adjust:100%}\n"
)

def esc(t):
    return H.escape(t, quote=False)

def render(segs, width=None):
    vis = "".join(t for _, t in segs)
    out = "".join((f'<span class="{c}">{esc(t)}</span>' if c else esc(t)) for c, t in segs)
    if width is not None:
        pad = width - len(vis)
        assert pad >= 0, f"overflow by {-pad}: {vis!r}"
        out += " " * pad
    return out

def toprow(bcls, width, title_segs):
    tlen = sum(len(t) for _, t in title_segs)
    tail = width - 3 - tlen - 2
    assert tail >= 0, f"title too long for {width}"
    return (f'<span class="{bcls}">{esc("┌─ ")}</span>' + render(title_segs)
            + f'<span class="{bcls}">{esc(" " + "─" * tail + "┐")}</span>')

def botrow(bcls, width):
    return f'<span class="{bcls}">{esc("└" + "─" * (width - 2) + "┘")}</span>'

def panel(bcls, title_segs, width, rows, height):
    """rows: list of segs-lists, or ('act', segs) for a full-width highlighted row."""
    inner = width - 2
    out = [toprow(bcls, width, title_segs)]
    body = list(rows)
    while len(body) < height - 2:
        body.append([])
    assert len(body) == height - 2, f"panel too tall: {len(body)} > {height-2}"
    for r in body:
        cls = None
        if isinstance(r, tuple):
            cls, r = r
        h = render(r, inner)
        if cls:
            h = f'<span class="{cls}">{h}</span>'
        out.append(f'<span class="{bcls}">│</span>' + h + f'<span class="{bcls}">│</span>')
    out.append(botrow(bcls, width))
    assert len(out) == height
    return out

def t(name):
    """Panel title segs: first letter is the shortcut, highlighted."""
    return [("sc", name[0]), ("tt", name[1:])]

# The universal layout rendered at each size with a labelled mike application example.
NAV_ROWS = [
    [(None, " traffic")],
    [(None, " sessions")],
    ("act", [(None, " settings")]),
    [(None, " steering")],
    [(None, " memory")],
]
MID_ROWS = [
    [(None, " capture      on")],
    [(None, " steerer      on")],
    [(None, " weasel       off")],
    ("act", [(None, " tokens       4 keys")]),
    [(None, " ttl          30d")],
    [(None, " compose      on")],
]
DET_ROWS = [
    [("mut", " key      "), (None, "anthropic")],
    [("mut", " created  "), (None, "2026-05-01")],
    [("mut", " scope    "), (None, "proxy")],
    [("mut", " last use "), (None, "09:41")],
]

def screen(W, Ht, panels, help_segs):
    """panels: list of (bcls, title, width, rows). Widths must sum to W."""
    assert sum(p[2] for p in panels) == W, f"panel widths != {W}"
    ph = Ht - 2
    built = [panel(b, t(n), w, r, ph) for b, n, w, r in panels]
    rows = []
    appbar = render([(None, " "), ("emp", "Mike"), ("mut", " / "), (None, "settings"),
                     ("mut", " / "), ("emp", "tokens")], W)
    rows.append(f'<span class="appbar">{appbar}</span>')
    for parts in zip(*built):
        rows.append("".join(parts))
    rows.append(f'<span class="helpbar">{render(help_segs, W)}</span>')
    assert len(rows) == Ht
    return "\n".join(rows)

HELP_FULL = [(None, " "), ("key", "tab"), ("mut", " next panel · "),
             ("key", "enter"), ("mut", " open · "), ("key", "esc"), ("mut", " back · "),
             ("key", "n s t"), ("mut", " jump to panel · "), ("key", "q"), ("mut", " quit")]
HELP_STACK = [(None, " "), ("key", "tab"), ("mut", " panel · "), ("key", "enter"), ("mut", " open · "),
              ("key", "esc"), ("mut", " back · "), ("key", "n s t"), ("mut", " jump · "),
              ("key", "q"), ("mut", " quit")]
HELP_MODAL = [(None, " "), ("key", "esc"), ("mut", " close · "), ("key", "q"), ("mut", " quit")]

# Floor 80×24: nav content-sized at 12; middle and detail/preview split the rest equally.
SCREEN_FLOOR = screen(80, 24, [
    ("bd", "nav", 12, NAV_ROWS),
    ("bdf", "settings", 34, MID_ROWS),
    ("bd", "tokens", 34, DET_ROWS),
], HELP_FULL)

# Widescreen 160×40: nav stays content-sized; middle and detail/preview split the rest equally.
SCREEN_WIDE = screen(160, 40, [
    ("bd", "nav", 12, NAV_ROWS),
    ("bdf", "settings", 74, MID_ROWS),
    ("bd", "tokens", 74, DET_ROWS),
], HELP_FULL)

# Narrow 60×20: right items move below left items — tokens stacks below settings.
def stacked_screen():
    W, Ht = 60, 20
    nav = panel("bd", t("nav"), 12, NAV_ROWS, Ht - 2)
    mid = panel("bdf", t("settings"), 48, MID_ROWS, 9)
    det = panel("bd", t("tokens"), 48, DET_ROWS, 9)
    right = mid + det
    rows = []
    appbar = render([(None, " "), ("emp", "Mike"), ("mut", " / "), (None, "settings"),
                     ("mut", " / "), ("emp", "tokens")], W)
    rows.append(f'<span class="appbar">{appbar}</span>')
    for a, b in zip(nav, right):
        rows.append(a + b)
    rows.append(f'<span class="helpbar">{render(HELP_STACK, W)}</span>')
    assert len(rows) == Ht
    return "\n".join(rows)

SCREEN_SMALL = stacked_screen()

# Short 50×14: no rows to stack — tokens opens as a modal over the screen.
def modal_screen():
    W, Ht = 50, 14
    ch = Ht - 2  # content rows
    nav = panel("bd", t("nav"), 12, NAV_ROWS, ch)
    mid = panel("bd", t("settings"), 38, MID_ROWS, ch)
    modal = panel("bdf", t("tokens"), 30, DET_ROWS, 8)
    for i in range(8):
        inner = f'<span class="bd">│</span>' + "   " + modal[i] + "   " + f'<span class="bd">│</span>'
        mid[i + 2] = inner
    rows = []
    appbar = render([(None, " "), ("emp", "Mike"), ("mut", " / "), (None, "settings"),
                     ("mut", " / "), ("emp", "tokens")], W)
    rows.append(f'<span class="appbar">{appbar}</span>')
    for a, b in zip(nav, mid):
        rows.append(a + b)
    rows.append(f'<span class="helpbar">{render(HELP_MODAL, W)}</span>')
    assert len(rows) == Ht
    return "\n".join(rows)

SCREEN_MODAL = modal_screen()

CSS = FONT_RULES + """
/* Published SilkCircuit Neon tokens (lua/silkcircuit/variants.lua). Square geometry only. */
:root{
  --bg-base:#0a0812; --bg-surface:#12101a; --bg-overlay:#1a162a; --bg-selection:#44475a;
  --text-primary:#f8f8f2; --text-lavender:#e9d5ff; --text-muted:#6272a4; --text-emphasis:#ffffff;
  --accent-primary:#e135ff; --accent-secondary:#80ffea;
  --line:rgba(98,114,164,.28); --line-strong:#44475a;
}
*{box-sizing:border-box}
body{margin:0;background:var(--bg-base);color:var(--text-primary);
  font-family:"JetBrains Mono","Fira Code","Cascadia Code",ui-monospace,SFMono-Regular,Menlo,Consolas,monospace;
  font-size:14px;line-height:1.6}
.wrap{max-width:1200px;margin:0 auto;padding:0 24px 96px}
a{color:var(--accent-secondary);text-decoration:none}
a:hover{text-decoration:underline}
a:focus-visible{outline:1px solid var(--accent-primary);outline-offset:2px}
.term{background:var(--bg-base);border:1px solid var(--line-strong);overflow-x:auto;margin:28px 0 6px}
.term pre{margin:0;padding:14px 16px;line-height:1.0;color:var(--text-primary);
  font-family:'TermMono','JetBrains Mono',ui-monospace,SFMono-Regular,Menlo,Consolas,monospace}
.w60 pre{font-size:min(16px,calc((min(100vw,1200px) - 82px)/36.2))}
.w50 pre{font-size:min(16px,calc((min(100vw,1200px) - 82px)/30.2))}
.w80 pre{font-size:min(16px,calc((min(100vw,1200px) - 82px)/48.2))}
.w160 pre{font-size:min(12px,calc((min(100vw,1200px) - 82px)/96.4))}
.caption{font-size:12px;color:var(--text-muted);margin:6px 0 0}
.caption b{color:var(--text-lavender);font-weight:700}
.bd{color:var(--text-muted)}.bdf{color:var(--accent-primary)}
.tt{color:var(--text-emphasis);font-weight:700}
.sc{color:var(--accent-secondary);font-weight:700;text-decoration:underline}
.mut{color:var(--text-muted)}.emp{color:var(--text-emphasis);font-weight:700}
.key{color:var(--accent-secondary);font-weight:700}
.appbar{background:var(--bg-surface)}
.helpbar{background:var(--bg-surface)}
.act{background:var(--bg-selection);color:var(--text-emphasis)}
"""

CONTENT = f"""<title>TUI Design — Layout</title>
<style>{CSS}</style>
<div class="wrap">

<h1>TUI Design</h1>
<p class="caption">Universal layout rendition. The screens below are a labelled mike
application example; their settled frame geometry is normative, while panel-internal
content spacing remains reserved for the next generic <code>tui-kit</code> Panels phase.</p>

<div class="term w80"><pre>{SCREEN_FLOOR}</pre></div>
<p class="caption"><b>Mike application example: the floor — 80×24.</b> nav content-sized at 12 — never wider than its longest item — · middle and detail/preview split the rest equally (34 each).
Normative text: <a href="LAYOUT.md">docs/tui/LAYOUT.md</a>.</p>

<div class="term w160"><pre>{SCREEN_WIDE}</pre></div>
<p class="caption"><b>Mike application example: widescreen monitor — 160×40.</b> Same layout, wider: nav keeps its content-sized
12 columns; middle and detail/preview split the rest equally (74 each); extra rows go to
the panels. Nothing moves, nothing recenters.</p>

<div class="term w60"><pre>{SCREEN_SMALL}</pre></div>
<p class="caption"><b>Mike application example: narrow — 60×20.</b> Right items move below left items: tokens stacks
below settings, each keeping its full usable width. nav stays content-sized.</p>

<div class="term w50"><pre>{SCREEN_MODAL}</pre></div>
<p class="caption"><b>Mike application example: short — 50×14.</b> Not enough rows to stack: the panel's view becomes
a modal instead — tokens opens over the screen, esc closes back to the panels beneath.</p>

</div>
"""

head_end = CONTENT.index("</style>") + len("</style>")
FULL = ('<!DOCTYPE html>\n<html lang="en"><head>\n<meta charset="utf-8">\n'
        '<meta name="viewport" content="width=device-width,initial-scale=1">\n'
        + CONTENT[:head_end] + "\n</head><body>\n" + CONTENT[head_end:] + "\n</body></html>\n")

with open(OUT, "w") as f:
    f.write(FULL)
print(f"wrote {OUT} ({len(FULL)} bytes)")
if OUT_ARTIFACT:
    with open(OUT_ARTIFACT, "w") as f:
        f.write(CONTENT)
    print(f"wrote artifact fragment {OUT_ARTIFACT} ({len(CONTENT)} bytes)")
