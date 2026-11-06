<script lang="ts" module>
	export type CommsAvatarSize = 'message' | 'compact';
</script>

<script lang="ts">
	import { shortSessionId } from '$lib/format';
	import { roleTokens } from '$lib/fleet/roleTokens';

	interface Props {
		sessionUuid: string;
		label: string | null;
		role?: string | null;
		agentType?: string | null;
		size?: CommsAvatarSize;
		decorative?: boolean;
		sender?: 'session' | 'mike';
	}

	const {
		sessionUuid,
		label,
		role = null,
		agentType = null,
		size = 'message',
		decorative = true,
		sender = 'session'
	}: Props = $props();

	const tokens = $derived(
		sender === 'mike'
			? {
					fg: 'var(--ident-mike-fg)',
					bg: 'var(--ident-mike-fill)',
					border: 'var(--ident-mike-line)'
				}
			: roleTokens(role)
	);
	const accessibleName = $derived(label ?? shortSessionId(sessionUuid));

	function firstTwoChars(word: string): string {
		return (word.charAt(0) + word.charAt(1)).toUpperCase();
	}

	function deriveInitials(resolvedLabel: string | null, uuid: string): string {
		const words = (resolvedLabel ?? '')
			.split(/[\s\-_/]+/)
			.map((word) => word.replace(/[^\p{L}\p{N}]/gu, ''))
			.filter((word) => word.length > 0);
		const [first, second] = words;
		if (first !== undefined && second !== undefined) {
			return (first.charAt(0) + second.charAt(0)).toUpperCase();
		}
		if (first !== undefined) {
			return firstTwoChars(first);
		}
		return firstTwoChars(shortSessionId(uuid).replace(/[^\p{L}\p{N}]/gu, ''));
	}

	const initials = $derived(deriveInitials(label, sessionUuid));
</script>

<span
	class="comms-avatar"
	class:comms-avatar--compact={size === 'compact'}
	style:--role-fg={tokens.fg}
	style:--role-bg={tokens.bg}
	style:--role-border={tokens.border}
	data-slot="comms-avatar"
	data-session-uuid={sessionUuid}
	role={decorative ? undefined : 'img'}
	aria-hidden={decorative ? 'true' : undefined}
	aria-label={decorative ? undefined : accessibleName}
	title={decorative ? undefined : accessibleName}
>
	{initials}
</span>

<style>
	.comms-avatar {
		display: inline-flex;
		align-items: center;
		justify-content: center;
		flex: none;
		width: var(--control-h);
		height: var(--control-h);
		border-radius: 4px;
		background: var(--role-bg);
		color: var(--role-fg);
		border: 1px solid var(--role-border);
		font-family: var(--font-mono, monospace);
		font-size: 13px;
		font-weight: 600;
		letter-spacing: 0.02em;
		user-select: none;
	}

	.comms-avatar--compact {
		width: 20px;
		height: 20px;
		border-radius: 3px;
		font-size: 9px;
	}
</style>
