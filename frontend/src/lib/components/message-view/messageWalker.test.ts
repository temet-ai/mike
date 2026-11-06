import { describe, expect, it } from 'vitest';
import {
	extractTrailingUserTurn,
	summarizeToolInput,
	userMessageInlinePreview,
	walkRequestMessages,
	type ContentBlock
} from './messageWalker';

describe('summarizeToolInput', () => {
	it('returns the bash command verbatim', () => {
		expect(summarizeToolInput('Bash', { command: 'git status' })).toBe('git status');
	});

	it('preserves multi-line bash commands without truncation', () => {
		const cmd = 'set -e\nnpm install\nnpm test';
		expect(summarizeToolInput('Bash', { command: cmd })).toBe(cmd);
	});

	it('returns the file_path for Read / Write / Edit', () => {
		expect(summarizeToolInput('Read', { file_path: '/tmp/x.log' })).toBe('/tmp/x.log');
		expect(summarizeToolInput('Write', { file_path: '/tmp/y.txt' })).toBe('/tmp/y.txt');
		expect(summarizeToolInput('Edit', { file_path: '/tmp/z.svelte' })).toBe('/tmp/z.svelte');
	});

	it('combines pattern + path for Grep', () => {
		expect(summarizeToolInput('Grep', { pattern: 'TODO', path: 'src/' })).toBe('TODO  in  src/');
		expect(summarizeToolInput('Grep', { pattern: 'TODO' })).toBe('TODO');
	});

	it('returns the url for WebFetch', () => {
		expect(summarizeToolInput('WebFetch', { url: 'https://example.com' })).toBe(
			'https://example.com'
		);
	});

	it('combines subagent_type + description for Task / Agent', () => {
		expect(
			summarizeToolInput('Task', { subagent_type: 'researcher', description: 'gather sources' })
		).toBe('researcher · gather sources');
	});

	it('falls back to first non-empty string for unknown tools', () => {
		expect(summarizeToolInput('SomeRandomTool', { foo: 'hello', bar: 42 })).toBe('foo: hello');
	});

	it('falls back to scalar fields when no string fields present', () => {
		expect(summarizeToolInput('SomeRandomTool', { count: 7 })).toBe('count: 7');
	});

	it('falls back to a key list for object-only inputs', () => {
		expect(summarizeToolInput('SomeRandomTool', { meta: { a: 1 }, body: { b: 2 } })).toBe(
			'[meta, body]'
		);
	});

	it('returns empty string for null / undefined input', () => {
		expect(summarizeToolInput('Bash', null)).toBe('');
		expect(summarizeToolInput('Bash', undefined)).toBe('');
	});
});

describe('userMessageInlinePreview', () => {
	it('returns the bash command for a single tool_result', () => {
		const blocks: ContentBlock[] = [
			{
				kind: 'tool_result',
				tool_use_id: 'toolu_1',
				text: 'output here',
				is_error: false,
				tool_name: 'Bash',
				tool_input: { command: 'git status' }
			}
		];
		expect(userMessageInlinePreview(blocks)).toBe('git status');
	});

	it('joins multiple tool_result previews with " · "', () => {
		const blocks: ContentBlock[] = [
			{
				kind: 'tool_result',
				tool_use_id: 'toolu_1',
				text: 'output A',
				is_error: false,
				tool_name: 'Bash',
				tool_input: { command: 'git status' }
			},
			{
				kind: 'tool_result',
				tool_use_id: 'toolu_2',
				text: 'output B',
				is_error: false,
				tool_name: 'Read',
				tool_input: { file_path: '/tmp/x.log' }
			}
		];
		expect(userMessageInlinePreview(blocks)).toBe('git status  ·  /tmp/x.log');
	});

	it('returns empty string when message has no tool_results', () => {
		const blocks: ContentBlock[] = [{ kind: 'text', text: 'hi' }];
		expect(userMessageInlinePreview(blocks)).toBe('');
	});

	it('skips tool_results that have no resolvable input', () => {
		const blocks: ContentBlock[] = [
			{
				kind: 'tool_result',
				tool_use_id: 'toolu_orphan',
				text: 'output',
				is_error: false,
				tool_name: 'Bash'
			}
		];
		expect(userMessageInlinePreview(blocks)).toBe('');
	});
});

