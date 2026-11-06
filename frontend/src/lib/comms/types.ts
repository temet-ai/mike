import type { MailboxMessage } from '$lib/api/mailbox';

export type ThreadDeliveryState =
	'pending' | 'claimed' | 'delivered' | 'consumed' | 'dead_lettered';

export type ContextItemKind =
	'request' | 'session' | 'thread' | 'message' | 'commit' | 'file' | 'issue' | 'url' | 'note';

export interface ContextItem {
	kind: ContextItemKind;
	value: string;
	label: string | null;
}

export interface ParticipantIdentityDto {
	session_uuid: string;
	label: string;
	role: string | null;
	agent_type: string | null;
}

export interface ThreadProjectDto {
	project_uuid: string;
	name: string;
	path: string;
	pinned?: boolean;
}

export interface ThreadFleetDto {
	fleet_uuid: string;
	slug: string;
	state: string;
}

export interface ThreadSummaryDto {
	thread_uuid: string;
	subject: string;
	created_at: string;
	last_message_at: string;
	resolved_at: string | null;
	created_by_session_uuid: string | null;
	participants: string[];
	participant_identities?: ParticipantIdentityDto[];
	tags: string[];
	message_count: number;
	unread_count: number;
	worst_delivery_state: ThreadDeliveryState;
	has_stale_pending: boolean;
	project?: ThreadProjectDto | null;
	fleet?: ThreadFleetDto | null;
}

export interface ThreadMessageDto extends MailboxMessage {
	thread_uuid: string | null;
	in_reply_to_uuid: string | null;
	context_items: ContextItem[];
}

export interface SessionThreadsResponse {
	threads: ThreadSummaryDto[];
	total_count: number;
}

export interface ThreadDetailResponse {
	thread: ThreadSummaryDto;
	messages: ThreadMessageDto[];
}

export type ThreadsListResponse = SessionThreadsResponse;

export interface ThreadMessageDelivery {
	message_uuid: string;
	to_session_uuid: string;
}

export interface ThreadMessagePostResponse {
	thread_uuid: string;
	subject: string;
	deliveries: ThreadMessageDelivery[];
}

export interface ThreadReplyBody {
	from_session_id: string | null;
	body: string;
	to?: string[];
	in_reply_to?: string;
	context?: ContextItem[];
}

export interface ThreadResolveResponse {
	thread_uuid: string;
	resolved_at: string;
}

export interface NewThreadMessageBody {
	from_session_id?: string;
	message: string;
	subject: string;
	tags?: string[];
	context?: ContextItem[];
}

export interface ThreadTagDto {
	code: string;
	hue: number | null;
	usage_count: number;
}

export interface TagsResponse {
	tags: ThreadTagDto[];
}

export interface SessionFleetDto {
	in_fleet: boolean;
	peer_count: number;
	message_count: number;
	fleet_size: number;
	window_hours: number;
}

export interface PeerFleetIdentity {
	role: string | null;
	agentType: string | null;
	state: 'working' | 'idle' | 'stale' | 'done' | 'error' | 'unknown';
	unconsumedMail: number;
	label: string | null;
}

export interface PeerSummary {
	sessionId: string;
	messageCount: number;
	unconsumedCount: number;
	identity: PeerFleetIdentity;
}
