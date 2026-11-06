#!/usr/bin/env node
// Opt-in end-to-end check: exercises a REAL running prompt-assistant service
// (health -> create conversation -> one real streamed message) against a live
// Claude Agent SDK subprocess and a live mike inspect API. It costs real
// money and needs a service already running, so it does nothing unless
// PROMPT_ASSISTANT_E2E=1 is set explicitly — `pnpm run e2e` on its own, in a
// normal test run or CI, is a no-op.
//
// Usage (see README "Tests" section):
//   PROMPT_ASSISTANT_E2E=1 \
//   PROMPT_ASSISTANT_E2E_URL=http://127.0.0.1:9975 \
//   PROMPT_ASSISTANT_E2E_REQUEST_ID=421514 \
//     pnpm run e2e
//
// This is exactly the flow that was run by hand during development against a
// temporary instance on :9975 with a haiku model override — never against the
// real :9985 service a developer/operator may have running.

interface SseFrame {
	event: string;
	data: unknown;
}

/** Splits an accumulated SSE buffer on blank lines, returning parsed frames plus the unparsed remainder. */
function parseSseChunk(buffer: string): { frames: SseFrame[]; rest: string } {
	const frames: SseFrame[] = [];
	const parts = buffer.split('\n\n');
	const rest = parts.pop() ?? '';
	for (const part of parts) {
		if (part.trim() === '') continue;
		let eventName = 'message';
		let dataLine = '';
		for (const line of part.split('\n')) {
			if (line.startsWith('event:')) eventName = line.slice('event:'.length).trim();
			else if (line.startsWith('data:')) dataLine = line.slice('data:'.length).trim();
		}
		frames.push({ event: eventName, data: dataLine.length > 0 ? JSON.parse(dataLine) : {} });
	}
	return { frames, rest };
}

function assert(cond: boolean, message: string): asserts cond {
	if (!cond) throw new Error(`assertion failed: ${message}`);
}

async function main(): Promise<void> {
	if (process.env.PROMPT_ASSISTANT_E2E !== '1') {
		console.log('[e2e] skipped — set PROMPT_ASSISTANT_E2E=1 to run against a real, already-running service.');
		return;
	}

	const baseUrl = (process.env.PROMPT_ASSISTANT_E2E_URL ?? 'http://127.0.0.1:9985').replace(/\/+$/, '');
	const requestId = Number.parseInt(process.env.PROMPT_ASSISTANT_E2E_REQUEST_ID ?? '421514', 10);
	const question =
		process.env.PROMPT_ASSISTANT_E2E_QUESTION ??
		'In one sentence, what does the Harness section of this prompt say?';

	console.log(`[e2e] target: ${baseUrl}`);
	console.log(`[e2e] request_id: ${requestId}`);

	// 1. health
	const healthRes = await fetch(`${baseUrl}/api/assistant/health`);
	assert(healthRes.ok, `health check failed: HTTP ${healthRes.status}`);
	const health = (await healthRes.json()) as { ok: boolean; enabled: boolean; model: string };
	console.log('[e2e] health:', JSON.stringify(health));
	assert(health.ok === true, 'health.ok must be true');
	assert(health.enabled === true, 'health.enabled must be true (the prompt_assistant toggle must be on)');

	// 2. create a conversation
	const createRes = await fetch(`${baseUrl}/api/assistant/conversations`, {
		method: 'POST',
		headers: { 'content-type': 'application/json' },
		body: JSON.stringify({ request_id: requestId })
	});
	assert(createRes.ok, `create conversation failed: HTTP ${createRes.status}`);
	const { id: conversationId } = (await createRes.json()) as { id: string };
	console.log(`[e2e] conversation: ${conversationId}`);
	assert(typeof conversationId === 'string' && conversationId.length > 0, 'conversation id must be a non-empty string');

	// 3. fetch it back — should be empty
	const getRes = await fetch(`${baseUrl}/api/assistant/conversations/${conversationId}`);
	assert(getRes.ok, `get conversation failed: HTTP ${getRes.status}`);
	const conversation = (await getRes.json()) as { request_id: number; messages: unknown[] };
	assert(conversation.request_id === requestId, 'conversation.request_id must round-trip');
	assert(conversation.messages.length === 0, 'a fresh conversation must have no messages');

	// 4. one real message, streamed
	const msgRes = await fetch(`${baseUrl}/api/assistant/conversations/${conversationId}/messages`, {
		method: 'POST',
		headers: { 'content-type': 'application/json' },
		body: JSON.stringify({ text: question })
	});
	assert(msgRes.ok, `message request failed: HTTP ${msgRes.status}`);
	assert(msgRes.body !== null, 'message response must have a body');
	assert(
		(msgRes.headers.get('content-type') ?? '').includes('text/event-stream'),
		'message response content-type must be text/event-stream'
	);

	const counts: Record<string, number> = {};
	let assembledText = '';
	let costUsd: number | null = null;
	let buffer = '';
	const decoder = new TextDecoder();
	for await (const chunk of msgRes.body as unknown as AsyncIterable<Uint8Array>) {
		buffer += decoder.decode(chunk, { stream: true });
		const { frames, rest } = parseSseChunk(buffer);
		buffer = rest;
		for (const frame of frames) {
			counts[frame.event] = (counts[frame.event] ?? 0) + 1;
			console.log(`[e2e] event: ${frame.event} data: ${JSON.stringify(frame.data)}`);
			if (frame.event === 'text') assembledText += (frame.data as { delta: string }).delta;
			if (frame.event === 'done') costUsd = (frame.data as { cost_usd: number }).cost_usd;
			if (frame.event === 'error') throw new Error(`assistant turn errored: ${JSON.stringify(frame.data)}`);
		}
	}

	console.log(`[e2e] assembled text: ${assembledText}`);
	console.log(`[e2e] event counts: ${JSON.stringify(counts)}`);

	assert((counts.text ?? 0) > 0, 'expected at least one text event');
	assert((counts.done ?? 0) === 1, 'expected exactly one done event');
	assert((counts.error ?? 0) === 0, 'expected no error events on a clean run');
	assert(costUsd !== null && costUsd >= 0, 'the done event must carry a numeric cost_usd');
	assert(assembledText.trim().length > 0, 'the assembled assistant text must be non-empty');

	// 5. the conversation now has both turns, persisted
	const finalRes = await fetch(`${baseUrl}/api/assistant/conversations/${conversationId}`);
	assert(finalRes.ok, `final conversation fetch failed: HTTP ${finalRes.status}`);
	const final = (await finalRes.json()) as { messages: Array<{ role: string }> };
	assert(final.messages.length === 2, 'conversation should have a user + assistant message after the turn');
	assert(
		final.messages[0]?.role === 'user' && final.messages[1]?.role === 'assistant',
		'message roles must be user then assistant, in order'
	);

	console.log(`[e2e] PASS — cost_usd=${costUsd}`);
}

main().catch((err) => {
	console.error('[e2e] FAIL:', (err as Error).message);
	process.exitCode = 1;
});
