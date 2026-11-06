import type { OperatorTurnDto, RequestSummary } from '$lib/types';

export interface ReplyView {
	text: string;
	source: 'stop_response' | 'final_assistant_text' | null;
	interrupted: boolean;
	reasonChip: string | null;
	show: boolean;
}

export function replyView(
	turn: Pick<
		OperatorTurnDto,
		'stop_response' | 'stop_reason' | 'interrupted' | 'final_assistant_text'
	>
): ReplyView {
	const stop = turn.stop_response ?? null;
	const interrupted = turn.interrupted === true;
	const reason = turn.stop_reason ?? null;

	let text = '';
	let source: ReplyView['source'] = null;
	if (stop !== null) {
		text = stop;
		source = 'stop_response';
	} else if (turn.final_assistant_text && turn.final_assistant_text.trim().length > 0) {
		text = turn.final_assistant_text;
		source = 'final_assistant_text';
	}

	return {
		text,
		source,
		interrupted,
		reasonChip: reason !== null && reason !== 'end_turn' ? reason : null,
		show: interrupted || text.trim().length > 0
	};
}

function isTerminalStatus(status: string): boolean {
	return status !== 'Pending' && status !== 'Streaming';
}

export function turnsRefetchSignal(
	requests: ReadonlyArray<Pick<RequestSummary, 'id' | 'status'>>
): string {
	let maxId = 0;
	let terminal = 0;
	for (const r of requests) {
		if (r.id > maxId) maxId = r.id;
		if (isTerminalStatus(r.status)) terminal++;
	}
	return `${requests.length}:${maxId}:${terminal}`;
}
