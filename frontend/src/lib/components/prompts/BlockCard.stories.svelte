<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import BlockCard from './BlockCard.svelte';
	import type { BlockBadge } from './BlockCard.svelte';

	const SHORT_TEXT = 'You are Claude, an AI assistant built by Anthropic.';

	const LONG_TEXT = `You are Claude, an AI assistant operating inside the temet-proxy development environment.

Environment context:
- Platform: macOS 25.4.0
- Shell: zsh
- Working directory: /Users/dev/projects/ai/temet-proxy
- Today's date: 2026-05-04

Tool-use conventions:
- Prefer the dedicated tools (Read, Edit, Write, Bash) over shelling out to find / grep / cat.
- Batch independent tool calls into a single response turn when possible — parallel calls are faster.
- For research tasks spanning more than 3 queries, spawn an Explore sub-agent.
- Never use cat/head/tail/sed/awk/echo unless explicitly instructed.

Editing conventions:
- Prefer editing existing files over creating new ones.
- Never create documentation files (*.md) unless the user explicitly asks.
- Default to writing no comments. Only add a comment when the WHY is non-obvious.
- No multi-paragraph docstrings or multi-line comment blocks.

Response style:
- Concise. One sentence per update is almost always enough.
- No emojis unless the user explicitly requests them.
- No trailing summaries — the user can read the diff.
- When referencing code, use file_path:line_number format.

Security:
- Never generate or guess URLs unless you are confident they help the user with programming.
- Validate at system boundaries (user input, external APIs) — trust internal code.`;

	function buildCard(overrides: {
		label: string;
		text: string;
		badge?: BlockBadge;
		editable?: boolean;
		railRole?: 'volatile' | 'preamble' | 'skeleton';
		expandable?: boolean;
	}): {
		label: string;
		text: string;
		badge?: BlockBadge;
		editable?: boolean;
		railRole?: 'volatile' | 'preamble' | 'skeleton';
		expandable?: boolean;
	} {
		return {
			badge: null,
			editable: false,
			railRole: 'preamble',
			expandable: false,
			...overrides
		};
	}

	const SHORT_BLOCK = buildCard({ label: 'block[1]', text: SHORT_TEXT });

	const LONG_BLOCK = buildCard({
		label: 'block[2]',
		text: LONG_TEXT,
		railRole: 'skeleton'
	});

	const CACHE_CONTROL_BLOCK = buildCard({
		label: 'block[0]',
		text: 'HUMAN_TURN\n\nHuman: <system-reminder>You are operating inside the temet-proxy harness.</system-reminder>',
		badge: 'volatile',
		railRole: 'volatile'
	});

	const AUTHORED_BLOCK = buildCard({
		label: 'identity preamble',
		text: 'You are a frontend development expert specialising in Svelte 5, TypeScript, and Tailwind v4.',
		badge: 'authored',
		railRole: 'skeleton'
	});

	const DRAFT_BLOCK = buildCard({
		label: 'identity preamble (draft)',
		text: 'You are a frontend development expert. Prefer composable components and typed fixtures.',
		badge: 'draft',
		railRole: 'preamble'
	});

	const EDITABLE_BLOCK = buildCard({
		label: 'block[2] (editable)',
		text: 'You are Claude. Help the user with their coding task.',
		editable: true,
		railRole: 'skeleton'
	});

	const RAIL_BODY =
		'You are Claude, an AI assistant built by Anthropic. This block is identical across the three rail-role stories so the only changing variable is the 3px left-rail accent color.';

	const RAIL_VOLATILE = buildCard({
		label: 'block[0]',
		text: RAIL_BODY,
		railRole: 'volatile'
	});

	const RAIL_PREAMBLE = buildCard({
		label: 'block[1]',
		text: RAIL_BODY,
		railRole: 'preamble'
	});

	const RAIL_SKELETON = buildCard({
		label: 'block[2]',
		text: RAIL_BODY,
		railRole: 'skeleton'
	});

	function buildLargeSkeletonPrompt(): string {
		const header = `You are Claude, an AI assistant operating inside the temet-proxy development environment. This is the operator-supplied identity + instructions block (block[2], the "skeleton"). On real composition detail pages this block routinely exceeds 30,000 characters; the expandable card must render every byte.\n\n`;
		const sections: string[] = [header];
		for (let i = 1; i <= 60; i++) {
			sections.push(
				`## Operating directive ${i}\n\n` +
					`Directive ${i} of the operator instruction set. Each directive in a real prompt is a self-contained paragraph of guidance the operator authored; the proxy must surface all of them verbatim so an operator auditing prompt drift can read the complete instruction surface without a "show more" gate.\n\n` +
					`- Constraint ${i}.a: never reduce captured content to a preview; the operator is debugging the exact bytes the model received.\n` +
					`- Constraint ${i}.b: editing conventions, response style, and security boundaries are all carried in blocks like this one and must round-trip without amputation.\n` +
					`- Constraint ${i}.c: when this block is rendered expandable the card grows tall and the page scrolls past it — that is the intended inspection affordance.\n\n`
			);
		}
		return sections.join('');
	}

	const EXPANDABLE_LARGE = buildCard({
		label: 'block[2] (skeleton, expandable)',
		text: buildLargeSkeletonPrompt(),
		railRole: 'skeleton',
		expandable: true
	});

	const { Story } = defineMeta({
		title: 'Routes/Prompts/BlockCard',
		component: BlockCard,
		tags: ['autodocs'],
		parameters: {
			a11y: {
				config: {
					rules: [{ id: 'label', enabled: false }]
				}
			}
		}
	});
