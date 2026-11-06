#!/usr/bin/env node
// check-model — does the architecture model still describe this checkout?
//
// LikeC4's own validation proves the model is well formed. It cannot know that a module
// was moved, a pipeline stage was added or a schema was created. This reads the model as
// JSON and compares three things with the tree:
//
//   links    every link into this repository names a path that exists
//   stages   the request pipeline's stages are the ones in STAGE_ORDER, in that order
//   schemas  the database's schemas are the ones the migrations create
//
// An element tagged #planned is not built yet, so its links are not checked.
//
// Usage: node scripts/likec4/check-model.mjs <model.json>
//        scripts/arch check runs it after exporting the model.
// Exit 0 = the model matches. Exit 1 = it has drifted; each line says what to change.

import { existsSync, readFileSync, readdirSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..');
const repoUrl = /^https:\/\/github\.com\/temet-ai\/mike\/(?:tree|blob)\/main\/(.+?)(?:#.*)?$/;

const modelFile = process.argv[2];
if (!modelFile) {
	console.error('usage: check-model.mjs <model.json>');
	process.exit(2);
}
const model = JSON.parse(readFileSync(modelFile, 'utf8'));
const elements = Object.values(model.elements);
const relations = Object.values(model.relations);
const problems = [];

// --- links ------------------------------------------------------------------------------

let linksChecked = 0;
const checkLinks = (owner, links) => {
	for (const link of links ?? []) {
		const inRepo = repoUrl.exec(link.url);
		if (!inRepo) continue;
		linksChecked += 1;
		if (!existsSync(path.join(repoRoot, inRepo[1]))) {
			problems.push(`${owner}: links to ${inRepo[1]}, which is not in this checkout`);
		}
	}
};
for (const element of elements) {
	if (element.tags?.includes('planned')) continue;
	checkLinks(element.id, element.links);
}
for (const view of Object.values(model.views)) {
	checkLinks(`view ${view.id}`, view.links);
}

// --- pipeline stages --------------------------------------------------------------------

const pipelineId = 'mike.server.pipeline';
const pipelineSource = 'crates/mike/src/pipeline/mod.rs';
const declared = /STAGE_ORDER[^=]*=\s*\[([^\]]*)\]/.exec(
	readFileSync(path.join(repoRoot, pipelineSource), 'utf8'),
);
if (!declared) {
	problems.push(`${pipelineSource}: STAGE_ORDER was not found, so the stages cannot be compared`);
} else {
	// The injected-block replay runs before the stages and the composer after them; neither
	// is in STAGE_ORDER.
	const inCode = ['injected_block_replay']
		.concat([...declared[1].matchAll(/"([^"]+)"/g)].map((m) => m[1]))
		.concat('composer');
	const stages = new Map(
		elements
			.filter((e) => e.kind === 'stage' && e.id.startsWith(`${pipelineId}.`))
			.filter((e) => !e.tags?.includes('planned'))
			.map((e) => [e.id, e.metadata?.stage]),
	);
	const next = new Map(
		relations
			.filter((r) => r.kind === 'then' && stages.has(r.source.model))
			.map((r) => [r.source.model, r.target.model]),
	);
	const targets = new Set(next.values());
	const inModel = [];
	let at = [...stages.keys()].find((id) => !targets.has(id));
	while (at && inModel.length <= stages.size) {
		inModel.push(stages.get(at));
		at = next.get(at);
	}
	if (inModel.join(' > ') !== inCode.join(' > ')) {
		problems.push(
			`pipeline stages differ from STAGE_ORDER in ${pipelineSource}\n` +
				`    code:  ${inCode.join(' > ')}\n` +
				`    model: ${inModel.join(' > ')}\n` +
				`    fix docs/architecture/model/pipeline.c4 (each stage's metadata.stage and the -[then]-> chain)`,
		);
	}
}

// --- database schemas -------------------------------------------------------------------

const migrations = 'crates/mike/src/store/persistence/migrations';
// The pre-split schema every table once lived in. Nothing new goes there.
const notModelled = new Set(['claude_proxy']);
const created = new Set();
for (const file of readdirSync(path.join(repoRoot, migrations)).sort()) {
	if (!file.endsWith('.sql')) continue;
	const sql = readFileSync(path.join(repoRoot, migrations, file), 'utf8');
	for (const m of sql.matchAll(/^\s*(CREATE|DROP)\s+SCHEMA\s+(?:IF\s+(?:NOT\s+)?EXISTS\s+)?([a-z_]+)/gim)) {
		if (m[1].toUpperCase() === 'CREATE') created.add(m[2]);
		else created.delete(m[2]);
	}
}
const modelled = new Set(
	elements
		.filter((e) => e.kind === 'schema' && !e.tags?.includes('planned'))
		.map((e) => e.title),
);
for (const name of created) {
	if (!modelled.has(name) && !notModelled.has(name)) {
		problems.push(`schema ${name} is created in ${migrations} but is not in docs/architecture/model/data.c4`);
	}
}
for (const name of modelled) {
	if (!created.has(name)) {
		problems.push(`schema ${name} is in docs/architecture/model/data.c4 but no migration creates it`);
	}
}

// --- verdict ----------------------------------------------------------------------------

if (problems.length > 0) {
	console.error(`architecture model has drifted from the code (${problems.length}):`);
	for (const problem of problems) console.error(`  - ${problem}`);
	process.exit(1);
}
console.log(
	`model matches the code: ${linksChecked} source links, ${modelled.size} schemas, pipeline stage order`,
);
