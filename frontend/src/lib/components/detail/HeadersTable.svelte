<script lang="ts" module>
	export const SENSITIVE_HEADER_NAMES = new Set<string>([
		'x-api-key',
		'authorization',
		'cookie',
		'set-cookie'
	]);

	const MASKED_VALUE = '\u2022\u2022\u2022\u2022\u2022\u2022\u2022\u2022';

	export function isSensitiveHeader(name: string): boolean {
		return SENSITIVE_HEADER_NAMES.has(name.toLowerCase());
	}

	export function displayHeaderValue(name: string, value: string): string {
		return isSensitiveHeader(name) ? MASKED_VALUE : value;
	}

	export interface HeaderHelp {
		text: string;
		slug?: string;
	}

	export const KNOWN_HEADERS_HELP: Record<string, HeaderHelp> = {
		'anthropic-version': {
			text: 'Anthropic API version pinned by the client (e.g. 2023-06-01)',
			slug: 'anthropic-headers'
		},
		'anthropic-beta': {
			text: 'Comma-separated beta-feature flags requested by the client',
			slug: 'anthropic-headers'
		},
		'x-api-key': {
			text: 'Anthropic auth key — masked client-side (eight bullets) and server-side',
			slug: 'anthropic-headers'
		},
		authorization: {
			text: 'Auth header — masked client-side'
		},
		cookie: {
			text: 'Masked: cookies are not relevant to /v1/messages but appear if a proxy in the chain sets them'
		},
		'set-cookie': {
			text: 'Masked: cookies are not relevant to /v1/messages but appear if a proxy in the chain sets them'
		},
		'user-agent': {
			text: 'Client identification (Claude Code version, terminal, OS)'
		},
		'retry-after': {
			text: 'Anthropic-supplied wait hint after a 429 / 529. Honour it.',
			slug: 'retry-after'
		},
		'request-id': {
			text: 'Anthropic request-id (req_xxx) — quote in support tickets',
			slug: 'upstream-request-id'
		},
		'anthropic-ratelimit-requests-limit': {
			text: 'Per-minute rate-limit family — limit / remaining / reset epoch',
			slug: 'rate-limit-headers'
		},
		'anthropic-ratelimit-requests-remaining': {
			text: 'Per-minute rate-limit family — limit / remaining / reset epoch',
			slug: 'rate-limit-headers'
		},
		'anthropic-ratelimit-requests-reset': {
			text: 'Per-minute rate-limit family — limit / remaining / reset epoch',
			slug: 'rate-limit-headers'
		},
		'anthropic-ratelimit-input-tokens-limit': {
			text: 'Per-minute input-token rate-limit family',
			slug: 'rate-limit-headers'
		},
		'anthropic-ratelimit-input-tokens-remaining': {
			text: 'Per-minute input-token rate-limit family',
			slug: 'rate-limit-headers'
		},
		'anthropic-ratelimit-input-tokens-reset': {
			text: 'Per-minute input-token rate-limit family',
			slug: 'rate-limit-headers'
		},
		'anthropic-ratelimit-output-tokens-limit': {
			text: 'Per-minute output-token rate-limit family',
			slug: 'rate-limit-headers'
		},
		'anthropic-ratelimit-output-tokens-remaining': {
			text: 'Per-minute output-token rate-limit family',
			slug: 'rate-limit-headers'
		},
		'anthropic-ratelimit-output-tokens-reset': {
			text: 'Per-minute output-token rate-limit family',
			slug: 'rate-limit-headers'
		},
		'anthropic-ratelimit-tokens-limit': {
			text: 'Per-minute combined-token rate-limit family',
			slug: 'rate-limit-headers'
		},
		'anthropic-ratelimit-tokens-remaining': {
			text: 'Per-minute combined-token rate-limit family',
			slug: 'rate-limit-headers'
		},
		'anthropic-ratelimit-tokens-reset': {
			text: 'Per-minute combined-token rate-limit family',
			slug: 'rate-limit-headers'
		},
		'cf-ray': {
			text: 'Cloudflare diagnostics — Anthropic fronts the API with Cloudflare'
		},
		'cf-cache-status': {
			text: 'Cloudflare diagnostics — Anthropic fronts the API with Cloudflare'
		}
	};

	export function helpForHeader(name: string): HeaderHelp | undefined {
		return KNOWN_HEADERS_HELP[name.toLowerCase()];
	}
</script>

<script lang="ts">
	import Tooltip from '$lib/components/shared/Tooltip.svelte';

	let {
		headers,
		emptyLabel,
		nullLabel
	}: {
		headers: Array<[string, string]> | null;
		emptyLabel: string;
		nullLabel: string;
	} = $props();
</script>

<div class="overflow-auto">
	{#if headers === null}
		<p class="text-xs text-[var(--text-3)]">{nullLabel}</p>
	{:else}
		<table class="w-full border-collapse text-left text-xs">
			<tbody>
				{#if headers.length === 0}
					<tr>
						<td class="px-3 py-2 text-[var(--text-3)]">{emptyLabel}</td>
					</tr>
				{:else}
					{#each headers as [name, value], i (`${name}-${i}`)}
						{@const help = helpForHeader(name)}
						<tr class="border-t border-[var(--border-subtle)]">
							<td class="whitespace-nowrap px-3 py-2 font-mono text-[var(--text-2)]">
								{#if help}
									<Tooltip text={help.text} slug={help.slug}>
										<span>{name}</span>
									</Tooltip>
								{:else}
									{name}
								{/if}
							</td>
							<td class="break-all px-3 py-2 font-mono text-[var(--text-strong)]"
								>{displayHeaderValue(name, value)}</td
							>
						</tr>
					{/each}
				{/if}
			</tbody>
		</table>
	{/if}
</div>
