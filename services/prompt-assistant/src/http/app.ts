import type { IncomingMessage, RequestListener, ServerResponse } from 'node:http';
import {
	handleCreateConversation,
	handleCodexStatus,
	handleGetConversation,
	handleHealth,
	handlePostMessage,
	type AppDeps
} from './handlers.js';
import { isValidConversationId, readJsonBody, sendJson } from './util.js';

const CONVERSATION_PATH = /^\/api\/assistant\/conversations\/([^/]+)$/;
const MESSAGES_PATH = /^\/api\/assistant\/conversations\/([^/]+)\/messages$/;

export function createRequestListener(deps: AppDeps): RequestListener {
	return async (req: IncomingMessage, res: ServerResponse) => {
		try {
			await route(deps, req, res);
		} catch (err) {
			if (!res.headersSent) {
				sendJson(res, 500, { error: (err as Error).message });
			} else {
				res.end();
			}
		}
	};
}

async function route(deps: AppDeps, req: IncomingMessage, res: ServerResponse): Promise<void> {
	const url = new URL(req.url ?? '/', 'http://internal');
	const method = req.method ?? 'GET';

	if (method === 'GET' && url.pathname === '/api/assistant/health') {
		sendJson(res, 200, await handleHealth(deps));
		return;
	}

	if (method === 'GET' && url.pathname === '/api/assistant/integrations/codex') {
		sendJson(res, 200, await handleCodexStatus(deps));
		return;
	}

	if (method === 'POST' && url.pathname === '/api/assistant/conversations') {
		const body = await readJsonBody(req);
		const result = await handleCreateConversation(deps, body);
		sendJson(res, result.status, result.body);
		return;
	}

	const conversationMatch = url.pathname.match(CONVERSATION_PATH);
	if (method === 'GET' && conversationMatch) {
		const id = decodeURIComponent(conversationMatch[1] as string);
		if (!isValidConversationId(id)) {
			sendJson(res, 400, { error: 'conversation id must be a UUID' });
			return;
		}
		const result = await handleGetConversation(deps, id);
		sendJson(res, result.status, result.body);
		return;
	}

	const messagesMatch = url.pathname.match(MESSAGES_PATH);
	if (method === 'POST' && messagesMatch) {
		const id = decodeURIComponent(messagesMatch[1] as string);
		if (!isValidConversationId(id)) {
			sendJson(res, 400, { error: 'conversation id must be a UUID' });
			return;
		}
		const body = await readJsonBody(req);
		await handlePostMessage(deps, id, body, res);
		return;
	}

	sendJson(res, 404, { error: 'not found' });
}
