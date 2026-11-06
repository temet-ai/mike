import { describe, it, expect } from 'vitest';
import { contextChipHref, contextChipLabel, contextChipMode } from './contextChip';
import type { ContextItem, ContextItemKind } from './types';

function item(overrides: Partial<ContextItem> & { kind: ContextItemKind }): ContextItem {
	return { value: 'v', label: null, ...overrides };
}

const SESSION_ID = 'sess-page';

describe('contextChipMode', () => {
	it('maps request/session to internal-link', () => {
		expect(contextChipMode(item({ kind: 'request', value: '42' }))).toBe('internal-link');
		expect(contextChipMode(item({ kind: 'session', value: 'sess-2' }))).toBe('internal-link');
	});

	it('maps a valid http(s) url to external-link', () => {
		expect(contextChipMode(item({ kind: 'url', value: 'http://example.com' }))).toBe(
			'external-link'
		);
		expect(contextChipMode(item({ kind: 'url', value: 'https://example.com' }))).toBe(
			'external-link'
		);
	});

	it('degrades an unsafe or unparseable url to label', () => {
		expect(contextChipMode(item({ kind: 'url', value: 'javascript:alert(1)' }))).toBe('label');
		expect(
			contextChipMode(item({ kind: 'url', value: 'data:text/html,<script>alert(1)</script>' }))
		).toBe('label');
		expect(contextChipMode(item({ kind: 'url', value: 'not a url' }))).toBe('label');
	});

	it('maps thread/message to action', () => {
		expect(contextChipMode(item({ kind: 'thread', value: 'thread-1' }))).toBe('action');
		expect(contextChipMode(item({ kind: 'message', value: 'msg-1' }))).toBe('action');
	});

	it('maps commit/file/issue/note to label', () => {
		expect(contextChipMode(item({ kind: 'commit', value: 'abc123' }))).toBe('label');
		expect(contextChipMode(item({ kind: 'file', value: 'src/x.ts' }))).toBe('label');
		expect(contextChipMode(item({ kind: 'issue', value: '#42' }))).toBe('label');
		expect(contextChipMode(item({ kind: 'note', value: 'just a note' }))).toBe('label');
	});

	it('accepts a bare kind (kind-only call shape) and still maps url to external-link', () => {
		expect(contextChipMode('url')).toBe('external-link');
		expect(contextChipMode('request')).toBe('internal-link');
		expect(contextChipMode('note')).toBe('label');
	});
});

describe('contextChipHref', () => {
	it('builds a request href relative to the page session, encoding both segments', () => {
		const href = contextChipHref(item({ kind: 'request', value: 'req/1 two' }), 'sess with space');
		expect(href).toBe(
			`/sessions/${encodeURIComponent('sess with space')}/requests/${encodeURIComponent('req/1 two')}`
		);
	});

	it('builds a session href pointing at the referenced session, encoding the value', () => {
		const href = contextChipHref(item({ kind: 'session', value: 'sess/2 b' }), SESSION_ID);
		expect(href).toBe(`/sessions/${encodeURIComponent('sess/2 b')}/comms`);
	});

	it('passes through http and https urls unchanged', () => {
		expect(contextChipHref(item({ kind: 'url', value: 'http://example.com/a' }), SESSION_ID)).toBe(
			'http://example.com/a'
		);
		expect(contextChipHref(item({ kind: 'url', value: 'https://example.com/a' }), SESSION_ID)).toBe(
			'https://example.com/a'
		);
	});

	it('returns null for javascript:, data:, and unparseable url values', () => {
		expect(
			contextChipHref(item({ kind: 'url', value: 'javascript:alert(1)' }), SESSION_ID)
		).toBeNull();
		expect(
			contextChipHref(
				item({ kind: 'url', value: 'data:text/html,<script>alert(1)</script>' }),
				SESSION_ID
			)
		).toBeNull();
		expect(contextChipHref(item({ kind: 'url', value: 'not a url' }), SESSION_ID)).toBeNull();
	});

	it('returns null for the non-link kinds', () => {
		for (const kind of [
			'thread',
			'message',
			'commit',
			'file',
			'issue',
			'note'
		] as ContextItemKind[]) {
			expect(contextChipHref(item({ kind, value: 'x' }), SESSION_ID)).toBeNull();
		}
	});
});

describe('contextChipLabel', () => {
	it('uses the label when present', () => {
		expect(contextChipLabel(item({ kind: 'note', value: 'raw', label: 'Nice label' }))).toBe(
			'Nice label'
		);
	});

	it('falls back to the value when label is null', () => {
		expect(contextChipLabel(item({ kind: 'note', value: 'raw', label: null }))).toBe('raw');
	});
});
