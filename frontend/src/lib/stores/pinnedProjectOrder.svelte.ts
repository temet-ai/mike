const LS_KEY = 'mike:pinned-project-order:v1';

function loadPersisted(): string[] {
	if (typeof window === 'undefined' || typeof localStorage === 'undefined') return [];
	try {
		const raw = localStorage.getItem(LS_KEY);
		if (!raw) return [];
		const parsed: unknown = JSON.parse(raw);
		if (!Array.isArray(parsed)) return [];
		return parsed.filter((v): v is string => typeof v === 'string');
	} catch {
		return [];
	}
}

function persist(order: readonly string[]): void {
	if (typeof window === 'undefined' || typeof localStorage === 'undefined') return;
	try {
		localStorage.setItem(LS_KEY, JSON.stringify(order));
	} catch {}
}

function sameOrder(a: readonly string[], b: readonly string[]): boolean {
	return a.length === b.length && a.every((v, i) => v === b[i]);
}

export function computeDisplayOrder(
	order: readonly string[],
	pinnedPaths: readonly string[]
): string[] {
	const pinnedSet = new Set(pinnedPaths);
	const known = order.filter((p) => pinnedSet.has(p));
	const additions = pinnedPaths.filter((p) => !order.includes(p));
	return [...known, ...additions];
}

let order = $state<string[]>(loadPersisted());

export const pinnedProjectOrder = {
	get order(): readonly string[] {
		return order;
	},

	reconcile(pinnedPaths: readonly string[]): void {
		const next = computeDisplayOrder(order, pinnedPaths);
		if (sameOrder(next, order)) return;
		order = next;
		persist(order);
	},

	reorder(nextOrder: readonly string[]): void {
		order = [...nextOrder];
		persist(order);
	}
};
