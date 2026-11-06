import { estimateTokens } from '$lib/components/message-view/parseSystemPrompt';

export type SystemBlockKind = 'billing' | 'identity' | 'instructions' | 'other';

export interface SystemBlockInput {
	text: string;
	index: number;
	raw?: unknown;
}

export interface BillingPair {
	key: string;
	value: string;
}

export interface ClassifiedSystemBlock {
	kind: SystemBlockKind;
	label: string;
	index: number;
	text: string;
	charCount: number;
	estTokens: number;
	billingPairs: BillingPair[];
	raw: unknown;
}

export const BILLING_PREFIX = 'x-anthropic-billing-header:';

export const IDENTITY_MAX_CHARS = 200;

export const INSTRUCTIONS_MIN_CHARS = 600;

export function parseBillingPairs(text: string): BillingPair[] {
	const trimmed = text.trimStart();
	if (!trimmed.startsWith(BILLING_PREFIX)) return [];
	const tail = trimmed.slice(BILLING_PREFIX.length);
	const pairs: BillingPair[] = [];
	for (const segment of tail.split(';')) {
		const part = segment.trim();
		if (part.length === 0) continue;
		const match = /^([^=]+)=(.*)$/.exec(part);
		if (!match) continue;
		const key = (match[1] ?? '').trim();
		const value = (match[2] ?? '').trim();
		if (key.length === 0) continue;
		pairs.push({ key, value });
	}
	return pairs;
}

export function classifySystemBlock(
	block: SystemBlockInput,
	position: number
): ClassifiedSystemBlock {
	const trimmed = block.text.trim();
	const base = {
		index: block.index,
		text: block.text,
		charCount: block.text.length,
		estTokens: estimateTokens(block.text.length),
		raw: block.raw ?? null
	};

	if (trimmed.startsWith(BILLING_PREFIX)) {
		return {
			...base,
			kind: 'billing',
			label: 'Billing header',
			billingPairs: parseBillingPairs(block.text)
		};
	}

	if (trimmed.startsWith('You are ') && trimmed.length <= IDENTITY_MAX_CHARS) {
		return { ...base, kind: 'identity', label: 'Identity', billingPairs: [] };
	}

	if (block.text.length >= INSTRUCTIONS_MIN_CHARS) {
		return { ...base, kind: 'instructions', label: 'Instructions', billingPairs: [] };
	}

	return { ...base, kind: 'other', label: `Block ${position + 1}`, billingPairs: [] };
}

export function classifySystemBlocks(blocks: readonly SystemBlockInput[]): ClassifiedSystemBlock[] {
	return blocks.map((block, i) => classifySystemBlock(block, i));
}
