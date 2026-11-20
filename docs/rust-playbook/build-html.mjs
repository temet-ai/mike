#!/usr/bin/env node
// Assemble the single-file, offline-portable index.html for the Rust Service Playbook.
// All markdown is inlined (no runtime fetch → works from file://). Mermaid fences are rendered
// as <pre class="mermaid"> (the form mermaid v10 scans). marked + mermaid are vendored in assets/.
import { readFileSync, writeFileSync, existsSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';

const here = dirname(fileURLToPath(import.meta.url));
const R = (p) => readFileSync(join(here, p), 'utf8');

// Order: spine intro → 17 chapters → case studies → appendix.
const chapters = [
  ['01', '01-T01-workspace-crates.md', 'Workspace & crate boundaries', 'I'],
  ['02', '02-T02-hot-path-passthrough.md', 'The hot path & passthrough discipline', 'I'],
  ['03', '03-T03-db-planning.md', 'Plan for the database from day one', 'II'],
  ['04', '04-T04a-schema-design.md', 'Schema design & normalization', 'II'],
  ['05', '05-T04b-migrations-mechanics.md', 'Migrations: expand-only + discovery-before-scope', 'II'],
  ['06', '06-T05-persistence-boundary.md', 'The persistence boundary & write discipline', 'II'],
  ['07', '07-T07-async-concurrency.md', 'Async & concurrency discipline', 'III'],
  ['08', '08-T06-memory-resource-safety.md', 'Memory & resource safety (+ heavy compute off-process)', 'III'],
  ['09', '09-T08-error-handling.md', 'Error-handling architecture', 'III'],
  ['10', '10-T09-observability-fidelity.md', 'Observability & inspection fidelity', 'IV'],
  ['11', '11-T10a-db-test-isolation.md', 'DB-backed test isolation', 'IV'],
  ['12', '12-T10b-test-strategy.md', 'Test strategy: taxonomy, mocking, regression, parity', 'IV'],
  ['13', '13-T12-tooling-quality-gates.md', 'Tooling & quality gates', 'IV'],
  ['14', '14-T11a-config-architecture.md', 'Configuration architecture', 'IV'],
  ['15', '15-T11b-ops-deploy-safety.md', 'Operational safety & deploy', 'IV'],
  ['16', '16-T13a-agent-scope-delegation.md', 'Agent-assisted dev: scope & delegation', 'V'],
  ['17', '17-T13b-lean-main-turn.md', 'Completion integrity & the lean main turn', 'V'],
];
const parts = { I: 'Part I — Foundations & Structure', II: 'Part II — Data & Persistence', III: 'Part III — Runtime & Resource Safety', IV: 'Part IV — Quality, Observability & Operations', V: 'Part V — Building with AI Agents' };

// Rewrite all inter-document links into in-page anchors.
function rewrite(md) {
  return md
    // any chapter file ref (optionally prefixed by ./ ../ then optionally parts/) → #ch-NN (drop sub-fragment)
    .replace(/\]\((?:\.\.?\/)?(?:parts\/)?(\d\d)-[A-Za-z0-9-]+\.md(?:#[\w-]+)?\)/g, '](#ch-$1)')
    .replace(/\]\((?:\.\.?\/)?case-studies\.md(#cs-[\w-]+)\)/g, ']($1)')
    .replace(/\]\((?:\.\.?\/)?case-studies\.md\)/g, '](#case-studies)')
    .replace(/\]\((?:\.\.?\/)?appendix-a-adopt\.md\)/g, '](#appendix-a)')
    .replace(/\]\((?:\.\.?\/)?addendum-b-temet-proxy\.md(?:#[\w-]+)?\)/g, '](#addendum-b)');
}

const missing = chapters.filter(([, f]) => !existsSync(join(here, 'parts', f)));
if (missing.length) { console.error('MISSING chapter files:', missing.map((m) => m[1])); process.exit(2); }

let combined = '';
combined += `<a id="top"></a>\n\n` + rewrite(R('PLAYBOOK.md')) + '\n\n';
let lastPart = '';
for (const [num, file, , part] of chapters) {
  if (part !== lastPart) { combined += `\n\n<a id="part-${part}"></a>\n\n`; lastPart = part; }
  combined += `\n\n<a id="ch-${num}"></a>\n\n` + rewrite(R(join('parts', file))) + '\n\n---\n\n';
}
combined += `\n\n<a id="case-studies"></a>\n\n` + rewrite(R('case-studies.md')) + '\n\n---\n\n';
combined += `\n\n<a id="appendix-a"></a>\n\n` + rewrite(R('appendix-a-adopt.md')) + '\n\n---\n\n';
combined += `\n\n<a id="addendum-b"></a>\n\n` + rewrite(R('addendum-b-temet-proxy.md')) + '\n';

// neutralize any </script> that would close our embedding tag
const safeMd = combined.replace(/<\/script>/gi, '<\\/script>');

// left-nav
let nav = '';
let lp = '';
for (const [num, , title, part] of chapters) {
  if (part !== lp) { nav += `<div class="navpart">${parts[part]}</div>`; lp = part; }
  nav += `<a class="navlink" href="#ch-${num}">${num}. ${title}</a>`;
}
nav += `<div class="navpart">Appendices</div><a class="navlink" href="#case-studies">Case studies</a><a class="navlink" href="#appendix-a">Appendix A — adopt from day one</a><a class="navlink" href="#addendum-b">Addendum B — temet-proxy actions</a>`;

const html = `<!doctype html>
<html lang="en"><head>
<meta charset="utf-8"><meta name="viewport" content="width=device-width, initial-scale=1">
<title>The Rust Service Playbook</title>
<script src="assets/marked.min.js"></script>
<script src="assets/mermaid.min.js"></script>
<style>
  :root{--bg:#0f1419;--panel:#1a2027;--ink:#e6edf3;--mut:#9aa7b2;--acc:#4ea1ff;--line:#2b3640;--code:#0b0f14;--warn:#ffcf6b;}
  *{box-sizing:border-box}
  body{margin:0;background:var(--bg);color:var(--ink);font:16px/1.65 -apple-system,BlinkMacSystemFont,"Segoe UI",Roboto,Helvetica,Arial,sans-serif;overflow-x:hidden}
  #shell{display:grid;grid-template-columns:300px minmax(0,1fr);min-height:100vh}
  #nav{position:sticky;top:0;align-self:start;height:100vh;overflow:auto;background:var(--panel);border-right:1px solid var(--line)}
  #nav>summary{display:none}
  .navinner{padding:18px 14px}
  .brand{font-size:15px;font-weight:700;letter-spacing:.02em;margin:4px 6px 14px;color:var(--ink)}
  .navpart{margin:14px 6px 4px;font-size:11px;text-transform:uppercase;letter-spacing:.08em;color:var(--mut)}
  .navlink{display:block;padding:5px 8px;border-radius:6px;color:var(--ink);text-decoration:none;font-size:13.5px}
  .navlink:hover{background:#22303c;color:#fff}
  .navlink.on{background:#22303c}
  main{max-width:900px;margin:0 auto;padding:40px 40px 120px;min-width:0}
  h1,h2,h3{line-height:1.25;font-weight:680;overflow-wrap:anywhere}
  h1{font-size:clamp(23px,5.5vw,30px);border-bottom:1px solid var(--line);padding-bottom:.3em;margin-top:1.6em}
  h2{font-size:clamp(19px,4.2vw,22px);margin-top:1.8em}
  h3{font-size:clamp(16px,3.6vw,17px);color:#cbd6e0;margin-top:1.5em}
  a{color:var(--acc);overflow-wrap:anywhere} a:hover{text-decoration:underline}
  #content{overflow-wrap:break-word}
  code{background:var(--code);padding:.12em .38em;border-radius:5px;font:13.5px/1.5 ui-monospace,SFMono-Regular,Menlo,Consolas,monospace;overflow-wrap:anywhere}
  pre{background:var(--code);border:1px solid var(--line);border-radius:9px;padding:14px 16px;overflow:auto;max-width:100%}
  pre code{background:none;padding:0;overflow-wrap:normal}
  pre.mermaid{background:#f6f8fa;color:#111;text-align:center;border-color:#d0d7de;overflow-x:auto}
  pre.mermaid svg{max-width:100%!important;height:auto!important}
  blockquote{margin:1em 0;padding:.4em 1em;border-left:3px solid var(--acc);background:#172029;color:var(--mut);border-radius:0 8px 8px 0}
  table{display:block;overflow-x:auto;border-collapse:collapse;width:max-content;max-width:100%;margin:1.2em 0;font-size:14px}
  th,td{border:1px solid var(--line);padding:7px 10px;text-align:left;vertical-align:top}
  th{background:#1d2731}
  hr{border:none;border-top:1px solid var(--line);margin:2.6em 0}
  strong{color:#fff}
  @media(max-width:820px){
    #shell{display:block}
    #nav{height:auto;border-right:none;border-bottom:1px solid var(--line);z-index:20}
    #nav>summary{display:flex;align-items:center;gap:.5em;list-style:none;cursor:pointer;padding:13px 16px;font-weight:700;font-size:15px;color:var(--ink);position:sticky;top:0;background:var(--panel)}
    #nav>summary::-webkit-details-marker{display:none}
    #nav>summary::before{content:"\\2630";color:var(--acc)}
    #nav[open]>summary{border-bottom:1px solid var(--line)}
    .navinner{max-height:62vh;overflow:auto;padding:6px 12px 14px}
    .brand{display:none}
    .navlink{padding:8px 8px;font-size:14.5px}
    main{padding:22px 16px 80px}
    h2{margin-top:1.5em}
  }
  /* lesson callouts (Do this / Not this / Why / Evidence / Pattern) */
  .callout{border-left:3px solid var(--line);padding:.15em 0 .15em .9em;margin:.7em 0}
  .callout>strong:first-child{display:inline-block;font-size:10.5px;font-weight:700;text-transform:uppercase;letter-spacing:.05em;padding:1px 7px;border-radius:5px;margin-right:.5em;color:#0b0f14;vertical-align:.08em}
  .c-do{border-left-color:#3fb950}  .c-do>strong:first-child{background:#3fb950}
  .c-not{border-left-color:#f85149} .c-not>strong:first-child{background:#f85149}
  .c-why{border-left-color:#4ea1ff} .c-why>strong:first-child{background:#4ea1ff}
  .c-ev{border-left-color:#6e7d8a}  .c-ev>strong:first-child{background:#8b98a5}
  .c-pat{border-left-color:#d2a8ff} .c-pat>strong:first-child{background:#d2a8ff}
  /* Further reading */
  h2.furtherreading{font-size:clamp(16px,3.6vw,18px);margin-top:2.2em;padding-top:.7em;border-top:1px solid var(--line);color:var(--acc)}
  h2.furtherreading + ul{list-style:none;background:#141b22;border:1px solid var(--line);border-radius:10px;padding:12px 16px;font-size:14px;margin-top:.6em}
  h2.furtherreading + ul li{margin:.45em 0;padding-left:1.1em;position:relative}
  h2.furtherreading + ul li::before{content:"→";position:absolute;left:0;color:var(--acc);opacity:.7}
  /* external-link affordance */
  #content a[href^="http"]::after{content:"\\2197";font-size:.78em;margin-left:.12em;opacity:.55;vertical-align:.1em}
  /* motion */
  html{scroll-behavior:smooth}
  .reveal{opacity:0;transform:translateY(14px);transition:opacity .5s ease,transform .5s ease}
  .reveal.in{opacity:1;transform:none}
  pre.mermaid{opacity:0;transform:scale(.99);transition:opacity .6s ease,transform .6s ease}
  pre.mermaid.in{opacity:1;transform:none}
  #content>h1:first-of-type{animation:hero .7s ease both}
  .navlink{transition:background .15s ease,color .15s ease}
  @keyframes hero{from{opacity:0;transform:translateY(10px)}to{opacity:1;transform:none}}
</style>
</head><body>
<div id="shell">
  <details id="nav" open><summary>Contents</summary><div class="navinner"><div class="brand">The Rust Service Playbook</div>${nav}</div></details>
  <main id="content">Rendering…</main>
</div>
<script id="md" type="text/markdown">${safeMd}</script>
<script>
  const src = document.getElementById('md').textContent;
  const renderer = new marked.Renderer();
  const baseCode = renderer.code.bind(renderer);
  renderer.code = (code, lang) => {
    const text = typeof code === 'object' ? code.text : code;
    const language = typeof code === 'object' ? code.lang : lang;
    if ((language || '').trim() === 'mermaid') {
      const esc = text.replace(/&/g,'&amp;').replace(/</g,'&lt;').replace(/>/g,'&gt;');
      return '<pre class="mermaid">' + esc + '</pre>';
    }
    return baseCode(code, lang);
  };
  marked.setOptions({ renderer, gfm:true, breaks:false });
  document.getElementById('content').innerHTML = marked.parse(src);
  // post-process via DOM (avoids regex-on-HTML escaping hazards)
  const CO={'Do this':'do','Not this':'not','Why':'why','Evidence':'ev','Pattern':'pat'};
  document.querySelectorAll('#content p > strong:first-child').forEach(s=>{
    const m=(s.textContent||'').trim().match(/^(Do this|Not this|Why|Evidence|Pattern)\\b/);
    if(m) s.parentElement.classList.add('callout','c-'+CO[m[1]]);
  });
  document.querySelectorAll('#content h2').forEach(h=>{ if(/^\\s*Further reading\\s*$/i.test(h.textContent)) h.classList.add('furtherreading'); });
  mermaid.initialize({ startOnLoad:false, theme:'neutral', securityLevel:'loose' });
  mermaid.run({ querySelector:'pre.mermaid' })
    .then(()=>document.querySelectorAll('pre.mermaid').forEach(p=>p.classList.add('in')))
    .catch(e=>{console.error('mermaid',e);document.querySelectorAll('pre.mermaid').forEach(p=>p.classList.add('in'));});
  // scroll-reveal — section headers only (body text never hidden). Self-healing: any header at or
  // above the viewport is revealed, so nav-jumps that skip headers never leave them stuck hidden.
  const reveals=[...document.querySelectorAll('#content h2')];
  reveals.forEach(el=>el.classList.add('reveal'));
  function revealVisible(){ for(const el of reveals){ if(!el.classList.contains('in') && el.getBoundingClientRect().top < innerHeight*0.92) el.classList.add('in'); } }
  revealVisible();
  let revTick=false;
  addEventListener('scroll',()=>{ if(!revTick){ revTick=true; requestAnimationFrame(()=>{ revealVisible(); revTick=false; }); } }, {passive:true});
  // scrollspy
  const links=[...document.querySelectorAll('.navlink')];
  const map=new Map(links.map(a=>[a.getAttribute('href').slice(1),a]));
  const io=new IntersectionObserver((es)=>{for(const e of es){if(e.isIntersecting){const id=e.target.id;if(map.has(id)){links.forEach(l=>l.classList.remove('on'));map.get(id).classList.add('on');}}}},{rootMargin:'-10% 0px -85% 0px'});
  document.querySelectorAll('[id^="ch-"],#case-studies,#appendix-a,#addendum-b').forEach(el=>io.observe(el));
  // mobile nav: collapse by default on small screens; close after navigating
  const navEl=document.getElementById('nav');
  const mq=matchMedia('(max-width:820px)');
  const syncNav=()=>{ navEl.open = !mq.matches; };
  syncNav(); mq.addEventListener('change', syncNav);
  links.forEach(a=>a.addEventListener('click',()=>{ if(mq.matches) navEl.open=false; }));
</script>
</body></html>`;

writeFileSync(join(here, 'index.html'), html);
console.log('Wrote index.html (' + (html.length/1024).toFixed(0) + ' KB, ' + chapters.length + ' chapters inlined)');
