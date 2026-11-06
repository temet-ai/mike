export interface MdSegment {
	text: string;
	bold: boolean;
	code: boolean;
	/** The target of a `[text](url)` link segment, or `null` for plain/bold/code text. A
	 *  link segment is never also `code` or `bold` — link text is taken verbatim, with no
	 *  nested inline formatting. */
	href: string | null;
}

export type MdBlock =
	| { kind: 'heading'; level: number; segments: MdSegment[] }
	| { kind: 'ordered'; marker: string; segments: MdSegment[] }
	| { kind: 'bullet'; segments: MdSegment[] }
	| { kind: 'code'; text: string }
	| { kind: 'paragraph'; segments: MdSegment[] };

const LINK_RE = /^\[([^\]]+)\]\(([^)]+)\)$/;
const SAFE_HREF_RE = /^(https?:\/\/|mailto:)/i;

/** Only absolute http(s) and mailto targets become links. Anything else (`javascript:`,
 *  `data:`, relative paths) stays literal text, so model-written markdown can never
 *  produce a script URL in the DOM, and the target stays visible to the operator. */
export function isSafeHref(href: string): boolean {
	return SAFE_HREF_RE.test(href.trim());
}

/** Appends `seg`, folding it into the previous segment when both are plain text of the same
 *  weight, so literal text left over from a rejected link reads as one run. */
function pushSegment(out: MdSegment[], seg: MdSegment): void {
	const last = out[out.length - 1];
	const plain = (s: MdSegment) => !s.code && s.href === null;
	if (last && plain(last) && plain(seg) && last.bold === seg.bold) {
		last.text += seg.text;
		return;
	}
	out.push(seg);
}

function parseInline(text: string, bold = false): MdSegment[] {
	const out: MdSegment[] = [];
	const parts = text.split(/(`[^`]+`|\*\*[^*]+\*\*|\[[^\]]+\]\([^)]+\))/);
	for (const part of parts) {
		if (!part) continue;
		const linkMatch = LINK_RE.exec(part);
		if (linkMatch && isSafeHref(linkMatch[2]!)) {
			pushSegment(out, { text: linkMatch[1]!, bold: false, code: false, href: linkMatch[2]! });
		} else if (part.startsWith('`') && part.endsWith('`') && part.length >= 2) {
			pushSegment(out, { text: part.slice(1, -1), bold: false, code: true, href: null });
		} else if (part.startsWith('**') && part.endsWith('**') && part.length >= 4) {
			for (const inner of parseInline(part.slice(2, -2), true)) pushSegment(out, inner);
		} else {
			pushSegment(out, { text: part, bold, code: false, href: null });
		}
	}
	return out;
}

const HEADING_RE = /^(#{1,6})\s+(.*)$/;
const ORDERED_RE = /^(\d+)\.\s+(.*)$/;
const BULLET_RE = /^[-*]\s+(.*)$/;
const FENCE_RE = /^```/;

export function parseSimpleMarkdown(source: string): MdBlock[] {
	const blocks: MdBlock[] = [];
	const lines = source.split('\n');
	let i = 0;
	while (i < lines.length) {
		const line = lines[i]!;
		if (FENCE_RE.test(line)) {
			const codeLines: string[] = [];
			i += 1;
			while (i < lines.length && !FENCE_RE.test(lines[i]!)) {
				codeLines.push(lines[i]!);
				i += 1;
			}
			i += 1; // skip closing fence
			blocks.push({ kind: 'code', text: codeLines.join('\n') });
			continue;
		}
		if (!line.trim()) {
			i += 1;
			continue;
		}
		let m: RegExpMatchArray | null;
		if ((m = HEADING_RE.exec(line))) {
			blocks.push({ kind: 'heading', level: m[1]!.length, segments: parseInline(m[2]!) });
		} else if ((m = ORDERED_RE.exec(line))) {
			blocks.push({ kind: 'ordered', marker: `${m[1]}.`, segments: parseInline(m[2]!) });
		} else if ((m = BULLET_RE.exec(line))) {
			blocks.push({ kind: 'bullet', segments: parseInline(m[1]!) });
		} else {
			blocks.push({ kind: 'paragraph', segments: parseInline(line) });
		}
		i += 1;
	}
	return blocks;
}
