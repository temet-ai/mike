import { describe, expect, it } from 'vitest';
import { parseAssistantStreamFrame, SseFrameParser, type SseFrame } from './assistant';

describe('SseFrameParser', () => {
	it('emits nothing until a full blank-line-terminated frame arrives', () => {
		const parser = new SseFrameParser();
		expect(parser.push('event: text\n')).toEqual([]);
		expect(parser.push('data: {"delta":"hi"}\n')).toEqual([]);
		expect(parser.push('\n')).toEqual([{ event: 'text', data: '{"delta":"hi"}' }]);
	});

	it('parses a frame delivered in one chunk', () => {
		const parser = new SseFrameParser();
		const frames = parser.push('event: status\ndata: {"phase":"reading prompt"}\n\n');
		expect(frames).toEqual([{ event: 'status', data: '{"phase":"reading prompt"}' }]);
	});

	it('splits multiple frames arriving in a single chunk', () => {
		const parser = new SseFrameParser();
		const frames = parser.push(
			'event: text\ndata: {"delta":"a"}\n\nevent: text\ndata: {"delta":"b"}\n\n'
		);
		expect(frames).toEqual([
			{ event: 'text', data: '{"delta":"a"}' },
			{ event: 'text', data: '{"delta":"b"}' }
		]);
	});

	it('handles a frame split at an arbitrary byte boundary, mid-line', () => {
		const parser = new SseFrameParser();
		expect(parser.push('event: te')).toEqual([]);
		expect(parser.push('xt\ndata: {"del')).toEqual([]);
		expect(parser.push('ta":"hi"}\n\n')).toEqual([{ event: 'text', data: '{"delta":"hi"}' }]);
	});

	it('joins multiple data: lines within one frame with a newline (multi-line SSE data)', () => {
		const parser = new SseFrameParser();
		const frames = parser.push('event: text\ndata: line one\ndata: line two\n\n');
		expect(frames).toEqual([{ event: 'text', data: 'line one\nline two' }]);
	});

	it('defaults to event "message" when no event: line is present', () => {
		const parser = new SseFrameParser();
		const frames = parser.push('data: {"delta":"hi"}\n\n');
		expect(frames).toEqual([{ event: 'message', data: '{"delta":"hi"}' }]);
	});

	it('drops a frame with no data: line at all', () => {
		const parser = new SseFrameParser();
		expect(parser.push('event: status\n\n')).toEqual([]);
	});

	it('keeps a trailing partial frame buffered for the next push', () => {
		const parser = new SseFrameParser();
		parser.push('event: text\ndata: {"delta":"a"}\n\nevent: text\ndata: {"del');
		const frames = parser.push('ta":"b"}\n\n');
		expect(frames).toEqual([{ event: 'text', data: '{"delta":"b"}' }]);
	});
});

describe('parseAssistantStreamFrame', () => {
	function frame(event: string, data: unknown): SseFrame {
		return { event, data: JSON.stringify(data) };
	}

	it('parses a text delta frame', () => {
		expect(parseAssistantStreamFrame(frame('text', { delta: 'hello' }))).toEqual({
			type: 'text',
			delta: 'hello'
		});
	});

	it('parses a status frame', () => {
		expect(parseAssistantStreamFrame(frame('status', { phase: 'dry-run' }))).toEqual({
			type: 'status',
			phase: 'dry-run'
		});
	});

	it('parses an error frame', () => {
		expect(parseAssistantStreamFrame(frame('error', { message: 'budget exceeded' }))).toEqual({
			type: 'error',
			message: 'budget exceeded'
		});
	});

	it('parses a done frame', () => {
		expect(parseAssistantStreamFrame(frame('done', { cost_usd: 0.012 }))).toEqual({
			type: 'done',
			cost_usd: 0.012
		});
	});

	it('parses an applied frame', () => {
		expect(
			parseAssistantStreamFrame(frame('applied', { proposal_id: 'q1', rule_id: 'r-42' }))
		).toEqual({ type: 'applied', proposal_id: 'q1', rule_id: 'r-42' });
	});

	it('parses a well-formed proposal frame', () => {
		const proposal = {
			id: 'q1',
			title: 'Reword · Adds code-search guidance',
			edit_id: '60',
			action: 'replace' as const,
			match: 'Prefer ast-grep',
			before: 'Prefer ast-grep and the LSP tools…',
			after: 'Use grep for most searches…',
			reach: 'Goes to every main session prompt'
		};
		expect(parseAssistantStreamFrame(frame('proposal', proposal))).toEqual({
			type: 'proposal',
			proposal
		});
	});

	it('passes project_path through on a project-scoped proposal', () => {
		const proposal = {
			id: 'q3',
			title: 'Project-only tweak',
			edit_id: null,
			action: 'replace' as const,
			match: 'm',
			before: 'b',
			after: 'a',
			reach: 'r',
			project_path: '/mnt/data/projects/other'
		};
		expect(parseAssistantStreamFrame(frame('proposal', proposal))).toEqual({
			type: 'proposal',
			proposal
		});
	});

	it('rejects a proposal frame with an invalid action', () => {
		expect(
			parseAssistantStreamFrame(
				frame('proposal', {
					id: 'q1',
					title: 't',
					edit_id: null,
					action: 'delete',
					match: 'm',
					before: 'b',
					after: 'a',
					reach: 'r'
				})
			)
		).toBeNull();
	});

	it('rejects a proposal frame whose edit_id is a number (the wire contract is a string, matching FilterRuleDto.id)', () => {
		expect(
			parseAssistantStreamFrame(
				frame('proposal', {
					id: 'q1',
					title: 't',
					edit_id: 60,
					action: 'replace',
					match: 'm',
					before: 'b',
					after: 'a',
					reach: 'r'
				})
			)
		).toBeNull();
	});

	it('accepts a proposal with edit_id: null (a brand-new rule, not yet applied)', () => {
		const proposal = {
			id: 'q2',
			title: 't',
			edit_id: null,
			action: 'append' as const,
			match: 'm',
			before: '',
			after: 'a',
			reach: 'r'
		};
		expect(parseAssistantStreamFrame(frame('proposal', proposal))).toEqual({
			type: 'proposal',
			proposal
		});
	});

	it('returns null for malformed JSON data', () => {
		expect(parseAssistantStreamFrame({ event: 'text', data: 'not json' })).toBeNull();
	});

	it('returns null for an unrecognised event name', () => {
		expect(parseAssistantStreamFrame(frame('unknown-event', { anything: true }))).toBeNull();
	});

	it('returns null when a required field is missing or mistyped', () => {
		expect(parseAssistantStreamFrame(frame('text', { notDelta: 'x' }))).toBeNull();
		expect(parseAssistantStreamFrame(frame('done', { cost_usd: '0.01' }))).toBeNull();
	});
});
