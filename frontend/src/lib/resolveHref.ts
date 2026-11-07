import { resolve } from '$app/paths';
import type { ResolvedPathname, RouteId, RouteParams } from '$app/types';

/** A route with no dynamic segment, such as `/agents` or `/settings/workers`. */
type StaticRouteId = {
	[K in RouteId]: RouteParams<K> extends Record<string, never> ? K : never;
}[RouteId];

/** A parameterless route, optionally followed by a search string or hash. */
export type StaticHref = StaticRouteId | `${StaticRouteId}?${string}` | `${StaticRouteId}#${string}`;

/**
 * Resolve a link that is one of many parameterless routes.
 *
 * `resolve()` types its arguments as one tuple per route, and TypeScript cannot match a
 * union of more than a handful of routes against that set of tuples, so a table of links
 * (the command palette, the home destination cards) cannot call it directly.
 * `StaticHref` already guarantees the route exists and takes no parameters; the one cast
 * below only tells `resolve()` which of its tuple shapes the argument has.
 */
export function resolveStatic(href: StaticHref): ResolvedPathname {
	return resolve(...([href] as Parameters<typeof resolve<'/'>>));
}
