import type { ProxyEvent, RowChangedOp, RowChangedTable } from '$lib/types';

export type RowChangedEvent = Extract<ProxyEvent, { type: 'RowChanged' }>;

export interface RowChangedFilter {
	readonly table: RowChangedTable;
	readonly ops?: readonly RowChangedOp[];
}

export function shouldRefetchOn(
	event: ProxyEvent,
	filters: readonly RowChangedFilter[]
): event is RowChangedEvent {
	if (event.type !== 'RowChanged') return false;
	for (const filter of filters) {
		if (filter.table !== event.table) continue;
		if (!filter.ops || filter.ops.includes(event.op)) return true;
	}
	return false;
}
