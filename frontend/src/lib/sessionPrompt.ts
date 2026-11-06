const TITLE_GEN_ENVELOPE_RE = /^\s*<session>([\s\S]*)<\/session>[\s\S]*$/;

const RELAY_ENVELOPE_RE =
	/^\s*<(teammate-message|agent-message)\b[^>]*>([\s\S]*?)(?:<\/\1>[\s\S]*)?$/;

export function operatorPromptText(raw: string | null | undefined): string {
	if (typeof raw !== 'string') return '';
	const titled = TITLE_GEN_ENVELOPE_RE.exec(raw)?.[1];
	if (titled !== undefined) return titled.trim();
	const relayed = RELAY_ENVELOPE_RE.exec(raw)?.[2];
	return (relayed ?? raw).trim();
}
