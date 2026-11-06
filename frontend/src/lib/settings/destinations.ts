import {
	Settings,
	Radar,
	Cpu,
	Bot,
	Boxes,
	ScanText,
	Plug,
	History,
	ListChecks
} from '@lucide/svelte';

export const SETTINGS_DESTINATIONS = [
	{ href: '/settings', label: 'General', icon: Settings },
	{ href: '/settings/steering', label: 'Steering', icon: Radar },
	{ href: '/settings/workers', label: 'Workers', icon: Cpu },
	{ href: '/settings/assistants', label: 'Assistants', icon: Bot },
	{ href: '/settings/embedding', label: 'Embedding', icon: Boxes },
	{ href: '/settings/flagging', label: 'Flagging', icon: ScanText },
	{ href: '/settings/integrations', label: 'Integrations', icon: Plug },
	{ href: '/settings/setup', label: 'Setup', icon: ListChecks },
	{ href: '/settings/history', label: 'History', icon: History }
] as const;