</script>

<Story name="ShortBlock" args={SHORT_BLOCK}>
	{#snippet template(args)}
		<div class="p-4 bg-[var(--surface-card)]">
			<BlockCard {...args} />
		</div>
	{/snippet}
</Story>

<Story name="LongBlock" args={LONG_BLOCK}>
	{#snippet template(args)}
		<div class="p-4 bg-[var(--surface-card)]">
			<BlockCard {...args} />
		</div>
	{/snippet}
</Story>

<Story name="WithCacheControl" args={CACHE_CONTROL_BLOCK}>
	{#snippet template(args)}
		<div class="p-4 bg-[var(--surface-card)]">
			<BlockCard {...args} />
		</div>
	{/snippet}
</Story>

<Story name="AuthoredBlock" args={AUTHORED_BLOCK}>
	{#snippet template(args)}
		<div class="p-4 bg-[var(--surface-card)]">
			<BlockCard {...args} />
		</div>
	{/snippet}
</Story>

<Story name="DraftBlock" args={DRAFT_BLOCK}>
	{#snippet template(args)}
		<div class="p-4 bg-[var(--surface-card)]">
			<BlockCard {...args} />
		</div>
	{/snippet}
</Story>

<Story name="EditableBlock" args={EDITABLE_BLOCK}>
	{#snippet template(args)}
		<div class="p-4 bg-[var(--surface-card)]">
			<BlockCard {...args} />
		</div>
	{/snippet}
</Story>

<Story name="RailVolatile" args={RAIL_VOLATILE}>
	{#snippet template(args)}
		<div class="p-4 bg-[var(--surface-card)]">
			<BlockCard {...args} />
		</div>
	{/snippet}
</Story>

<Story name="RailPreamble" args={RAIL_PREAMBLE}>
	{#snippet template(args)}
		<div class="p-4 bg-[var(--surface-card)]">
			<BlockCard {...args} />
		</div>
	{/snippet}
</Story>

<Story name="RailSkeleton" args={RAIL_SKELETON}>
	{#snippet template(args)}
		<div class="p-4 bg-[var(--surface-card)]">
			<BlockCard {...args} />
		</div>
	{/snippet}
</Story>

<Story name="ExpandableLarge" args={EXPANDABLE_LARGE}>
	{#snippet template(args)}
		<div class="p-4 bg-[var(--surface-card)] max-h-[600px] overflow-y-auto">
			<BlockCard {...args} />
		</div>
	{/snippet}
</Story>
