import type { RepeatGroup, RepeatItem, RepeatsResponse } from '$lib/api/improvement';

/*
 * "What you keep saying" fixtures: four groups taken from the captured data of 2026-10-16, with
 * made-up session ids and the short variants of each message written in the same voice.
 * `weekly` is worked out from the item dates, so the bars and the items always agree.
 */

export const REPEAT_WEEK_STARTS = [
	'2026-07-25',
	'2026-08-01',
	'2026-08-08',
	'2026-08-15',
	'2026-08-22',
	'2026-08-29',
	'2026-09-05',
	'2026-09-12',
	'2026-09-19',
	'2026-09-26',
	'2026-10-03',
	'2026-10-10'
];

const WEEK_MS = 7 * 86_400_000;
const FIRST_WEEK_MS = Date.parse(`${REPEAT_WEEK_STARTS[0]}T00:00:00Z`);

function weeklyOf(items: RepeatItem[]): number[] {
	const weekly = REPEAT_WEEK_STARTS.map(() => 0);
	for (const item of items) {
		const index = Math.floor((Date.parse(item.at) - FIRST_WEEK_MS) / WEEK_MS);
		if (index >= 0 && index < weekly.length) weekly[index]! += 1;
	}
	return weekly;
}

function group(id: number, live: boolean, projects: string[], items: RepeatItem[]): RepeatGroup {
	return {
		id,
		n: items.length,
		first_at: items[0]!.at,
		last_at: items.at(-1)!.at,
		projects,
		weekly: weeklyOf(items),
		live,
		items
	};
}

function item(turn_id: number, session_id: string, at: string, project: string, text: string) {
	return { turn_id, session_id, at, project, text } satisfies RepeatItem;
}

const COORDINATE = group(
	18999,
	false,
	['(no project)', 'gg', 'holdfast', 'night-watch-notes', 'night-watch', 'temet-proxy'],
	[
		item(
			18999,
			'5d1c7a0e-3b52-4f0b-9c61-2a8e4d7f1b03',
			'2026-07-29T10:12:00Z',
			'temet-proxy',
			'coordinate with that other session to make sure you are both aware of each other'
		),
		item(
			19240,
			'a3f09b64-7c18-4e2d-8b55-6d02c9e41f7a',
			'2026-08-06T14:40:00Z',
			'gg',
			'check the session d938444e-acbc-4647-883e-3b5c9c4dcd5b'
		),
		item(
			19611,
			'e84d2c17-0a9b-4635-b3f8-91c5d7a60e22',
			'2026-08-14T09:05:00Z',
			'night-watch',
			'talk to the other session before you touch that'
		),
		item(
			19902,
			'1b6e93d5-f240-4a8c-9d17-5e3a08c4b6f9',
			'2026-08-21T16:22:00Z',
			'holdfast',
			'just coordinate with session fbd394de-680d-4b76-8ca9-af59ad90df58 so that we can get the next set of work done'
		),
		item(
			20177,
			'c97a41f8-2d6e-4b03-a5c9-0f8e7d1b3a64',
			'2026-08-29T11:48:00Z',
			'(no project)',
			'are you talking to the other session about this'
		),
		item(
			20490,
			'3e0f5b72-9c14-48d6-b2a7-d4618e5f0c93',
			'2026-09-05T13:30:00Z',
			'night-watch-notes',
			'tell the other session what you are changing'
		),
		item(
			20733,
			'f2a8d6c0-5b19-4e37-8c42-7a91e3d05b68',
			'2026-09-12T08:55:00Z',
			'temet-proxy',
			'coordinate with the other session, you are both editing the same files'
		),
		item(
			20961,
			'7c4e1a95-d803-4f2b-b6a0-3e59c2d8f714',
			'2026-09-19T15:10:00Z',
			'gg',
			'make sure the other session knows what you did'
		),
		item(
			21204,
			'0d93b7e6-a41c-4580-9f2e-c8b6054a7d31',
			'2026-09-26T10:36:00Z',
			'night-watch',
			'you need to coordinate with the sessions that are working next to you'
		),
		item(
			21558,
			'b5e2f093-6d7a-41c8-a3e4-19d0c7b8265f',
			'2026-09-30T12:02:00Z',
			'temet-proxy',
			'message the other session and wait for it to answer'
		)
	]
);