describe('walkRequestMessages tool_input propagation', () => {
	it('attaches tool_use input to the matching tool_result block', () => {
		const body = {
			messages: [
				{
					role: 'assistant',
					content: [{ type: 'tool_use', id: 'toolu_1', name: 'Bash', input: { command: 'ls -la' } }]
				},
				{
					role: 'user',
					content: [{ type: 'tool_result', tool_use_id: 'toolu_1', content: 'total 8\n...' }]
				}
			]
		};
		const messages = walkRequestMessages(body);
		const result = messages[1]?.blocks[0];
		expect(result?.kind).toBe('tool_result');
		if (result?.kind === 'tool_result') {
			expect(result.tool_name).toBe('Bash');
			expect(result.tool_input).toEqual({ command: 'ls -la' });
		}
	});

	it('leaves tool_input undefined when no matching tool_use was seen', () => {
		const body = {
			messages: [
				{
					role: 'user',
					content: [{ type: 'tool_result', tool_use_id: 'toolu_orphan', content: 'output' }]
				}
			]
		};
		const messages = walkRequestMessages(body);
		const result = messages[0]?.blocks[0];
		expect(result?.kind).toBe('tool_result');
		if (result?.kind === 'tool_result') {
			expect(result.tool_input).toBeUndefined();
		}
	});
});

describe('extractTrailingUserTurn', () => {
	it('returns all user messages when the body has no assistant turn (session opener)', () => {
		const body = {
			messages: [
				{
					role: 'user',
					content: [
						{ type: 'text', text: '<system-reminder>boilerplate</system-reminder>' },
						{ type: 'text', text: 'can you format this sql for me' }
					]
				}
			]
		};
		const tail = extractTrailingUserTurn(body);
		expect(tail).toHaveLength(1);
		expect(tail[0]?.role).toBe('user');
		const proseBlock = tail[0]?.blocks.find(
			(b) => b.kind === 'text' && b.text === 'can you format this sql for me'
		);
		expect(proseBlock).toBeDefined();
	});

	it('returns only the turn AFTER the last assistant message — not the first turn', () => {
		const body = {
			messages: [
				{ role: 'user', content: [{ type: 'text', text: 'first operator message' }] },
				{ role: 'assistant', content: [{ type: 'text', text: 'first reply' }] },
				{ role: 'user', content: [{ type: 'text', text: 'second operator message' }] },
				{ role: 'assistant', content: [{ type: 'text', text: 'second reply' }] },
				{ role: 'user', content: [{ type: 'text', text: 'THIS bands new message' }] }
			]
		};
		const tail = extractTrailingUserTurn(body);
		expect(tail).toHaveLength(1);
		const texts = tail.flatMap((m) => m.blocks.flatMap((b) => (b.kind === 'text' ? [b.text] : [])));
		expect(texts).toContain('THIS bands new message');
		expect(texts).not.toContain('first operator message');
		expect(texts).not.toContain('second operator message');
	});

	it('returns the tool_result envelope for a harness tool-loop turn', () => {
		const body = {
			messages: [
				{ role: 'user', content: [{ type: 'text', text: 'do the thing' }] },
				{
					role: 'assistant',
					content: [{ type: 'tool_use', id: 'toolu_1', name: 'Bash', input: { command: 'ls' } }]
				},
				{
					role: 'user',
					content: [{ type: 'tool_result', tool_use_id: 'toolu_1', content: 'file.txt' }]
				}
			]
		};
		const tail = extractTrailingUserTurn(body);
		expect(tail).toHaveLength(1);
		expect(tail[0]?.blocks[0]?.kind).toBe('tool_result');
	});

	it('returns an empty array for an empty / malformed body', () => {
		expect(extractTrailingUserTurn({})).toEqual([]);
		expect(extractTrailingUserTurn(null)).toEqual([]);
		expect(extractTrailingUserTurn({ messages: [] })).toEqual([]);
	});

	it('returns empty when the trailing turn is assistant-only', () => {
		const body = {
			messages: [
				{ role: 'user', content: [{ type: 'text', text: 'hi' }] },
				{ role: 'assistant', content: [{ type: 'text', text: 'reply' }] }
			]
		};
		expect(extractTrailingUserTurn(body)).toEqual([]);
	});
});

