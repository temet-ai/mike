import { describe, expect, it } from 'vitest';
import { flowActivity, flowActor } from './__fixtures__/sessionFlow';
import {
	chipFor,
	chipForActivity,
	chipForActor,
	chipForLiveEvent,
	EVALUATIVE_CHIPS,
	MIKE_CHIP_KEYS
} from './serviceRoles';

describe('session-flow serviceRoles', () => {
	it('has the fixed 15-key chip catalogue', () => {
		expect(MIKE_CHIP_KEYS).toHaveLength(15);
		expect(new Set(MIKE_CHIP_KEYS).size).toBe(15);
	});

	it('lights filters/composer/inject from manifest_stages by stage code', () => {
		expect(
			chipForActivity(
				flowActivity('a', {
					kind: 'manifest_stages',
					metadata: { ...flowActivity().metadata, stage: 'filter' }
				})
			)
		).toBe('filters');
		expect(
			chipForActivity(
				flowActivity('a', {
					kind: 'manifest_stages',
					metadata: { ...flowActivity().metadata, stage: 'inject' }
				})
			)
		).toBe('inject');
		expect(
			chipForActivity(
				flowActivity('a', {
					kind: 'manifest_stages',
					metadata: { ...flowActivity().metadata, stage: 'anything-else' }
				})
			)
		).toBe('composer');
	});

	it('lights judge/verifier/embed from their dedicated components and from llm_calls by role', () => {
		expect(chipForActivity(flowActivity('a', { kind: 'judgments' }))).toBe('judge');
		expect(chipForActivity(flowActivity('a', { kind: 'embeddings' }))).toBe('embed');
		expect(chipForActivity(flowActivity('a', { kind: 'verifier_evaluations' }))).toBe('verifier');
		expect(chipForActivity(flowActivity('a', { kind: 'interventions' }))).toBe('verifier');
		expect(
			chipForActivity(
				flowActivity('a', {
					kind: 'llm_calls',
					metadata: { ...flowActivity().metadata, role: 'judge' }
				})
			)
		).toBe('judge');
		expect(
			chipForActivity(
				flowActivity('a', {
					kind: 'llm_calls',
					metadata: { ...flowActivity().metadata, role: 'verifier' }
				})
			)
		).toBe('verifier');
		expect(
			chipForActivity(
				flowActivity('a', {
					kind: 'llm_calls',
					metadata: { ...flowActivity().metadata, role: 'steerer' }
				})
			)
		).toBe('verifier');
		expect(chipForActivity(flowActivity('a', { kind: 'llm_calls' }))).toBeNull();
	});

	it('lights mailbox and upkeep', () => {
		expect(chipForActivity(flowActivity('a', { kind: 'messages' }))).toBe('mailbox');
		expect(chipForActivity(flowActivity('a', { kind: 'consolidation' }))).toBe('upkeep');
	});

	it('the mailbox actor lights mailbox regardless of activity kind', () => {
		const mailboxActor = flowActor('mailbox:s1', { kind: 'mike', label: 'Mailbox' });
		expect(chipForActor(mailboxActor)).toBe('mailbox');
		expect(chipFor(mailboxActor, flowActivity('a', { kind: 'requests' }))).toBe('mailbox');
	});

	it('classifies judge/verifier/embed as evaluative (mikeout), everything else as audit', () => {
		expect(EVALUATIVE_CHIPS.has('judge')).toBe(true);
		expect(EVALUATIVE_CHIPS.has('verifier')).toBe(true);
		expect(EVALUATIVE_CHIPS.has('embed')).toBe(true);
		expect(EVALUATIVE_CHIPS.has('filters')).toBe(false);
		expect(EVALUATIVE_CHIPS.has('mailbox')).toBe(false);
	});

	it('maps the live-only DiagnosticReady signal to analyzers', () => {
		expect(chipForLiveEvent('DiagnosticReady')).toBe('analyzers');
		expect(chipForLiveEvent('SomethingElse')).toBeNull();
	});
});
