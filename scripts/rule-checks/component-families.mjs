#!/usr/bin/env node
/**
 * component-families.mjs — stop a new duplicate before it ships.
 *
 * The frontend has 285 Svelte components under `frontend/src/lib/components`,
 * and a repeated failure mode is a new feature growing its own `FooCard.svelte`
 * or `BarPanel.svelte` instead of reusing or extending an existing one — e.g.
 * two unrelated `ToolCallCard.svelte` files living in `shell/` and `tool-calls/`
 * until unit B1 of the frontend-motion-and-consolidation thread merged them.
 *
 * This groups every production component by its filename SUFFIX (`Card`,
 * `Panel`, `Row`, ...) into "families" and requires `docs/frontend/
 * COMPONENT_CATALOGUE.md` to account for every family with two or more
 * members: one canonical component, and a one-line reason for every other
 * member explaining why it is not (yet) an extension of the canonical one.
 * A member missing from the catalogue fails the check by name — so a new
 * `FooCard.svelte` either becomes a `SectionCard` usage/variant, or the
 * author has to write down, in the catalogue, why it can't be.
 *
 * This module is imported directly (no subprocess) by
 * `frontend/src/lib/frontendArchitecture.guard.test.ts`, so it runs inside
 * `scripts/sb-test` on every frontend gate; it is also runnable standalone.
 *
 * Usage: node scripts/rule-checks/component-families.mjs
 * Exit 0 = CLEAN. Exit 1 = a family member is undocumented, or the catalogue
 * names a file that no longer exists.
 */

import { readFileSync, readdirSync, statSync, existsSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, resolve, relative, join, basename } from 'node:path';

const REPO = resolve(dirname(fileURLToPath(import.meta.url)), '..', '..');
const COMPONENTS_DIR = resolve(REPO, 'frontend/src/lib/components');
const CATALOGUE = resolve(REPO, 'docs/frontend/COMPONENT_CATALOGUE.md');

/**
 * Canonical family name -> accepted filename suffixes (exact, case-sensitive
 * match on the basename without `.svelte`). Case-sensitivity is deliberate:
 * this codebase's own components are PascalCase (`ToolCallCard.svelte`); the
 * lower-kebab-case files under `components/ui/**` (`dialog-header.svelte`,
 * `popover-content.svelte`) are bits-ui/shadcn-svelte primitive wrappers, out
 * of scope for this check, and a case-sensitive suffix never matches them.
 *
 * `Badge`/`Pill`/`Chip`/`Chips`/`Tag` are one family on purpose — the
 * operator named these six components (StatusPill, OutcomeBadge,
 * PromptKindBadge, ClassChip, Tag, ToolNamesChips) as one consolidation
 * target (unit B2), so the check must not treat "Badge" and "Chip" as
 * unrelated suffixes.
 */
export const FAMILIES = {
    Card: ['Card'],
    Panel: ['Panel'],
    Row: ['Row'],
    Strip: ['Strip'],
    Section: ['Section'],
    Band: ['Band'],
    Badge: ['Badge', 'Pill', 'Chip', 'Chips', 'Tag'],
    Header: ['Header'],
    Table: ['Table'],
    List: ['List'],
    Banner: ['Banner'],
    Drawer: ['Drawer'],
    Popover: ['Popover'],
    Indicator: ['Indicator'],
    Marker: ['Marker']
};

const EXCLUDED_DIRS = new Set(['__harness__', '__foundations__']);

/**
 * @param {string} dir
 * @returns {Generator<string>}
 */
function* walkSvelteFiles(dir) {
    for (const entry of readdirSync(dir)) {
        if (EXCLUDED_DIRS.has(entry)) continue;
        const abs = join(dir, entry);
        const st = statSync(abs);
        if (st.isDirectory()) {
            yield* walkSvelteFiles(abs);
        } else if (entry.endsWith('.svelte') && !entry.endsWith('.stories.svelte')) {
            yield abs;
        }
    }
}

/**
 * The family a component name belongs to, or `null` if it matches no suffix.
 * @param {string} componentName
 * @returns {string | null}
 */
export function familyOf(componentName) {
    for (const [family, suffixes] of Object.entries(FAMILIES)) {
        for (const suffix of suffixes) {
            if (componentName.endsWith(suffix)) return family;
        }
    }
    return null;
}

/**
 * `{ familyName: [repoRelativePath, ...] }` for every family with two or
 * more real members in the tree today. Families with 0-1 members are
 * dropped — nothing to reconcile yet.
 */
export function groupComponentsByFamily(componentsDir = COMPONENTS_DIR, repoRoot = REPO) {
    /** @type {Record<string, string[]>} */
    const groups = {};
    for (const abs of walkSvelteFiles(componentsDir)) {
        const name = basename(abs, '.svelte');
        const family = familyOf(name);
        if (!family) continue;
        (groups[family] ??= []).push(relative(repoRoot, abs));
    }
    for (const [family, members] of Object.entries(groups)) {
        members.sort();
        if (members.length < 2) delete groups[family];
    }
    return groups;
}

