<script module lang="ts">
	export type RenderedBlock =
		| { kind: 'text'; text: string }
		| { kind: 'thinking'; text: string; signature: string | null }
		| { kind: 'tool_use'; id: string; name: string; input: unknown }
		| { kind: 'tool_result'; tool_use_id: string; blocks: RenderedBlock[] }
		| { kind: 'image' }
		| { kind: 'unknown'; type: string };

	export type RenderedMessage = {
		role: string;
		blocks: RenderedBlock[];
	};

	export type RenderedBody =
		| { shape: 'request'; messages: RenderedMessage[] }
		| { shape: 'response'; blocks: RenderedBlock[] }
		| { shape: 'unknown' };

	function isRecord(value: unknown): value is Record<string, unknown> {
		return typeof value === 'object' && value !== null && !Array.isArray(value);
	}

	const KNOWN_CONTENT_BLOCK_TYPES = new Set([
		'text',
		'thinking',
		'tool_use',
		'tool_result',
		'image'
	]);

	export function walkContentBlock(block: unknown): RenderedBlock {
		if (typeof block === 'string') {
			return { kind: 'text', text: block };
		}
		if (!isRecord(block)) {
			return { kind: 'unknown', type: typeof block };
		}
		const type = typeof block.type === 'string' ? block.type : '';
		switch (type) {
			case 'text': {
				const text = typeof block.text === 'string' ? block.text : '';
				return { kind: 'text', text };
			}
			case 'thinking': {
				const text = typeof block.thinking === 'string' ? block.thinking : '';
				const signature = typeof block.signature === 'string' ? block.signature : null;
				return { kind: 'thinking', text, signature };
			}
			case 'tool_use': {
				const id = typeof block.id === 'string' ? block.id : '';
				const name = typeof block.name === 'string' ? block.name : '';
				return { kind: 'tool_use', id, name, input: block.input };
			}
			case 'tool_result': {
				const toolUseId = typeof block.tool_use_id === 'string' ? block.tool_use_id : '';
				const inner: RenderedBlock[] = [];
				const content = block.content;
				if (typeof content === 'string') {
					inner.push({ kind: 'text', text: content });
				} else if (Array.isArray(content)) {
					for (const child of content) {
						inner.push(walkContentBlock(child));
					}
				} else if (content !== undefined) {
					inner.push(walkContentBlock(content));
				}
				return { kind: 'tool_result', tool_use_id: toolUseId, blocks: inner };
			}
			case 'image':
				return { kind: 'image' };
			default:
				return { kind: 'unknown', type: type || 'unknown' };
		}
	}

	function walkMessage(msg: unknown): RenderedMessage | null {
		if (!isRecord(msg)) return null;
		const role = typeof msg.role === 'string' ? msg.role : 'unknown';
		const content = msg.content;
		const blocks: RenderedBlock[] = [];
		if (typeof content === 'string') {
			blocks.push({ kind: 'text', text: content });
		} else if (Array.isArray(content)) {
			for (const block of content) {
				blocks.push(walkContentBlock(block));
			}
		} else {
			return { role, blocks: [] };
		}
		return { role, blocks };
	}

	export function renderBody(parsed: unknown): RenderedBody {
		if (
			Array.isArray(parsed) &&
			parsed.length > 0 &&
			parsed.every((e) => isRecord(e) && typeof e.type === 'string')
		) {
			return { shape: 'response', blocks: parsed.map((block) => walkContentBlock(block)) };
		}
		if (!isRecord(parsed)) return { shape: 'unknown' };
		const messages = parsed.messages;
		if (Array.isArray(messages) && messages.length > 0) {
			const walked: RenderedMessage[] = [];
			let allValid = true;
			for (const entry of messages) {
				const msg = walkMessage(entry);
				if (!msg) {
					allValid = false;
					break;
				}
				walked.push(msg);
			}
			if (allValid) {
				return { shape: 'request', messages: walked };
			}
		}
		const content = parsed.content;
		if (Array.isArray(content) && content.length > 0) {
			let allValid = true;
			for (const entry of content) {
				if (typeof entry === 'string') continue;
				if (isRecord(entry) && typeof entry.type === 'string') continue;
				allValid = false;
				break;
			}
			if (allValid) {
				const blocks = content.map((block) => walkContentBlock(block));
				return { shape: 'response', blocks };
			}
		}
		if (typeof parsed.type === 'string' && KNOWN_CONTENT_BLOCK_TYPES.has(parsed.type)) {
			return { shape: 'response', blocks: [walkContentBlock(parsed)] };
		}
		return { shape: 'unknown' };
	}
</script>

<script lang="ts">
	import ToolCallCard from '$lib/components/tool-calls/ToolCallCard.svelte';
	import ToolResultCard from '$lib/components/tool-calls/ToolResultCard.svelte';

	let { blocks }: { blocks: RenderedBlock[] } = $props();

	function extractResultText(innerBlocks: RenderedBlock[]): string {
		return innerBlocks
			.filter((b): b is Extract<RenderedBlock, { kind: 'text' }> => b.kind === 'text')
			.map((b) => b.text)
			.join('\n');
	}
</script>

<div class="flex flex-col gap-2">
	{#each blocks as block, i (i)}
		{#if block.kind === 'text'}
			<pre
				class="whitespace-pre-wrap break-words font-mono text-xs text-[var(--text-strong)]">{block.text}</pre>
		{:else if block.kind === 'thinking'}
			<div
				class="flex flex-col gap-1 rounded-[2px] border border-[var(--border-subtle)] border-l-2 border-l-[var(--status-info-border)] bg-[var(--surface-2)] p-2"
			>
				<div
					class="flex items-center gap-2 text-label uppercase tracking-wider text-[var(--text-3)]"
				>
					<span>Thinking</span>
					{#if block.signature}
						<span
							class="font-mono normal-case tracking-normal text-[var(--text-faint)]"
							title="Anthropic thinking-block signature">signed</span
						>
					{/if}
				</div>
				<pre
					class="whitespace-pre-wrap break-words font-mono text-xs text-[var(--text-2)]">{block.text}</pre>
			</div>
		{:else if block.kind === 'tool_use'}
			<ToolCallCard name={block.name} input={block.input} showRawInput={false} />
		{:else if block.kind === 'tool_result'}
			<ToolResultCard content={extractResultText(block.blocks)} />
		{:else if block.kind === 'image'}
			<span class="text-label italic text-[var(--text-faint)]">[image]</span>
		{:else if block.kind === 'unknown'}
			<span class="text-label italic text-[var(--text-faint)]">[{block.type}]</span>
		{/if}
	{/each}
</div>
