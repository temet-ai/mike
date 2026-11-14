export interface DecisionArea {
	key: string;
	label: string;
}

export const DECISION_AREAS: readonly DecisionArea[] = [
	{ key: 'steering', label: 'Steering' },
	{ key: 'judges', label: 'Judges' },
	{ key: 'memory and lessons', label: 'Memory and lessons' },
	{ key: 'capture', label: 'Capture' },
	{ key: 'fleet', label: 'Fleet' },
	{ key: 'launch, mail and tools', label: 'Launch, mail and tools' }
];

export const OTHER_AREA: DecisionArea = { key: 'other', label: 'Other' };

export function areaOf(key: string): DecisionArea {
	return DECISION_AREAS.find((a) => a.key === key) ?? OTHER_AREA;
}

export function groupByArea<T extends { area: string }>(
	rows: readonly T[]
): { area: DecisionArea; rows: T[] }[] {
	const groups = new Map<string, T[]>();
	for (const row of rows) {
		const key = areaOf(row.area).key;
		const list = groups.get(key);
		if (list) list.push(row);
		else groups.set(key, [row]);
	}
	return [...DECISION_AREAS, OTHER_AREA]
		.filter((a) => groups.has(a.key))
		.map((a) => ({ area: a, rows: groups.get(a.key) ?? [] }));
}
