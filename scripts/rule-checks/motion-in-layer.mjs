#!/usr/bin/env node
/**
 * motion-in-layer.mjs — motion comes from the shared layer, never from the component.
 *
 * The shared layer is `frontend/src/lib/components/ui/motion.css`, `ui/motion.ts`,
 * `ui/elevation.css` and `lib/actions/reveal.ts`. A component names its motion with
 * `data-motion="…"` and takes its timing from there. Counts, in every non-story `.svelte`
 * file under `frontend/src`, the motion it writes itself:
 *
 *   transition  a `transition` or `transition-*` declaration, in <style> or a style attribute
 *   animation   an `animation` or `animation-*` declaration, in <style> or a style attribute
 *   keyframes   an `@keyframes` block
 *   directive   a Svelte `transition:`, `in:` or `out:` directive with a literal duration,
 *               or using a `svelte/transition` built-in, whose default duration is also a
 *               number nothing keeps in step with the tokens
 *   utility     a Tailwind motion utility in a class attribute: `transition-*`,
 *               `duration-*`, `ease-*`, `delay-*`, `animate-*`
 *
 * The counts are held against `motion-in-layer.baseline`. A file may not rise above its
 * recorded count, and a file with no record starts at zero. The baseline only ever goes
 * down: `--update` rewrites it, and refuses when any count would rise.
 *
 * Ready to be imported by `frontend/src/lib/frontendArchitecture.guard.test.ts`, the way
 * `page-composition.mjs` is; also runnable standalone.
 *
 * Usage: node scripts/rule-checks/motion-in-layer.mjs [--update] [--list]
 * Exit 0 = CLEAN. Exit 1 = a file rose above its baseline.
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
const SOURCE = resolve(REPO, "frontend/src");
const BASELINE = resolve(REPO, "scripts/rule-checks/motion-in-layer.baseline");

/** @typedef {{transition: number, animation: number, keyframes: number, directive: number, utility: number}} MotionCount */

/** @type {ReadonlyArray<keyof MotionCount>} */
const KINDS = ["transition", "animation", "keyframes", "directive", "utility"];

const TRANSITION_DECL = /(?<![\w-])transition(?:-[a-z]+)*\s*:/g;
const ANIMATION_DECL = /(?<![\w-])animation(?:-[a-z]+)*\s*:/g;
const KEYFRAMES = /@keyframes\s/g;
const STYLE_ATTR = /\sstyle(?::[\w-]+)?=(?:"([^"]*)"|'([^']*)'|\{`([^`]*)`\})/g;
const DIRECTIVE =
  /\s(?:transition|in|out):([A-Za-z_$][\w$]*)(?:\|[\w|]+)?(?:=\{\{([\s\S]*?)\}\})?/g;
const BUILT_INS = new Set([
  "fade",
  "blur",
  "fly",
  "slide",
  "scale",
  "draw",
  "crossfade",
  "send",
  "receive",
]);
const LITERAL_DURATION = /\bduration\s*:\s*\d/;
const CLASS_ATTR = /\sclass=(?:"([^"]*)"|'([^']*)')/g;
const UTILITY =
  /^(?:[\w-]+:)*(?:transition(?:-(?:all|colors|opacity|shadow|transform|none|discrete))?|duration-[\w[\].-]+|ease-(?:linear|in|out|in-out|\[[^\]]+\])|delay-[\w[\].-]+|animate-[\w[\]().-]+)$/;

/**
 * @param {string} dir
 * @returns {Generator<string>}
 */
function* components(dir) {
  for (const entry of readdirSync(dir)) {
    const abs = join(dir, entry);
    if (statSync(abs).isDirectory()) yield* components(abs);
    else if (entry.endsWith(".svelte") && !entry.endsWith(".stories.svelte"))
      yield abs;
  }
}

/**
 * @param {string} text
 * @param {RegExp} pattern
 * @returns {number}
 */
function count(text, pattern) {
  return (text.match(pattern) ?? []).length;
}

/**
 * @param {string} markup
 * @param {RegExp} pattern
 * @returns {string[]}
 */
function attributeValues(markup, pattern) {
  /** @type {string[]} */
  const values = [];
  for (const match of markup.matchAll(pattern)) {
    values.push(match[1] ?? match[2] ?? match[3] ?? "");
  }
  return values;
}

/**
 * @param {string} markup
 * @returns {number}
 */
function countDirectives(markup) {
  let found = 0;
  for (const match of markup.matchAll(DIRECTIVE)) {
    const name = match[1] ?? "";
    const params = match[2] ?? "";
    if (BUILT_INS.has(name) || LITERAL_DURATION.test(params)) found += 1;
  }
  return found;
}

/**
 * @param {string} markup
 * @returns {number}
 */
function countUtilities(markup) {
  let found = 0;
  for (const value of attributeValues(markup, CLASS_ATTR)) {
    for (const token of value.split(/\s+/)) {
      if (token && UTILITY.test(token)) found += 1;
    }
  }
  return found;
}

/**
 * @param {string} source
 * @returns {MotionCount}
 */