import { walkMessageDelta } from './messageWalker';

function textMessage(role: 'user' | 'assistant', text: string): unknown {
	return { role, content: [{ type: 'text', text }] };
}

describe('walkMessageDelta — shape classification', () => {
	it('returns first-turn when prev is null', () => {
		const curr = {
			model: 'claude-3-5-sonnet',
			messages: [textMessage('user', 'hi')]
		};
		const delta = walkMessageDelta(null, curr);
		expect(delta.shape).toBe('first-turn');
		expect(delta.messages_added).toEqual([]);
	});

	it('returns first-turn when prev is undefined', () => {
		const curr = { model: 'x', messages: [] };
		expect(walkMessageDelta(undefined, curr).shape).toBe('first-turn');
	});

	it('returns lineage-broken when curr is null', () => {
		expect(walkMessageDelta({ messages: [] }, null).shape).toBe('lineage-broken');
	});

	it('tail-append — prev[0..2] is a prefix of curr[0..4]', () => {
		const prev = {
			model: 'm',
			messages: [
				textMessage('user', 'one'),
				textMessage('assistant', 'reply one'),
				textMessage('user', 'two')
			]
		};
		const curr = {
			model: 'm',
			messages: [
				textMessage('user', 'one'),
				textMessage('assistant', 'reply one'),
				textMessage('user', 'two'),
				textMessage('assistant', 'reply two'),
				textMessage('user', 'three')
			]
		};
		const delta = walkMessageDelta(prev, curr);
		expect(delta.shape).toBe('tail-append');
		expect(delta.messages_added).toHaveLength(2);
		expect(delta.messages_added[0]?.role).toBe('assistant');
		expect(delta.messages_added[0]?.blocks[0]).toEqual({ kind: 'text', text: 'reply two' });
		expect(delta.messages_added[1]?.role).toBe('user');
		expect(delta.messages_added[1]?.blocks[0]).toEqual({ kind: 'text', text: 'three' });
	});

	it('compaction — prev has many messages, curr has few, nothing in common', () => {
		const prev = {
			messages: [
				textMessage('user', 'a'),
				textMessage('assistant', 'b'),
				textMessage('user', 'c'),
				textMessage('assistant', 'd'),
				textMessage('user', 'e')
			]
		};
		const curr = { messages: [textMessage('user', 'fresh start')] };
		expect(walkMessageDelta(prev, curr).shape).toBe('compaction');
	});

	it('lineage-broken — same length, divergent at message[0]', () => {
		const prev = { messages: [textMessage('user', 'old root')] };
		const curr = { messages: [textMessage('user', 'new root')] };
		expect(walkMessageDelta(prev, curr).shape).toBe('lineage-broken');
	});

	it('lineage-broken — partial-divergence after shared prefix', () => {
		const prev = {
			messages: [
				textMessage('user', 'one'),
				textMessage('assistant', 'reply one'),
				textMessage('user', 'two-original')
			]
		};
		const curr = {
			messages: [
				textMessage('user', 'one'),
				textMessage('assistant', 'reply one'),
				textMessage('user', 'two-edited'),
				textMessage('assistant', 'reply two')
			]
		};
		expect(walkMessageDelta(prev, curr).shape).toBe('lineage-broken');
	});
});

