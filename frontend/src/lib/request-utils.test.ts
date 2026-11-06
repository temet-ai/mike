import { describe, expect, it } from 'vitest';
import { isBackgroundRequest, subagentBadgeLabel, turnOriginLabel } from './request-utils';

describe('isBackgroundRequest', () => {
	it('returns true for count_tokens path', () => {
		expect(isBackgroundRequest({ path: '/v1/messages/count_tokens', max_tokens: null })).toBe(true);
	});

	it('returns false for normal /v1/messages path with a real max_tokens', () => {
		expect(isBackgroundRequest({ path: '/v1/messages', max_tokens: 1024 })).toBe(false);
	});

	it('returns false for /v1/messages when max_tokens is unknown', () => {
		expect(isBackgroundRequest({ path: '/v1/messages', max_tokens: null })).toBe(false);
	});

	it('does NOT classify haiku model as background (haiku alone is not a probe)', () => {
		expect(isBackgroundRequest({ path: '/v1/messages', max_tokens: 8192 })).toBe(false);
	});

	it('returns true for count_tokens regardless of other fields', () => {
		expect(isBackgroundRequest({ path: '/v1/messages/count_tokens', max_tokens: 1 })).toBe(true);
	});

	it('returns false for empty path', () => {
		expect(isBackgroundRequest({ path: '', max_tokens: null })).toBe(false);
	});

	it('classifies /v1/messages with max_tokens === 1 as a quota probe', () => {
		expect(isBackgroundRequest({ path: '/v1/messages', max_tokens: 1 })).toBe(true);
	});

	it('does not classify non-message paths with max_tokens === 1 as background', () => {
		expect(isBackgroundRequest({ path: '/v1/other', max_tokens: 1 })).toBe(false);
	});
});

describe('turnOriginLabel', () => {
	it('returns "session start" for primary_first', () => {
		expect(turnOriginLabel({ turn_origin: 'primary_first' })).toBe('session start');
	});

	it('returns "title-gen" / "advisor" for sidecars', () => {
		expect(turnOriginLabel({ turn_origin: 'title_sidecar' })).toBe('title-gen');
		expect(turnOriginLabel({ turn_origin: 'advisor_sidecar' })).toBe('advisor');
	});

	it('returns "subagent" for subagent rows', () => {
		expect(turnOriginLabel({ turn_origin: 'subagent' })).toBe('subagent');
	});

	it('returns "quota-probe" for quota probes', () => {
		expect(turnOriginLabel({ turn_origin: 'quota_probe' })).toBe('quota-probe');
	});

	it('returns empty string for plain primary so the chip suppresses', () => {
		expect(turnOriginLabel({ turn_origin: 'primary' })).toBe('');
	});

	it('returns empty string when turn_origin is absent', () => {
		expect(turnOriginLabel({ turn_origin: undefined })).toBe('');
		expect(turnOriginLabel({})).toBe('');
	});
});

describe('subagentBadgeLabel', () => {
	it('returns "subagent · <persona>" for a subagent row with a known persona', () => {
		expect(
			subagentBadgeLabel({
				turn_origin: 'subagent',
				subagent_persona: 'Frontend Development Expert'
			})
		).toBe('subagent · Frontend Development Expert');
	});

	it('renders the full persona without truncation, including longer labels', () => {
		const persona = 'UI/design-system specialist who maintains the style guide';
		expect(
			subagentBadgeLabel({
				turn_origin: 'subagent',
				subagent_persona: persona
			})
		).toBe(`subagent · ${persona}`);
	});

	it('returns the bare "subagent" label when persona is null', () => {
		expect(subagentBadgeLabel({ turn_origin: 'subagent', subagent_persona: null })).toBe(
			'subagent'
		);
	});

	it('returns the bare "subagent" label when persona is undefined', () => {
		expect(subagentBadgeLabel({ turn_origin: 'subagent', subagent_persona: undefined })).toBe(
			'subagent'
		);
		expect(subagentBadgeLabel({ turn_origin: 'subagent' })).toBe('subagent');
	});

	it('returns the bare "subagent" label when persona is the empty string', () => {
		expect(subagentBadgeLabel({ turn_origin: 'subagent', subagent_persona: '' })).toBe('subagent');
	});

	it('returns null for non-subagent origins', () => {
		expect(subagentBadgeLabel({ turn_origin: 'primary', subagent_persona: null })).toBeNull();
		expect(
			subagentBadgeLabel({
				turn_origin: 'primary_first',
				subagent_persona: null
			})
		).toBeNull();
		expect(
			subagentBadgeLabel({
				turn_origin: 'title_sidecar',
				subagent_persona: 'irrelevant'
			})
		).toBeNull();
		expect(
			subagentBadgeLabel({
				turn_origin: 'advisor_sidecar',
				subagent_persona: null
			})
		).toBeNull();
	});

	it('returns null when turn_origin is absent (legacy backend)', () => {
		expect(subagentBadgeLabel({ turn_origin: undefined })).toBeNull();
		expect(subagentBadgeLabel({})).toBeNull();
	});

	it('ignores stray persona on non-subagent rows', () => {
		expect(
			subagentBadgeLabel({
				turn_origin: 'primary',
				subagent_persona: 'Frontend Development Expert'
			})
		).toBeNull();
	});
});
