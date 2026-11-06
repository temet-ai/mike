import { describe, expect, it } from 'vitest';
import { extractLastUserMessageText, resolveDryRunSessionMessage } from './lastUserMessage';

describe('extractLastUserMessageText', () => {
	it('returns empty for non-array input', () => {
		expect(extractLastUserMessageText(null)).toBe('');
		expect(extractLastUserMessageText(undefined)).toBe('');
		expect(extractLastUserMessageText('not an array')).toBe('');
	});

	it('returns empty when there are no user messages', () => {
		expect(extractLastUserMessageText([{ role: 'assistant', content: 'hi' }])).toBe('');
		expect(extractLastUserMessageText([])).toBe('');
	});

	it('extracts plain string content from the last user message', () => {
		const messages = [
			{ role: 'user', content: 'first' },
			{ role: 'assistant', content: 'reply' },
			{ role: 'user', content: 'second' }
		];
		expect(extractLastUserMessageText(messages)).toBe('second');
	});

	it('joins text blocks from array content, skipping non-text blocks', () => {
		const messages = [
			{
				role: 'user',
				content: [
					{ type: 'text', text: 'Hello ' },
					{ type: 'tool_result', text: 'ignored' },
					{ type: 'text', text: 'world' }
				]
			}
		];
		expect(extractLastUserMessageText(messages)).toBe('Hello world');
	});

	it('picks the LAST user message, not the first', () => {
		const messages = [
			{ role: 'user', content: 'old question' },
			{ role: 'assistant', content: 'answer' },
			{ role: 'user', content: [{ type: 'text', text: 'new question' }] }
		];
		expect(extractLastUserMessageText(messages)).toBe('new question');
	});
});

describe('resolveDryRunSessionMessage', () => {
	it('falls back to the source message when filtered_messages is null (no rule touched messages)', () => {
		expect(resolveDryRunSessionMessage(null, 'the source message')).toBe('the source message');
		expect(resolveDryRunSessionMessage(undefined, 'the source message')).toBe('the source message');
	});

	it('extracts the real filtered text when filtered_messages is present, even if empty', () => {
		expect(resolveDryRunSessionMessage([], 'the source message')).toBe('');
		expect(
			resolveDryRunSessionMessage([{ role: 'user', content: 'filtered text' }], 'the source message')
		).toBe('filtered text');
	});
});
