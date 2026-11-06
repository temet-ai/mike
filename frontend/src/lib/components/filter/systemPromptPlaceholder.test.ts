import { describe, expect, it } from 'vitest';
import { showsSessionMainNote, systemPromptPlaceholder } from './systemPromptPlaceholder';

describe('systemPromptPlaceholder', () => {
	it('returns none when the request has blocks, regardless of source', () => {
		expect(systemPromptPlaceholder(2, 'exact')).toEqual({ kind: 'none' });
		expect(systemPromptPlaceholder(1, 'session_main')).toEqual({ kind: 'none' });
		expect(systemPromptPlaceholder(1, null)).toEqual({ kind: 'none' });
	});

	it('returns not_captured when there are no blocks and the source is not_captured', () => {
		expect(systemPromptPlaceholder(0, 'not_captured')).toEqual({ kind: 'not_captured' });
	});

	it('never reports not_captured for a request that resolved via a real source', () => {
		expect(systemPromptPlaceholder(0, 'exact')).toEqual({ kind: 'no_blocks' });
		expect(systemPromptPlaceholder(0, 'class')).toEqual({ kind: 'no_blocks' });
		expect(systemPromptPlaceholder(0, 'agent_instance')).toEqual({ kind: 'no_blocks' });
		expect(systemPromptPlaceholder(0, 'session_main')).toEqual({ kind: 'no_blocks' });
	});

	it('falls back to no_blocks when the detail has not loaded a source yet', () => {
		expect(systemPromptPlaceholder(0, null)).toEqual({ kind: 'no_blocks' });
	});
});

describe('showsSessionMainNote', () => {
	it('shows the note only once there are blocks resolved via the session main lane', () => {
		expect(showsSessionMainNote(2, 'session_main')).toBe(true);
	});

	it('hides the note for every other source', () => {
		expect(showsSessionMainNote(2, 'exact')).toBe(false);
		expect(showsSessionMainNote(2, 'class')).toBe(false);
		expect(showsSessionMainNote(2, 'agent_instance')).toBe(false);
		expect(showsSessionMainNote(2, 'not_captured')).toBe(false);
		expect(showsSessionMainNote(2, null)).toBe(false);
	});

	it('hides the note when there are no blocks to show, even if the source is session_main', () => {
		expect(showsSessionMainNote(0, 'session_main')).toBe(false);
	});
});
