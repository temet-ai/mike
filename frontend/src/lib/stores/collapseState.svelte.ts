const STORAGE_KEY = 'proxy-inspector:collapsed';

const SEEDED_STORAGE_KEY = 'proxy-inspector:collapsed-seeded';

const CAP = 3000;

export interface CollapseStore {
	isCollapsed(key: string): boolean;
	isExpanded(key: string, defaultExpanded?: boolean): boolean;
	toggle(key: string): void;
	setDefault(key: string, collapsed: boolean): void;
	setAll(keys: string[], collapsed: boolean): void;
}

function loadFromStorage(): Set<string> {
	try {
		const raw = localStorage.getItem(STORAGE_KEY);
		if (!raw) return new Set();
		const parsed: unknown = JSON.parse(raw);
		if (!Array.isArray(parsed)) return new Set();
		const set = new Set(parsed.filter((v): v is string => typeof v === 'string'));
		evictToCapInPlace(set);
		return set;
	} catch {
		return new Set();
	}
}

function evictToCapInPlace(set: Set<string>, protectedKey?: string): void {
	while (set.size > CAP) {
		const oldest = set.values().next().value as string | undefined;
		if (oldest === undefined) break;
		if (oldest === protectedKey) {
			set.delete(oldest);
			set.add(oldest);
			continue;
		}
		set.delete(oldest);
	}
}

function saveToStorage(collapsed: Set<string>): void {
	try {
		localStorage.setItem(STORAGE_KEY, JSON.stringify(Array.from(collapsed)));
	} catch {}
}

function loadSeededFromStorage(): Set<string> {
	try {
		const raw = sessionStorage.getItem(SEEDED_STORAGE_KEY);
		if (!raw) return new Set();
		const parsed: unknown = JSON.parse(raw);
		if (!Array.isArray(parsed)) return new Set();
		return new Set(parsed.filter((v): v is string => typeof v === 'string'));
	} catch {
		return new Set();
	}
}

function saveSeededToStorage(seeded: Set<string>): void {
	try {
		sessionStorage.setItem(SEEDED_STORAGE_KEY, JSON.stringify(Array.from(seeded)));
	} catch {}
}

export function createCollapseStore(): CollapseStore {
	const defaultsApplied = loadSeededFromStorage();

	let collapsed = $state<Set<string>>(loadFromStorage());

	return {
		isCollapsed(key: string): boolean {
			return collapsed.has(key);
		},

		isExpanded(key: string, defaultExpanded = false): boolean {
			return defaultExpanded ? !collapsed.has(key) : collapsed.has(key);
		},

		toggle(key: string): void {
			const next = new Set(collapsed);
			if (next.has(key)) {
				next.delete(key);
			} else {
				next.add(key);
				evictToCapInPlace(next, key);
			}
			collapsed = next;
			saveToStorage(next);
		},

		setDefault(key: string, value: boolean): void {
			if (defaultsApplied.has(key)) return;
			defaultsApplied.add(key);
			saveSeededToStorage(defaultsApplied);
			if (collapsed.has(key)) return;
			if (!value) return;
			const next = new Set(collapsed);
			next.add(key);
			collapsed = next;
			saveToStorage(next);
		},

		setAll(keys: string[], value: boolean): void {
			if (keys.length === 0) return;
			const next = new Set(collapsed);
			let changed = false;
			for (const key of keys) {
				if (value) {
					if (!next.has(key)) {
						next.add(key);
						changed = true;
					}
				} else if (next.delete(key)) {
					changed = true;
				}
			}
			if (!changed) return;
			collapsed = next;
			saveToStorage(next);
		}
	};
}
