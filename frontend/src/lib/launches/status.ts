import { CircleCheck, CircleX, CircleDashed, TriangleAlert, Clock } from '@lucide/svelte';
import type { Component } from 'svelte';
import type { ChannelCheck, LaunchResult, LaunchRow } from '$lib/api/launches';

export type IconComponent = Component<{
	size?: number | string;
	class?: string;
	'aria-hidden'?: 'true' | boolean;
}>;
export type Tone = 'success' | 'warning' | 'danger' | 'info' | 'muted';

export interface StatusDescriptor {
	icon: IconComponent;
	label: string;
	tone: Tone;
	title?: string;
}

export function fact(
	state: 'pass' | 'fail' | 'na',
	label: string,
	title?: string
): StatusDescriptor {
	if (state === 'pass') return { icon: CircleCheck, label, tone: 'success', title };
	if (state === 'fail') return { icon: CircleX, label, tone: 'danger', title };
	return { icon: CircleDashed, label, tone: 'muted', title };
}

export function describeResult(result: LaunchResult): StatusDescriptor {
	switch (result) {
		case 'success':
			return { icon: CircleCheck, label: 'success', tone: 'success' };
		case 'unconfirmed':
			return {
				icon: TriangleAlert,
				label: 'unconfirmed',
				tone: 'warning',
				title:
					'Opened and delivered, but the backend never confirmed the session was actually ready.'
			};
		case 'failed':
			return { icon: CircleX, label: 'failed', tone: 'danger' };
		case 'pending':
		default:
			return {
				icon: Clock,
				label: 'pending',
				tone: 'info',
				title: 'Still in progress — not yet a terminal outcome.'
			};
	}
}

export function describeChannelCheck(value: ChannelCheck | null): StatusDescriptor {
	switch (value) {
		case 'found':
			return fact('pass', 'channel: found');
		case 'timeout':
			return fact(
				'fail',
				'channel: timeout',
				'The launched session never linked a Remote Control channel within the configured budget.'
			);
		case 'db_unreachable':
			return fact(
				'fail',
				'channel: db unreachable',
				'The capture database was unreachable while waiting for the channel.'
			);
		case 'no_session_id':
			return fact(
				'na',
				'channel: n/a',
				'No session id was preassigned for this launch — nothing to wait for.'
			);
		case 'disabled':
			return fact(
				'na',
				'channel: disabled',
				'The channel check is turned off (launch_channel_check_enabled).'
			);
		default:
			return fact(
				'na',
				'channel: not checked',
				'This launch has not reached the channel check yet.'
			);
	}
}

export function describeOpened(row: LaunchRow): StatusDescriptor {
	return row.opened ? fact('pass', 'opened') : fact('na', 'not opened');
}

export function describeDelivered(row: LaunchRow): StatusDescriptor {
	if (row.delivered === null) return fact('na', 'no brief', 'There was no brief to deliver.');
	return row.delivered ? fact('pass', 'delivered') : fact('fail', 'delivery failed');
}

export function describeConfirmed(row: LaunchRow): StatusDescriptor {
	return row.confirmed ? fact('pass', 'confirmed') : fact('na', 'unconfirmed');
}

export function pipelineFacts(row: LaunchRow): StatusDescriptor[] {
	return [
		describeChannelCheck(row.channel_check),
		describeOpened(row),
		describeDelivered(row),
		describeConfirmed(row)
	];
}

export function placementOf(row: LaunchRow): string {
	return row.actual_placement ?? row.requested_placement ?? '—';
}
