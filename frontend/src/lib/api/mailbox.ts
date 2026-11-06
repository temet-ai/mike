import { ApiError } from '$lib/api';

export type MailboxStatus = 'pending' | 'claimed' | 'delivered' | 'consumed' | 'dead_lettered';

export interface MailboxMessage {
	message_uuid: string;
	from_session_uuid: string | null;
	to_session_uuid: string;
	body: string;
	queued_at: string;
	claimed_at: string | null;
	sent_request_id: number | null;
	sent_at: string | null;
	consumed_at: string | null;
	dead_lettered_at: string | null;
	channel_delivered_at?: string | null;
	delivery_attempts: number;
	status: MailboxStatus;
}

export interface MailboxEnvelope {
	session_id: string;
	inbox: MailboxMessage[];
	outbox: MailboxMessage[];
}

export interface SendMailboxMessageBody {
	from_session_id?: string;
	message: string;
}

export interface SendMailboxMessageResponse {
	message_uuid: string;
	to_session_uuid: string;
	from_session_uuid: string | null;
	queued_at: string;
}

export async function getSessionMessages(sessionId: string): Promise<MailboxEnvelope> {
	const path = `/api/sessions/${encodeURIComponent(sessionId)}/messages`;
	const res = await fetch(path, { headers: { accept: 'application/json' } });
	if (res.status === 404) {
		return { session_id: sessionId, inbox: [], outbox: [] };
	}
	if (!res.ok) {
		throw new ApiError(`GET ${path} failed: ${res.status} ${res.statusText}`, res.status, path);
	}
	return (await res.json()) as MailboxEnvelope;
}

export async function sendSessionMessage(
	toSessionId: string,
	body: SendMailboxMessageBody
): Promise<SendMailboxMessageResponse> {
	const path = `/api/sessions/${encodeURIComponent(toSessionId)}/message`;
	const res = await fetch(path, {
		method: 'POST',
		headers: { 'content-type': 'application/json', accept: 'application/json' },
		body: JSON.stringify(body)
	});
	if (!res.ok) {
		throw new ApiError(`POST ${path} failed: ${res.status} ${res.statusText}`, res.status, path);
	}
	return (await res.json()) as SendMailboxMessageResponse;
}

export interface AckMailboxMessageResponse {
	message_uuid: string;
	consumed_at: string;
}

interface MailboxErrorBody {
	error: string;
	message: string;
}

const ACK_ERROR_COPY: Record<string, string> = {
	not_yet_delivered:
		"This message hasn't reached the other session yet, so it can't be marked as read. Try again in a moment.",
	message_not_found: 'This message could not be found — it may already have been removed.',
	persistence_disabled: 'Message history is turned off right now, so this cannot be acknowledged.',
	database_error: 'Something went wrong acknowledging this message. Try again in a moment.'
};

export const ACK_ERROR_DETAIL_SEP = '\u001f';

export function splitAckErrorMessage(message: string): { text: string; detail: string | null } {
	const [text, detail] = message.split(ACK_ERROR_DETAIL_SEP);
	return { text: text ?? message, detail: detail ?? null };
}

async function describeAckError(res: Response): Promise<string> {
	let body: Partial<MailboxErrorBody> = {};
	try {
		body = (await res.json()) as Partial<MailboxErrorBody>;
	} catch {}
	const detail =
		typeof body.message === 'string' && body.message.length > 0 ? body.message : res.statusText;
	const plain = typeof body.error === 'string' ? ACK_ERROR_COPY[body.error] : undefined;
	return plain !== undefined ? `${plain}${ACK_ERROR_DETAIL_SEP}${detail}` : detail;
}

export async function ackSessionMessage(
	sessionId: string,
	messageUuid: string
): Promise<AckMailboxMessageResponse> {
	const path = `/api/sessions/${encodeURIComponent(sessionId)}/messages/${encodeURIComponent(messageUuid)}/ack`;
	const res = await fetch(path, {
		method: 'POST',
		headers: { accept: 'application/json' }
	});
	if (!res.ok) {
		throw new ApiError(await describeAckError(res), res.status, path);
	}
	return (await res.json()) as AckMailboxMessageResponse;
}