export function countMotion(source) {
  const styles = [...source.matchAll(/<style[^>]*>([\s\S]*?)<\/style>/g)]
    .map((match) => match[1] ?? "")
    .join("\n")
    .replace(/\/\*[\s\S]*?\*\//g, "");
  const markup = source
    .replace(/<style[^>]*>[\s\S]*?<\/style>/g, "")
    .replace(/<script[^>]*>[\s\S]*?<\/script>/g, "")
    .replace(/<!--[\s\S]*?-->/g, "");
  const inline = attributeValues(markup, STYLE_ATTR).join(";");
  return {
    transition: count(styles, TRANSITION_DECL) + count(inline, TRANSITION_DECL),
    animation: count(styles, ANIMATION_DECL) + count(inline, ANIMATION_DECL),
    keyframes: count(styles, KEYFRAMES),
    directive: countDirectives(markup),
    utility: countUtilities(markup),
  };
}

/**
 * @param {string} [sourceDir]
 * @param {string} [repoRoot]
 * @returns {Record<string, MotionCount>}
 */
export function measureMotion(sourceDir = SOURCE, repoRoot = REPO) {
  /** @type {Record<string, MotionCount>} */
  const counts = {};
  for (const abs of components(sourceDir)) {
    counts[relative(repoRoot, abs)] = countMotion(readFileSync(abs, "utf8"));
  }
  return counts;
}

/**
 * @param {string} [file]
 * @returns {Record<string, MotionCount>}
 */
export function readMotionBaseline(file = BASELINE) {
  /** @type {Record<string, MotionCount>} */
  const baseline = {};
  if (!existsSync(file)) return baseline;
  for (const line of readFileSync(file, "utf8").split("\n")) {
    if (line.startsWith("#") || line.trim() === "") continue;
    const fields = line.split("\t");
    const numbers = fields.slice(0, KINDS.length).map(Number);
    baseline[fields.slice(KINDS.length).join("\t")] = {
      transition: numbers[0] ?? 0,
      animation: numbers[1] ?? 0,
      keyframes: numbers[2] ?? 0,
      directive: numbers[3] ?? 0,
      utility: numbers[4] ?? 0,
    };
  }
  return baseline;
}

/**
 * @param {Record<string, MotionCount>} [counts]
 * @param {Record<string, MotionCount>} [baseline]
 * @returns {{ok: boolean, problems: string[]}}
 */
export function checkMotionInLayer(
  counts = measureMotion(),
  baseline = readMotionBaseline(),
) {
  /** @type {string[]} */
  const problems = [];
  const none = {
    transition: 0,
    animation: 0,
    keyframes: 0,
    directive: 0,
    utility: 0,
  };
  for (const [file, now] of Object.entries(counts)) {
    const allowed = baseline[file] ?? none;
    for (const kind of KINDS) {
      if (now[kind] > allowed[kind]) {
        problems.push(
          `${file}: ${kind} ${now[kind]}, allowed ${allowed[kind]}`,
        );
      }
    }
  }
  return { ok: problems.length === 0, problems };
}

/**
 * @param {MotionCount} counts
 * @returns {number}
 */
function total(counts) {
  return KINDS.reduce((sum, kind) => sum + counts[kind], 0);
}

/** @param {Record<string, MotionCount>} counts */
function writeBaseline(counts) {
  const lines = [
    "# transition\tanimation\tkeyframes\tdirective\tutility\tfile — see scripts/rule-checks/motion-in-layer.mjs",
    "# Counts only go down. Rewrite with --update after moving motion into the shared layer.",
  ];
  for (const [file, row] of Object.entries(counts).sort(([left], [right]) =>
    left.localeCompare(right),
  )) {
    if (total(row) === 0) continue;
    lines.push([...KINDS.map((kind) => row[kind]), file].join("\t"));
  }
  writeFileSync(BASELINE, lines.join("\n") + "\n");
}

/** @param {Record<string, MotionCount>} counts */
function printList(counts) {
  /** @type {MotionCount} */
  const sum = {
    transition: 0,
    animation: 0,
    keyframes: 0,
    directive: 0,
    utility: 0,
  };
  let files = 0;
  for (const [file, row] of Object.entries(counts).sort(
    ([, left], [, right]) => total(right) - total(left),
  )) {
    if (total(row) === 0) continue;
    files += 1;
    for (const kind of KINDS) sum[kind] += row[kind];
    console.log(`${KINDS.map((kind) => row[kind]).join("\t")}\t${file}`);
  }
  console.log(
    `${files} files: ${KINDS.map((kind) => `${sum[kind]} ${kind}`).join(", ")}`,
  );
}

if (
  process.argv[1] &&
  resolve(process.argv[1]) === fileURLToPath(import.meta.url)
) {
  const counts = measureMotion();
  if (process.argv.includes("--list")) {
    printList(counts);
    process.exit(0);
  }
  const { ok, problems } = checkMotionInLayer(counts);
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
    console.log(
      "CLEAN: no component wrote more motion than its recorded count.",
    );
    process.exit(0);
  }
  console.error("MOTION WRITTEN BY HAND: name it with data-motion instead.");
  console.error(
    "The layer is frontend/src/lib/components/ui/motion.css; the gallery is Foundations/Motion.",
  );
  for (const problem of problems) console.error("  " + problem);
  process.exit(1);
}
