export const SESSION_LIST_STORE_KEY = Symbol('session-list-store');

export const SESSION_DETAIL_STORE_KEY = Symbol('session-detail-store');

export const COLLAPSE_STORE_KEY = Symbol('collapse-store');

export const FILTER_RULES_STORE_KEY = Symbol('filter-rules-store');

export const STATS_KEY = Symbol('stats');
export type StatsContext = {
	readonly value: import('$lib/types').StatsResponse | null;
};

export const MAILBOX_STORE_KEY = Symbol('mailbox-store');

export const SESSION_FLOW_STORE_KEY = Symbol('session-flow-store');