describe('walkMessageDelta — chrome stripping', () => {
	it('cache_control on the LAST user message floats — prefix still matches', () => {
		const m1 = textMessage('user', 'one');
		const m2Prev = {
			role: 'user',
			content: [{ type: 'text', text: 'two' }],
			cache_control: { type: 'ephemeral' }
		};
		const m2Curr = textMessage('user', 'two');
		const m3 = textMessage('assistant', 'reply two');
		const m4 = {
			role: 'user',
			content: [{ type: 'text', text: 'three' }],
			cache_control: { type: 'ephemeral' }
		};
		const prev = { messages: [m1, m2Prev, m3] };
		const curr = { messages: [m1, m2Curr, m3, m4] };
		const delta = walkMessageDelta(prev, curr);
		expect(delta.shape).toBe('tail-append');
		expect(delta.messages_added).toHaveLength(1);
		expect(delta.messages_added[0]?.role).toBe('user');
		expect(delta.messages_added[0]?.blocks[0]).toEqual({ kind: 'text', text: 'three' });
	});

	it('systemChanged stays false when only the billing-header block differs', () => {
		const prev = {
			messages: [textMessage('user', 'x')],
			system: [
				{ type: 'text', text: 'x-anthropic-billing-header:abc' },
				{ type: 'text', text: 'You are Claude Code' }
			]
		};
		const curr = {
			messages: [textMessage('user', 'x')],
			system: [
				{ type: 'text', text: 'x-anthropic-billing-header:def' },
				{ type: 'text', text: 'You are Claude Code' }
			]
		};
		const delta = walkMessageDelta(prev, curr);
		expect(delta.systemChanged).toBe(false);
	});

	it('systemChanged true when persona block changes', () => {
		const prev = {
			messages: [textMessage('user', 'x')],
			system: [{ type: 'text', text: 'You are Claude Code' }]
		};
		const curr = {
			messages: [textMessage('user', 'x')],
			system: [{ type: 'text', text: 'You are a frontend specialist' }]
		};
		expect(walkMessageDelta(prev, curr).systemChanged).toBe(true);
	});
});

describe('walkMessageDelta — fan-out flags', () => {
	it('modelChanged true when model differs', () => {
		const base = { messages: [textMessage('user', 'x')] };
		const prev = { ...base, model: 'claude-3-5-sonnet' };
		const curr = { ...base, model: 'claude-opus-4-7' };
		expect(walkMessageDelta(prev, curr).modelChanged).toBe(true);
	});

	it('toolsChanged true when tool count differs', () => {
		const prev = {
			messages: [textMessage('user', 'x')],
			tools: [{ name: 'a' }, { name: 'b' }]
		};
		const curr = {
			messages: [textMessage('user', 'x')],
			tools: [{ name: 'a' }, { name: 'b' }, { name: 'c' }]
		};
		expect(walkMessageDelta(prev, curr).toolsChanged).toBe(true);
	});

	it('paramsChanged true when max_tokens differs', () => {
		const prev = {
			messages: [textMessage('user', 'x')],
			max_tokens: 4096
		};
		const curr = {
			messages: [textMessage('user', 'x')],
			max_tokens: 8192
		};
		expect(walkMessageDelta(prev, curr).paramsChanged).toBe(true);
	});

	it('all flags false on a clean tail-append', () => {
		const prev = {
			model: 'm',
			max_tokens: 4096,
			messages: [textMessage('user', 'one')]
		};
		const curr = {
			model: 'm',
			max_tokens: 4096,
			messages: [textMessage('user', 'one'), textMessage('assistant', 'reply')]
		};
		const delta = walkMessageDelta(prev, curr);
		expect(delta.shape).toBe('tail-append');
		expect(delta.systemChanged).toBe(false);
		expect(delta.toolsChanged).toBe(false);
		expect(delta.modelChanged).toBe(false);
		expect(delta.paramsChanged).toBe(false);
	});
});

describe('walkMessageDelta — degenerate inputs', () => {
	it('both prev and curr empty', () => {
		const delta = walkMessageDelta({ messages: [] }, { messages: [] });
		expect(delta.shape).toBe('tail-append');
		expect(delta.messages_added).toHaveLength(0);
	});

	it('curr has no messages array', () => {
		const delta = walkMessageDelta({ messages: [] }, { foo: 'bar' });
		expect(delta.shape).toBe('tail-append');
		expect(delta.messages_added).toHaveLength(0);
	});

	it('prev is a non-record (number)', () => {
		expect(walkMessageDelta(42, { messages: [] }).shape).toBe('first-turn');
	});
});

import { deepEqual, stripBillingHeader, stripCacheControl } from './stripChrome';

