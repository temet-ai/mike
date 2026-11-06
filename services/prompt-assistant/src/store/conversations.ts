import { randomUUID } from 'node:crypto';
import fs from 'node:fs/promises';
import path from 'node:path';
import type { Conversation, ConversationMessage } from '../types.js';

function conversationsDir(stateDir: string): string {
	return path.join(stateDir, 'conversations');
}

function conversationFile(stateDir: string, id: string): string {
	return path.join(conversationsDir(stateDir), `${id}.json`);
}

async function ensureDir(stateDir: string): Promise<void> {
	await fs.mkdir(conversationsDir(stateDir), { recursive: true });
}

export async function createConversation(stateDir: string, requestId: number): Promise<Conversation> {
	await ensureDir(stateDir);
	const now = new Date().toISOString();
	const conversation: Conversation = {
		id: randomUUID(),
		request_id: requestId,
		created_at: now,
		updated_at: now,
		messages: []
	};
	await fs.writeFile(conversationFile(stateDir, conversation.id), JSON.stringify(conversation, null, 2));
	return conversation;
}

export async function getConversation(stateDir: string, id: string): Promise<Conversation | null> {
	try {
		const raw = await fs.readFile(conversationFile(stateDir, id), 'utf8');
		return JSON.parse(raw) as Conversation;
	} catch (err) {
		if ((err as NodeJS.ErrnoException).code === 'ENOENT') return null;
		throw err;
	}
}

async function saveConversation(stateDir: string, conversation: Conversation): Promise<void> {
	await ensureDir(stateDir);
	await fs.writeFile(
		conversationFile(stateDir, conversation.id),
		JSON.stringify(conversation, null, 2)
	);
}

export function newMessage(
	role: ConversationMessage['role'],
	text: string,
	extra: Partial<ConversationMessage> = {}
): ConversationMessage {
	return {
		id: randomUUID(),
		role,
		text,
		created_at: new Date().toISOString(),
		...extra
	};
}

/**
 * Appends a message to a conversation and persists it. Returns the updated
 * conversation, or null when the conversation does not exist.
 */
export async function appendMessage(
	stateDir: string,
	id: string,
	message: ConversationMessage
): Promise<Conversation | null> {
	const conversation = await getConversation(stateDir, id);
	if (conversation === null) return null;
	conversation.messages.push(message);
	conversation.updated_at = new Date().toISOString();
	await saveConversation(stateDir, conversation);
	return conversation;
}

/** Lists every conversation file's id and last-activity timestamp, for the keep_days sweep. */
export async function listConversationIds(
	stateDir: string
): Promise<Array<{ id: string; lastActivity: string }>> {
	let entries: string[];
	try {
		entries = await fs.readdir(conversationsDir(stateDir));
	} catch (err) {
		if ((err as NodeJS.ErrnoException).code === 'ENOENT') return [];
		throw err;
	}
	const out: Array<{ id: string; lastActivity: string }> = [];
	for (const entry of entries) {
		if (!entry.endsWith('.json')) continue;
		const id = entry.slice(0, -'.json'.length);
		const conversation = await getConversation(stateDir, id);
		if (conversation === null) continue;
		const lastActivity =
			conversation.messages.length > 0
				? (conversation.messages[conversation.messages.length - 1]?.created_at ?? conversation.updated_at)
				: conversation.updated_at;
		out.push({ id, lastActivity });
	}
	return out;
}

export async function deleteConversation(stateDir: string, id: string): Promise<void> {
	await fs.rm(conversationFile(stateDir, id), { force: true });
}