const GET_WORK_DONE = group(
	20344,
	false,
	['(no project)', 'night-watch', 'team-2', 'temet-proxy'],
	[
		item(
			19120,
			'6a2d8f41-b093-4c7e-85d1-e4f6a0c93b27',
			'2026-08-07T09:20:00Z',
			'temet-proxy',
			'commit all the work that has been done'
		),
		item(
			19377,
			'94c1e0b7-3f58-4a6d-b2e9-0d7a5c18f346',
			'2026-08-12T14:11:00Z',
			'team-2',
			'get the work done, committed and pushed.'
		),
		item(
			19640,
			'd0b7a3e5-8c29-4f14-9a63-5e12f8c7b094',
			'2026-08-18T10:45:00Z',
			'night-watch',
			'jesus, just get the work done'
		),
		item(
			19855,
			'28f6c9d1-e374-4b0a-a7c5-1b9e3d60f482',
			'2026-08-25T16:30:00Z',
			'(no project)',
			'why is this not commited yet'
		),
		item(
			20061,
			'e1a5b8f3-0c76-4d92-b4e8-7f23a9c0d615',
			'2026-08-31T08:58:00Z',
			'temet-proxy',
			'commit the work'
		),
		item(
			20288,
			'5b90d2c4-a8e1-4673-8f0b-c3d7e1a69f58',
			'2026-09-04T13:14:00Z',
			'night-watch',
			'just finish it and commit'
		),
		item(
			20517,
			'b3c7e6a2-19d4-4f85-a0b3-8e5f2c47d190',
			'2026-09-10T11:27:00Z',
			'team-2',
			'stop asking and get the work done'
		),
		item(
			20744,
			'0f4a2e8b-d615-4c39-9b7a-e68c1d05f3a2',
			'2026-09-15T15:49:00Z',
			'temet-proxy',
			'commit and push, how many times do i have to say it'
		),
		item(
			20972,
			'c68d1f05-7a3b-4e92-b8c4-2a5e90d7f163',
			'2026-09-19T09:33:00Z',
			'(no project)',
			'get it done'
		),
		item(
			21190,
			'7e2b94a8-f0c5-43d1-a6e7-d91f3b058c24',
			'2026-09-24T12:41:00Z',
			'night-watch',
			'push it. dont ask me'
		),
		item(
			21416,
			'a9d3c0f6-5e82-4b17-9c04-6f8b2e1a7d35',
			'2026-09-27T10:09:00Z',
			'temet-proxy',
			'can you seriously stop with this bs one more thing. get the work done'
		),
		item(
			21602,
			'3c5f8e71-b2a4-4d09-8e6c-a0d4719b5f82',
			'2026-09-29T14:56:00Z',
			'team-2',
			'commit all of it'
		)
	]
);

const ARTIFACT = group(
	21876,
	true,
	['(no project)', 'company', 'temet-proxy'],
	[
		item(
			19480,
			'f7b1d4a9-6e03-4c58-a2d7-0c93e5b8f146',
			'2026-08-17T09:41:00Z',
			'company',
			'create a claude artifact that i can read.'
		),
		item(
			20105,
			'4a8e0c63-d9f2-4b71-b5a8-e17d3c60942f',
			'2026-08-27T13:18:00Z',
			'temet-proxy',
			'create an artifact for me that i can view'
		),
		item(
			20620,
			'e5d92b0a-3c78-4f14-86a1-7b04f9d3c258',
			'2026-09-08T11:05:00Z',
			'(no project)',
			'put it in an artifact so i can read it'
		),
		item(
			21033,
			'12c6f8d4-a0b9-4e35-9d72-c58e1a3b7064',
			'2026-09-21T15:37:00Z',
			'company',
			'make it an artifact'
		),
		item(
			21390,
			'8d04a7e1-f563-4b2c-a9c8-30e6d1f58b97',
			'2026-10-02T10:24:00Z',
			'temet-proxy',
			'where is the design artifact that i asked for?'
		),
		item(
			21745,
			'c3a95e7f-1d68-40b4-8e2a-b7f0d46c9135',
			'2026-10-09T14:52:00Z',
			'temet-proxy',
			'is it possible to make that artifact awesome and also put a something like an editor in it, with a live way to see the code and talk about it.'
		),
		item(
			21860,
			'96f1b3d8-e4a7-4c20-b5d9-02a8c7e3f651',
			'2026-10-15T09:13:00Z',
			'(no project)',
			'i still cant read this, artifact please'
		)
	]
);

const COMMIT_AND_PUSH = group(
	22410,
	true,
	['(no project)', 'night-watch', 'temet-proxy'],
	[
		item(
			19001,
			'0b7e5d29-a8c3-4f61-9e04-d3f2a61b8c75',
			'2026-07-29T15:03:00Z',
			'temet-proxy',
			'commit and push everything.'
		),
		item(
			20830,
			'd6a2f094-7b15-4e83-a0c6-59e1b3d8f270',
			'2026-09-02T10:50:00Z',
			'night-watch',
			'commit and push everything and make sure that it is on main'
		),
		item(
			21277,
			'3f8c1e5a-c940-4b27-b6d3-a28e07f4d519',
			'2026-09-23T13:25:00Z',
			'(no project)',
			'push it to main'
		),
		item(
			21801,
			'a14b7d90-2e6f-4c35-8b1a-f90c53e6d782',
			'2026-10-12T09:44:00Z',
			'temet-proxy',
			'commit and push the work so that it can be versioned and sent. stop hedging and continue'
		),
		item(
			22398,
			'e8c05a36-b1d7-4f92-a4e0-6c3d9f12b087',
			'2026-10-16T08:31:00Z',
			'night-watch',
			'commit and push'
		)
	]
);

export const REPEATS: RepeatsResponse = {
	weeks: 12,
	threshold: 0.84,
	min_count: 3,
	turns: 1745,
	computed_at: '2026-10-16T09:00:00Z',
	week_starts: REPEAT_WEEK_STARTS,
	groups: [GET_WORK_DONE, COORDINATE, ARTIFACT, COMMIT_AND_PUSH]
};

/** A fixed "now" for stories and tests, so "last said" reads the same on every run. */
export const REPEATS_NOW = Date.parse('2026-10-16T12:00:00Z');