describe('stripCacheControl', () => {
	it('passes primitives through unchanged', () => {
		expect(stripCacheControl(42)).toBe(42);
		expect(stripCacheControl('hello')).toBe('hello');
		expect(stripCacheControl(null)).toBe(null);
		expect(stripCacheControl(true)).toBe(true);
	});

	it('removes top-level cache_control key', () => {
		const input = { role: 'user', cache_control: { type: 'ephemeral' }, content: 'x' };
		expect(stripCacheControl(input)).toEqual({ role: 'user', content: 'x' });
	});

	it('recurses into arrays', () => {
		const input = [
			{ a: 1, cache_control: { type: 'ephemeral' } },
			{ b: 2, cache_control: null }
		];
		expect(stripCacheControl(input)).toEqual([{ a: 1 }, { b: 2 }]);
	});

	it('recurses into nested objects (e.g. content[*] blocks)', () => {
		const input = {
			role: 'user',
			content: [
				{
					type: 'text',
					text: 'hello',
					cache_control: { type: 'ephemeral' }
				},
				{
					type: 'tool_result',
					tool_use_id: 'toolu_x',
					content: 'result',
					cache_control: { type: 'ephemeral' }
				}
			]
		};
		expect(stripCacheControl(input)).toEqual({
			role: 'user',
			content: [
				{ type: 'text', text: 'hello' },
				{ type: 'tool_result', tool_use_id: 'toolu_x', content: 'result' }
			]
		});
	});

	it('does NOT mutate the input', () => {
		const input = {
			role: 'user',
			cache_control: { type: 'ephemeral' },
			content: 'x'
		};
		stripCacheControl(input);
		expect(input).toEqual({
			role: 'user',
			cache_control: { type: 'ephemeral' },
			content: 'x'
		});
	});
});

describe('stripBillingHeader', () => {
	it('returns non-array system unchanged', () => {
		expect(stripBillingHeader('plain string system')).toBe('plain string system');
		expect(stripBillingHeader(null)).toBe(null);
		expect(stripBillingHeader(undefined)).toBe(undefined);
	});

	it('drops the billing-header block, preserves persona + project blocks', () => {
		const input = [
			{ type: 'text', text: 'x-anthropic-billing-header:abc123' },
			{ type: 'text', text: 'You are Claude Code…' },
			{ type: 'text', text: 'Project state: feature/foo' }
		];
		expect(stripBillingHeader(input)).toEqual([
			{ type: 'text', text: 'You are Claude Code…' },
			{ type: 'text', text: 'Project state: feature/foo' }
		]);
	});

	it('preserves blocks with no text field', () => {
		const input = [
			{ type: 'text', text: 'x-anthropic-billing-header:abc' },
			{ type: 'image', source: { type: 'base64', data: '...' } }
		];
		expect(stripBillingHeader(input)).toEqual([
			{ type: 'image', source: { type: 'base64', data: '...' } }
		]);
	});

	it('keeps non-matching billing-like prefixes', () => {
		const input = [
			{ type: 'text', text: 'x-anthropic-billing-header:abc' },
			{ type: 'text', text: 'X-Anthropic-Billing-Header:abc' },
			{ type: 'text', text: 'see x-anthropic-billing-header: inline' }
		];
		const out = stripBillingHeader(input) as Array<{ type: string; text: string }>;
		expect(out).toHaveLength(2);
		expect(out[0]!.text).toBe('X-Anthropic-Billing-Header:abc');
		expect(out[1]!.text).toBe('see x-anthropic-billing-header: inline');
	});
});

describe('deepEqual', () => {
	it('identity short-circuits', () => {
		const a = { x: 1 };
		expect(deepEqual(a, a)).toBe(true);
	});

	it('compares primitives', () => {
		expect(deepEqual(1, 1)).toBe(true);
		expect(deepEqual(1, '1')).toBe(false);
		expect(deepEqual(null, null)).toBe(true);
		expect(deepEqual(null, undefined)).toBe(false);
	});

	it('compares arrays positionally', () => {
		expect(deepEqual([1, 2, 3], [1, 2, 3])).toBe(true);
		expect(deepEqual([1, 2, 3], [3, 2, 1])).toBe(false);
		expect(deepEqual([1, 2], [1, 2, 3])).toBe(false);
	});

	it('compares objects key-as-set (order-insensitive)', () => {
		expect(deepEqual({ a: 1, b: 2 }, { b: 2, a: 1 })).toBe(true);
		expect(deepEqual({ a: 1 }, { a: 1, b: 2 })).toBe(false);
	});

	it('recurses into nested arrays + objects', () => {
		expect(
			deepEqual(
				{ role: 'user', content: [{ type: 'text', text: 'x' }] },
				{ role: 'user', content: [{ type: 'text', text: 'x' }] }
			)
		).toBe(true);
		expect(
			deepEqual(
				{ role: 'user', content: [{ type: 'text', text: 'x' }] },
				{ role: 'user', content: [{ type: 'text', text: 'y' }] }
			)
		).toBe(false);
	});

	it('rejects array vs object even with similar shape', () => {
		expect(deepEqual([1, 2], { 0: 1, 1: 2 })).toBe(false);
	});
});

