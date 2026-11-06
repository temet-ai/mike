const LS_KEY = 'mike:capability-banner-dismissed:v1';

function dismissalKey(id: string, state: string): string {
	return `${id}:${state}`;
}

function loadPersisted(): Set<string> {
	if (typeof window === 'undefined' || typeof localStorage === 'undefined') return new Set();
	try {
		const raw = localStorage.getItem(LS_KEY);
		if (!raw) return new Set();
		const parsed: unknown = JSON.parse(raw);
		if (!Array.isArray(parsed)) return new Set();
		return new Set(parsed.filter((v): v is string => typeof v === 'string'));
	} catch {
		return new Set();
	}
}

function persist(dismissed: ReadonlySet<string>): void {
	if (typeof window === 'undefined' || typeof localStorage === 'undefined') return;
	try {
		localStorage.setItem(LS_KEY, JSON.stringify(Array.from(dismissed)));
	} catch {}
}

let dismissed = $state<Set<string>>(loadPersisted());

export const capabilityBannerDismissal = {
	isDismissed(id: string, state: string): boolean {
		return dismissed.has(dismissalKey(id, state));
	},

	dismiss(id: string, state: string): void {
		const key = dismissalKey(id, state);
		if (dismissed.has(key)) return;
		const next = new Set(dismissed);
		next.add(key);
		dismissed = next;
		persist(next);
	}
};
