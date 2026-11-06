import { deleteConversation, listConversationIds } from './conversations.js';

/**
 * Deletes every conversation whose most recent message (or, for an
 * empty conversation, its creation time) is older than `keepDays` days
 * relative to `now`. Returns the ids it deleted, for logging/testing.
 */
export async function sweepOldConversations(
	stateDir: string,
	keepDays: number,
	now: Date = new Date()
): Promise<string[]> {
	const cutoffMs = now.getTime() - keepDays * 24 * 60 * 60 * 1000;
	const all = await listConversationIds(stateDir);
	const stale = all.filter((c) => new Date(c.lastActivity).getTime() < cutoffMs);
	for (const c of stale) {
		await deleteConversation(stateDir, c.id);
	}
	return stale.map((c) => c.id);
}
