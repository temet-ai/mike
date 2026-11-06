// @vitest-environment jsdom

import { describe, expect, it } from 'vitest';
import {
	SENSITIVE_HEADER_NAMES,
	isSensitiveHeader,
	displayHeaderValue,
	helpForHeader,
	KNOWN_HEADERS_HELP
} from './HeadersTable.svelte';

const MASKED = '\u2022\u2022\u2022\u2022\u2022\u2022\u2022\u2022';

describe('SENSITIVE_HEADER_NAMES set', () => {
	it('contains the four documented sensitive headers in lowercase', () => {
		expect(SENSITIVE_HEADER_NAMES.has('x-api-key')).toBe(true);
		expect(SENSITIVE_HEADER_NAMES.has('authorization')).toBe(true);
		expect(SENSITIVE_HEADER_NAMES.has('cookie')).toBe(true);
		expect(SENSITIVE_HEADER_NAMES.has('set-cookie')).toBe(true);
		expect(SENSITIVE_HEADER_NAMES.size).toBe(4);
	});
});

describe('isSensitiveHeader / displayHeaderValue', () => {
	it('matches case-insensitively across all four sensitive names', () => {
		const variants: Array<[string, string]> = [
			['x-api-key', 'sk-ant-real'],
			['X-API-KEY', 'sk-ant-real'],
			['X-Api-Key', 'sk-ant-real'],
			['authorization', 'Bearer abc'],
			['Authorization', 'Bearer abc'],
			['AUTHORIZATION', 'Bearer abc'],
			['cookie', 'session=xyz'],
			['Cookie', 'session=xyz'],
			['set-cookie', 'session=xyz; HttpOnly'],
			['Set-Cookie', 'session=xyz; HttpOnly']
		];
		for (const [name, value] of variants) {
			expect(isSensitiveHeader(name)).toBe(true);
			expect(displayHeaderValue(name, value)).toBe(MASKED);
			expect(displayHeaderValue(name, value)).not.toContain(value);
		}
	});

	it('returns the original value for non-sensitive headers', () => {
		expect(isSensitiveHeader('content-type')).toBe(false);
		expect(displayHeaderValue('content-type', 'application/json')).toBe('application/json');
		expect(isSensitiveHeader('accept-encoding')).toBe(false);
		expect(displayHeaderValue('accept-encoding', 'gzip')).toBe('gzip');
	});
});

describe('KNOWN_HEADERS_HELP / helpForHeader', () => {
	it('exposes Anthropic-specific tooltip copy keyed lowercase', () => {
		expect(KNOWN_HEADERS_HELP['anthropic-version']).toBeDefined();
		expect(KNOWN_HEADERS_HELP['anthropic-version']?.text).toContain('API version');
		expect(KNOWN_HEADERS_HELP['retry-after']?.slug).toBe('retry-after');
	});

	it('looks up help case-insensitively', () => {
		expect(helpForHeader('Anthropic-Version')?.text).toBe(
			KNOWN_HEADERS_HELP['anthropic-version']?.text
		);
		expect(helpForHeader('ANTHROPIC-RATELIMIT-REQUESTS-REMAINING')?.slug).toBe(
			'rate-limit-headers'
		);
	});

	it('returns undefined for unknown headers (no fabrication)', () => {
		expect(helpForHeader('x-totally-made-up')).toBeUndefined();
	});
});
