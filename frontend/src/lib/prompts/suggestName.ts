const MAX_WORDS = 8;
const TRAILING_FILLER = new Set(['a', 'an', 'the', 'for', 'of', 'to', 'and', 'with', 'in', 'on', 'from', 'by']);

/**
 * A short name for a composition, taken from the first sentence of its agent instructions:
 * "You are a code reviewer…" → "Code reviewer", "You write a 3-5 word progress label…" →
 * "Write a 3-5 word progress label". Null when the text has no usable first sentence.
 */
export function suggestCompositionName(instructions: string): string | null {
	const firstLine = instructions
		.split('\n')
		.map((line) => line.replace(/^[#>*\-\s]+/, '').trim())
		.find((line) => line.length > 0);
	if (firstLine === undefined) return null;

	const role = firstLine.match(/^you are (?:an? |the )?([^.,:;!?]+)/i);
	const phrase = role?.[1] ?? firstLine.replace(/^you /i, '').split(/[.:;!?]\s|[.:;!?]$/)[0] ?? '';

	const words = phrase.split(/\s+/).filter((w) => w.length > 0).slice(0, MAX_WORDS);
	while (words.length > 1 && TRAILING_FILLER.has((words.at(-1) ?? '').toLowerCase())) words.pop();
	const name = words.join(' ').replace(/[,;:]+$/, '');
	if (name.length === 0) return null;
	return name.charAt(0).toUpperCase() + name.slice(1);
}
