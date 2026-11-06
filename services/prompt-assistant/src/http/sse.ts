import type { ServerResponse } from 'node:http';
import type { SseEvent } from '../types.js';

export function startSse(res: ServerResponse): void {
	res.writeHead(200, {
		'content-type': 'text/event-stream',
		'cache-control': 'no-cache',
		connection: 'keep-alive',
		'x-accel-buffering': 'no'
	});
}

/** Frames one SSE event exactly as `event: <name>\ndata: <json>\n\n`. */
export function writeSse(res: ServerResponse, event: SseEvent): void {
	res.write(`event: ${event.event}\ndata: ${JSON.stringify(event.data)}\n\n`);
}
