import { describe, expect, it } from 'vitest';
import { HELP_CONTENT, helpFor, type HelpEntry } from './helpContent';

const REQUIRED_SLUGS = [
	'session',
	'request',
	'project',
	'byte-identity-forwarding',
	'sse-accumulator',
	'inspect-api',
	'anthropic-base-url',

	'request-status',
	'http-method',
	'http-status',
	'streaming-vs-non-streaming',
	'request-parameters',
	'thinking-mode',
	'model-id',

	'conversation-message',
	'tool-call',
	'tool-result',
	'tool-loop',
	'thinking-blocks',
	'thinking-block-signature',
	'system-reminder',
	'stop-reason',
	'subagent',

	'upstream-request-id',
	'upstream-message-id',

	'composition',
	'prompt-sha256',
	'block-hash',
	'composition-mode',
	'mode-observed',
	'mode-file-backed',
	'mode-dynamic',
	'composition-match',
	'match-matched',
	'match-unknown',
	'match-extra',
	'system-block',
	'volatile-lines',
	'anomaly',
	'anomaly-sub-pattern',
	'drift-queue',

	'grader',
	'analyzer',
	'finding',
	'weasel-pattern',
	'weasel-categories',
	'cat-justifying-shortcuts',
	'cat-deflecting-blame',
	'cat-avoiding-work',
	'cat-faking-understanding',
	'cat-overselling',

	'filter-rule',
	'filter-rule-kind',
	'kind-text-substring',
	'kind-xml-tag',
	'filter-rule-action',
	'action-drop',
	'action-replace',
	'action-append',

	'steering',
	'steerer',
	'intervention',
	'steering-authority',
	'steering-mode',

	'ttl',
	'project-pin',

	'usage-tokens',
	'prompt-cache',
	'cache-control',
	'cache-efficiency',
	'request-cost',
	'token-estimate',

	'anthropic-headers',
	'rate-limit-headers',
	'retry-after',

	'agent-registry',
	'agent-version',
	'mode-builtin',
	'agent-dispatches',
	'agent-prompt-versions',
	'agent-instances',

	'steerer',
	'bigbro',
	'refocus-steerer',
	'verifier-steerer',
	'steering-enrolment',
	'outcome-corrected',

	'lesson',
	'memory-stages',
	'stage-raw',
	'stage-daily',
	'stage-long-term',
	'operator-turn',
	'turn-outcome',
	'outcome-accepted',
	'outcome-shipped',
	'outcome-abandoned',
	'outcome-unknown',
	'semantic-recall',
	'embeddings',

	'sediment',
	'fleet'
];

describe('helpFor', () => {
	it('returns a complete entry for a known slug', () => {
		const entry = helpFor('composition') as HelpEntry;
		expect(entry).toBeDefined();
		expect(entry.slug).toBe('composition');
		expect(entry.term).toBe('Composition');
		expect(typeof entry.short).toBe('string');
		expect(entry.short.length).toBeGreaterThan(0);
		expect(typeof entry.long).toBe('string');
		expect(entry.long.length).toBeGreaterThan(0);
	});

	it('returns undefined for an unknown slug', () => {
		expect(helpFor('definitely-not-a-slug')).toBeUndefined();
	});

	it('exposes every required glossary slug', () => {
		const missing = REQUIRED_SLUGS.filter((slug) => !HELP_CONTENT[slug]);
		expect(missing).toEqual([]);
	});
});

describe('HELP_CONTENT entry shape', () => {
	it('keeps `short` strings reasonable for tooltip rendering', () => {
		const overlong = Object.values(HELP_CONTENT).filter((e) => e.short.length > 90);
		expect(overlong.map((e) => e.slug)).toEqual([]);
	});

	it('matches each entry to its key in HELP_CONTENT', () => {
		for (const [key, entry] of Object.entries(HELP_CONTENT)) {
			expect(entry.slug).toBe(key);
		}
	});
});
