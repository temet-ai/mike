import type { ThreadMessageDto } from './types';

export interface MessageTreeNode {
	message: ThreadMessageDto;
	depth: number;
	children: MessageTreeNode[];
}

export const MAX_VISUAL_INDENT_DEPTH = 4;

export function buildMessageTree(messages: ThreadMessageDto[]): MessageTreeNode[] {
	const byUuid = new Map(messages.map((m) => [m.message_uuid, m]));
	const childrenOf = new Map<string, ThreadMessageDto[]>();
	const roots: ThreadMessageDto[] = [];

	for (const m of messages) {
		const parentUuid = m.in_reply_to_uuid;
		if (parentUuid && byUuid.has(parentUuid)) {
			const list = childrenOf.get(parentUuid) ?? [];
			list.push(m);
			childrenOf.set(parentUuid, list);
		} else {
			roots.push(m);
		}
	}

	function build(m: ThreadMessageDto, depth: number): MessageTreeNode {
		const kids = childrenOf.get(m.message_uuid) ?? [];
		return { message: m, depth, children: kids.map((k) => build(k, depth + 1)) };
	}

	return roots.map((r) => build(r, 0));
}