import { renderBody, walkContentBlock } from './MessageContentRenderer.svelte';

describe('renderBody', () => {
	it('walks a request body with one user text message into the Rendered shape', () => {
		const body = {
			model: 'claude-sonnet-4-5',
			messages: [
				{
					role: 'user',
					content: [
						{
							type: 'text',
							text: 'Line one\nLine two\nLine three'
						}
					]
				}
			]
		};

		const result = renderBody(body);
		expect(result.shape).toBe('request');
		if (result.shape !== 'request') throw new Error('narrowing');
		expect(result.messages).toHaveLength(1);
		const first = result.messages[0];
		expect(first?.role).toBe('user');
		expect(first?.blocks).toHaveLength(1);
		const block = first?.blocks[0];
		expect(block?.kind).toBe('text');
		if (block?.kind !== 'text') throw new Error('narrowing');
		expect(block.text).toBe('Line one\nLine two\nLine three');
		expect(block.text).toContain('\n');
		expect(block.text).not.toContain('\\n');
	});

	it('walks a request body where content is a plain string (legacy shape)', () => {
		const body = {
			messages: [{ role: 'user', content: 'What is the capital of Ukraine?' }]
		};
		const result = renderBody(body);
		expect(result.shape).toBe('request');
		if (result.shape !== 'request') throw new Error('narrowing');
		const first = result.messages[0];
		expect(first?.blocks[0]).toEqual({
			kind: 'text',
			text: 'What is the capital of Ukraine?'
		});
	});

	it('walks a response body with text + tool_use blocks as a flat block list', () => {
		const body = {
			id: 'msg_01',
			type: 'message',
			role: 'assistant',
			content: [
				{ type: 'text', text: 'Reading the file now.' },
				{
					type: 'tool_use',
					id: 'toolu_01',
					name: 'Read',
					input: { file_path: '/tmp/foo.txt' }
				},
				{ type: 'text', text: 'Done.' }
			]
		};

		const result = renderBody(body);
		expect(result.shape).toBe('response');
		if (result.shape !== 'response') throw new Error('narrowing');
		expect(result.blocks).toHaveLength(3);

		const [textA, toolUse, textB] = result.blocks;
		expect(textA?.kind).toBe('text');
		if (textA?.kind !== 'text') throw new Error('narrowing');
		expect(textA.text).toBe('Reading the file now.');

		expect(toolUse?.kind).toBe('tool_use');
		if (toolUse?.kind !== 'tool_use') throw new Error('narrowing');
		expect(toolUse.id).toBe('toolu_01');
		expect(toolUse.name).toBe('Read');
		expect(toolUse.input).toEqual({ file_path: '/tmp/foo.txt' });

		expect(textB?.kind).toBe('text');
	});

	it('walks a streamed response body with thinking + text + tool_use blocks', () => {
		const body = {
			id: 'msg_02',
			type: 'message',
			role: 'assistant',
			content: [
				{
					type: 'thinking',
					thinking: 'Let me reason about this first.',
					signature: 'sig_abc123'
				},
				{ type: 'text', text: 'Here is the answer.' },
				{
					type: 'tool_use',
					id: 'toolu_02',
					name: 'Edit',
					input: { file_path: '/tmp/x.ts' }
				}
			]
		};

		const result = renderBody(body);
		expect(result.shape).toBe('response');
		if (result.shape !== 'response') throw new Error('narrowing');
		expect(result.blocks).toHaveLength(3);

		const [thinking, text, toolUse] = result.blocks;
		expect(thinking?.kind).toBe('thinking');
		if (thinking?.kind !== 'thinking') throw new Error('narrowing');
		expect(thinking.text).toBe('Let me reason about this first.');
		expect(thinking.signature).toBe('sig_abc123');

		expect(text?.kind).toBe('text');
		expect(toolUse?.kind).toBe('tool_use');
	});

	it('walks a thinking block with no signature into a null-signature thinking block', () => {
		const block = walkContentBlock({
			type: 'thinking',
			thinking: 'Reasoning without a signature.'
		});
		expect(block.kind).toBe('thinking');
		if (block.kind !== 'thinking') throw new Error('narrowing');
		expect(block.text).toBe('Reasoning without a signature.');
		expect(block.signature).toBeNull();
	});

	it('hides the Rendered tab when the JSON body is not a messages structure', () => {
		const notMessages = {
			oauth_endpoint: 'https://example.com/authorize',
			scopes: ['read', 'write'],
			client_id: 'abc'
		};
		expect(renderBody(notMessages).shape).toBe('unknown');

		expect(renderBody({ messages: [] }).shape).toBe('unknown');
		expect(renderBody({ content: [] }).shape).toBe('unknown');

		expect(renderBody(['some', 'array']).shape).toBe('unknown');
		expect(renderBody('string').shape).toBe('unknown');
		expect(renderBody(null).shape).toBe('unknown');
	});

	it('rejects a `content` array whose entries are not block-shaped', () => {
		const odd = { content: [{ body: 'no type field' }, { body: 'ditto' }] };
		expect(renderBody(odd).shape).toBe('unknown');
	});

	it('walks tool_result blocks with an array content (inner text extracted)', () => {
		const body = {
			messages: [
				{
					role: 'user',
					content: [
						{
							type: 'tool_result',
							tool_use_id: 'toolu_01',
							content: [
								{ type: 'text', text: 'First line of output' },
								{ type: 'text', text: 'Second line' }
							]
						}
					]
				}
			]
		};
		const result = renderBody(body);
		expect(result.shape).toBe('request');
		if (result.shape !== 'request') throw new Error('narrowing');
		const block = result.messages[0]?.blocks[0];
		expect(block?.kind).toBe('tool_result');
		if (block?.kind !== 'tool_result') throw new Error('narrowing');
		expect(block.tool_use_id).toBe('toolu_01');
		expect(block.blocks).toHaveLength(2);
		expect(block.blocks[0]).toEqual({ kind: 'text', text: 'First line of output' });
		expect(block.blocks[1]).toEqual({ kind: 'text', text: 'Second line' });
	});

	it('renders a bare `system` array of content blocks as a response shape', () => {
		const systemArray = [
			{ type: 'text', text: 'x-anthropic-billing-header: cc_version=1' },
			{ type: 'text', text: 'You are Claude Code.' },
			{ type: 'text', text: 'Body', cache_control: { type: 'ephemeral' } }
		];
		const result = renderBody(systemArray);
		expect(result.shape).toBe('response');
		if (result.shape !== 'response') throw new Error('narrowing');
		expect(result.blocks).toHaveLength(3);
		expect(result.blocks[2]).toEqual({ kind: 'text', text: 'Body' });
	});

	it('renders a single content-block object (one `system` block) as a response shape', () => {
		const oneBlock = { type: 'text', text: 'Body', cache_control: { scope: 'global' } };
		const result = renderBody(oneBlock);
		expect(result.shape).toBe('response');
		if (result.shape !== 'response') throw new Error('narrowing');
		expect(result.blocks).toHaveLength(1);
		expect(result.blocks[0]).toEqual({ kind: 'text', text: 'Body' });
	});

	it('leaves a lone record with an UNKNOWN type as `unknown` (no Rendered hijack)', () => {
		expect(renderBody({ type: 'message', role: 'assistant' }).shape).toBe('unknown');
	});
});

describe('walkContentBlock', () => {
	it('maps an image block to `{ kind: "image" }` without crashing on the source field', () => {
		const block = { type: 'image', source: { type: 'base64', data: 'AAAA' } };
		expect(walkContentBlock(block)).toEqual({ kind: 'image' });
	});

	it('maps an unknown block type to `{ kind: "unknown", type: <t> }`', () => {
		const block = { type: 'weird_block', weirdness: 42 };
		expect(walkContentBlock(block)).toEqual({
			kind: 'unknown',
			type: 'weird_block'
		});
	});
});
