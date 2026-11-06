export type DiffRow =
	| {
			status: 'unchanged';
			text: string;
			originalIndex: number;
			sentIndex: number;
	  }
	| {
			status: 'replaced';
			originalText: string;
			sentText: string;
			originalIndex: number;
			sentIndex: number;
	  }
	| {
			status: 'dropped';
			text: string;
			originalIndex: number;
	  }
	| {
			status: 'appended';
			text: string;
			sentIndex: number;
	  };

export interface DiffInput {
	original: string[];
	sent: string[];
}

export function diffBlocks(input: DiffInput): DiffRow[] {
	const { original, sent } = input;

	if (original.length === 0 && sent.length === 0) return [];

	if (original.length === 0) {
		return sent.map((text, i) => ({
			status: 'appended',
			text,
			sentIndex: i
		}));
	}
	if (sent.length === 0) {
		return original.map((text, i) => ({
			status: 'dropped',
			text,
			originalIndex: i
		}));
	}

	const n = original.length;
	const m = sent.length;
	const dp: number[][] = Array.from({ length: n + 1 }, () => new Array(m + 1).fill(0));
	for (let i = 1; i <= n; i++) {
		for (let j = 1; j <= m; j++) {
			const a = original[i - 1];
			const b = sent[j - 1];
			const prevRow = dp[i - 1];
			const curRow = dp[i];
			if (prevRow === undefined || curRow === undefined) continue;
			if (a === b) {
				curRow[j] = (prevRow[j - 1] ?? 0) + 1;
			} else {
				curRow[j] = Math.max(prevRow[j] ?? 0, curRow[j - 1] ?? 0);
			}
		}
	}

	type RawOp =
		| { kind: 'unchanged'; text: string; oi: number; si: number }
		| { kind: 'dropped'; text: string; oi: number }
		| { kind: 'appended'; text: string; si: number };

	const reversed: RawOp[] = [];
	let i = n;
	let j = m;
	while (i > 0 && j > 0) {
		const a = original[i - 1];
		const b = sent[j - 1];
		const rowI = dp[i];
		const rowIm1 = dp[i - 1];
		if (rowI === undefined || rowIm1 === undefined) break;
		if (a === b) {
			reversed.push({ kind: 'unchanged', text: a ?? '', oi: i - 1, si: j - 1 });
			i--;
			j--;
		} else if ((rowIm1[j] ?? 0) >= (rowI[j - 1] ?? 0)) {
			reversed.push({ kind: 'dropped', text: a ?? '', oi: i - 1 });
			i--;
		} else {
			reversed.push({ kind: 'appended', text: b ?? '', si: j - 1 });
			j--;
		}
	}
	while (i > 0) {
		reversed.push({ kind: 'dropped', text: original[i - 1] ?? '', oi: i - 1 });
		i--;
	}
	while (j > 0) {
		reversed.push({ kind: 'appended', text: sent[j - 1] ?? '', si: j - 1 });
		j--;
	}
	const ops = reversed.reverse();

	const out: DiffRow[] = [];
	let k = 0;
	while (k < ops.length) {
		const cur = ops[k];
		const next = ops[k + 1];
		if (cur === undefined) break;
		if (cur.kind === 'dropped' && next !== undefined && next.kind === 'appended') {
			out.push({
				status: 'replaced',
				originalText: cur.text,
				sentText: next.text,
				originalIndex: cur.oi,
				sentIndex: next.si
			});
			k += 2;
			continue;
		}
		if (cur.kind === 'appended' && next !== undefined && next.kind === 'dropped') {
			out.push({
				status: 'replaced',
				originalText: next.text,
				sentText: cur.text,
				originalIndex: next.oi,
				sentIndex: cur.si
			});
			k += 2;
			continue;
		}
		if (cur.kind === 'unchanged') {
			out.push({
				status: 'unchanged',
				text: cur.text,
				originalIndex: cur.oi,
				sentIndex: cur.si
			});
		} else if (cur.kind === 'dropped') {
			out.push({
				status: 'dropped',
				text: cur.text,
				originalIndex: cur.oi
			});
		} else {
			out.push({
				status: 'appended',
				text: cur.text,
				sentIndex: cur.si
			});
		}
		k++;
	}
	return out;
}