/**
 * Parses the catalogue's `## <Family>` sections. Each section holds exactly
 * one `Canonical: \`path\`` line and zero or more `- \`path\` — reason`
 * bullets (em dash or hyphen accepted). Returns
 * `{ familyName: { canonical: path|null, members: Map<path, reason> } }` —
 * the canonical path is also entered into `members` (reason `"(canonical)"`)
 * so membership checks don't need a separate branch.
 * @param {string} text
 * @returns {Record<string, { canonical: string | null, members: Map<string, string> }>}
 */
export function parseCatalogue(text) {
    /** @type {Record<string, { canonical: string | null, members: Map<string, string> }>} */
    const sections = {};
    /** @type {{ canonical: string | null, members: Map<string, string> } | null} */
    let current = null;
    for (const rawLine of text.split('\n')) {
        const heading = rawLine.match(/^##\s+(\S.*?)\s*$/);
        const rawHeading = heading?.[1];
        if (rawHeading !== undefined) {
            const family = rawHeading.replace(/\s*\(.*\)\s*$/, '').trim();
            const section = sections[family] ?? { canonical: null, members: new Map() };
            sections[family] = section;
            current = section;
            continue;
        }
        if (!current) continue;
        const canonicalMatch = rawLine.match(/^Canonical:\s*`([^`]+)`/);
        const rawPath = canonicalMatch?.[1];
        if (rawPath !== undefined) {
            const path = rawPath.trim();
            current.canonical = path;
            current.members.set(path, '(canonical)');
            continue;
        }
        const bulletMatch = rawLine.match(/^-\s*`([^`]+)`\s*[—-]\s*(.+?)\s*$/);
        const rawMemberPath = bulletMatch?.[1];
        const rawReason = bulletMatch?.[2];
        if (rawMemberPath !== undefined && rawReason !== undefined) {
            current.members.set(rawMemberPath.trim(), rawReason.trim());
        }
    }
    return sections;
}

/**
 * Runs the check. `catalogueText` is injectable for tests; production
 * callers read `docs/frontend/COMPONENT_CATALOGUE.md` from disk.
 */
export function checkCatalogue({
    componentsDir = COMPONENTS_DIR,
    catalogueText = null,
    repoRoot = REPO,
    cataloguePath = CATALOGUE
} = {}) {
    if (catalogueText === null && !existsSync(cataloguePath)) {
        return { ok: false, problems: [`Catalogue not found at ${relative(repoRoot, cataloguePath)}.`] };
    }
    const text = catalogueText ?? readFileSync(cataloguePath, 'utf8');
    const sections = parseCatalogue(text);
    const groups = groupComponentsByFamily(componentsDir, repoRoot);
    const problems = [];

    for (const [family, members] of Object.entries(groups)) {
        const section = sections[family];
        if (!section || !section.canonical) {
            problems.push(
                `Family "${family}" has ${members.length} members (${members.join(', ')}) but ` +
                    `${relative(repoRoot, cataloguePath)} has no "## ${family}" section naming a Canonical component.`
            );
            continue;
        }
        for (const member of members) {
            if (!section.members.has(member)) {
                problems.push(
                    `${member} is a new "${family}"-family component, undocumented in ` +
                        `${relative(repoRoot, cataloguePath)}. Either extend the family's canonical ` +
                        `component (${section.canonical}) instead of adding a new file, or add a ` +
                        `"- \`${member}\` — <reason>" line under "## ${family}" explaining why this one ` +
                        `is not an extension of it.`
                );
            }
        }
    }

    // A catalogue entry naming a file that no longer exists (renamed, merged, deleted).
    for (const [family, section] of Object.entries(sections)) {
        for (const member of section.members.keys()) {
            if (!existsSync(resolve(repoRoot, member))) {
                problems.push(
                    `${relative(repoRoot, cataloguePath)}'s "## ${family}" section names ${member}, ` +
                        `which no longer exists in the tree. Remove or update that entry.`
                );
            }
        }
    }

    return { ok: problems.length === 0, problems };
}

// CLI entrypoint — `node scripts/rule-checks/component-families.mjs`.
if (import.meta.url === `file://${process.argv[1]}`) {
    const { ok, problems } = checkCatalogue();
    if (ok) {
        const groups = groupComponentsByFamily();
        console.log(
            `CLEAN — ${Object.keys(groups).length} component-name families with 2+ members, ` +
                `all documented in ${relative(REPO, CATALOGUE)}.`
        );
        process.exit(0);
    }
    console.error(`COMPONENT FAMILY DRIFT — ${problems.length} problem(s):\n`);
    for (const p of problems) console.error(`  - ${p}`);
    console.error(
        `\nA new component named after an existing suffix family (Card, Panel, Row, Strip, ` +
            `Section, Band, Badge/Pill/Chip/Tag, Header, Table, List, Banner, Drawer, Popover, ` +
            `Indicator, Marker) needs a reason in docs/frontend/COMPONENT_CATALOGUE.md, or it needs ` +
            `to not exist — extend the family's canonical component instead.\n`
    );
    process.exit(1);
}
