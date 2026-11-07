import type { SystemPromptSource } from '$lib/types';

export type SystemPromptPlaceholder =
	{ kind: 'none' } | { kind: 'not_captured' } | { kind: 'no_blocks' };

/**
 * Which placeholder (if any) the workbench's prompt pane shows in place of the real
 * `WorkbenchSourcePane`. `not_captured` covers a request with no system-prompt capture of its
 * own — the fallback query never borrows another request's system prompt for it, so the pane
 * must say so plainly rather than rendering an empty pane that reads the same as "nothing to
 * filter here by design".
 */
export function systemPromptPlaceholder(
	blockCount: number,
	source: SystemPromptSource | null
): SystemPromptPlaceholder {
	if (blockCount > 0) return { kind: 'none' };
	if (source === 'not_captured') return { kind: 'not_captured' };
	return { kind: 'no_blocks' };
}

/**
 * Whether to show the "shared with the session's main prompt" note above the source pane — only
 * meaningful once there are blocks to show, and only for a request resolved via the session's
 * main-lane system prompt (a legitimate share, unlike the sidecar/subagent borrowing the fix
 * closes off).
 */
export function showsSessionMainNote(
	blockCount: number,
	source: SystemPromptSource | null
): boolean {
	return blockCount > 0 && source === 'session_main';
}
