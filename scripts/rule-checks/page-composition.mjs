#!/usr/bin/env node
/**
 * page-composition.mjs — a page is assembled from components, not restyled by hand.
 *
 * Counts, in every `frontend/src/routes/**` `+page.svelte` and `+layout.svelte`, the things a
 * page should get from a component instead of writing itself:
 *
 *   controls    raw <button>, <input>, <select>, <textarea>
 *   type        font-size, font-family, font-weight, letter-spacing or text-transform
 *               declared in the page's own <style> block
 *   chrome      data-augmented-ui written on the page
 *
 * The counts are held against `page-composition.baseline`. A page may not rise above its
 * recorded count, and a page with no record starts at zero. The baseline only ever goes
 * down: `--update` rewrites it, and refuses when any count would rise.
 *
 * Imported by `frontend/src/lib/frontendArchitecture.guard.test.ts`, so it runs inside
 * `scripts/sb-test`; also runnable standalone.
 *
 * Usage: node scripts/rule-checks/page-composition.mjs [--update]
 * Exit 0 = CLEAN. Exit 1 = a page rose above its baseline.
 */

import {
  readFileSync,
  writeFileSync,
  readdirSync,
  statSync,
  existsSync,
} from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, resolve, relative, join } from "node:path";

const REPO = resolve(dirname(fileURLToPath(import.meta.url)), "..", "..");
const ROUTES = resolve(REPO, "frontend/src/routes");
const BASELINE = resolve(REPO, "scripts/rule-checks/page-composition.baseline");

const CONTROL = /<(button|input|select|textarea)[\s>]/g;
const TYPE =
  /^\s*(font-size|font-family|font-weight|letter-spacing|text-transform)\s*:/gm;
const CHROME = /data-augmented-ui=/g;

/**
 * @param {string} dir
 * @returns {Generator<string>}
 */
function* pages(dir) {
  for (const entry of readdirSync(dir)) {
    const abs = join(dir, entry);
    if (statSync(abs).isDirectory()) yield* pages(abs);
    else if (entry === "+page.svelte" || entry === "+layout.svelte") yield abs;
  }
}

/** @param {string} source */
export function countPage(source) {
  const styleStart = source.lastIndexOf("<style");
  const markup = styleStart === -1 ? source : source.slice(0, styleStart);
  const style = styleStart === -1 ? "" : source.slice(styleStart);
  const body = markup.replace(/<script[\s\S]*?<\/script>/g, "");
  return {
    controls: (body.match(CONTROL) ?? []).length,
    type: (style.match(TYPE) ?? []).length,
    chrome: (body.match(CHROME) ?? []).length,
  };
}

export function measure(routesDir = ROUTES, repoRoot = REPO) {
  /** @type {Record<string, {controls: number, type: number, chrome: number}>} */
  const counts = {};
  for (const abs of pages(routesDir)) {
    counts[relative(repoRoot, abs)] = countPage(readFileSync(abs, "utf8"));
  }
  return counts;
}

export function readBaseline(file = BASELINE) {
  /** @type {Record<string, {controls: number, type: number, chrome: number}>} */
  const baseline = {};
  if (!existsSync(file)) return baseline;
  for (const line of readFileSync(file, "utf8").split("\n")) {
    if (line.startsWith("#") || line.trim() === "") continue;
    const [controls, type, chrome, ...rest] = line.split("\t");
    baseline[rest.join("\t")] = {
      controls: Number(controls),
      type: Number(type),
      chrome: Number(chrome),
    };
  }
  return baseline;
}

export function checkComposition(
  counts = measure(),
  baseline = readBaseline(),
) {
  const problems = [];
  for (const [file, now] of Object.entries(counts)) {
    const allowed = baseline[file] ?? { controls: 0, type: 0, chrome: 0 };
    for (const kind of /** @type {const} */ (["controls", "type", "chrome"])) {
      if (now[kind] > allowed[kind]) {
        problems.push(
          `${file}: ${kind} ${now[kind]}, allowed ${allowed[kind]}`,
        );
      }
    }
  }
  return { ok: problems.length === 0, problems };
}

/** @param {Record<string, {controls: number, type: number, chrome: number}>} counts */
function writeBaseline(counts) {
  const lines = [
    "# controls\ttype\tchrome\tpage — see scripts/rule-checks/page-composition.mjs",
    "# Counts only go down. Rewrite with --update after moving markup into a component.",
  ];
  for (const [file, { controls, type, chrome }] of Object.entries(counts).sort(
    ([left], [right]) => left.localeCompare(right),
  )) {
    if (controls + type + chrome === 0) continue;
    lines.push([controls, type, chrome, file].join("\t"));
  }
  writeFileSync(BASELINE, lines.join("\n") + "\n");
}

if (
  process.argv[1] &&
  resolve(process.argv[1]) === fileURLToPath(import.meta.url)
) {
  const counts = measure();
  const { ok, problems } = checkComposition(counts);
  if (process.argv.includes("--update")) {
    if (!ok && existsSync(BASELINE)) {
      console.error("REFUSED: the baseline only goes down. These rose:");
      for (const problem of problems) console.error("  " + problem);
      process.exit(1);
    }
    writeBaseline(counts);
    console.log(`Wrote ${relative(REPO, BASELINE)}`);
    process.exit(0);
  }
  if (ok) {
    console.log("CLEAN: no page rose above its recorded count.");
    process.exit(0);
  }
  console.error("PAGE RESTYLED BY HAND: import the component instead.");
  console.error(
    "scripts/component-index <word> names the component for a pattern.",
  );
  for (const problem of problems) console.error("  " + problem);
  process.exit(1);
}
