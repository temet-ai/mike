interface TimelineScrollState {
	activeBandId: number | null;
}

export const __state: TimelineScrollState = {
	activeBandId: null
};

export function resetForRoute(): void {
	__state.activeBandId = null;
}

export function setActiveBandId(id: number | null): boolean {
	if (__state.activeBandId === id) return false;
	__state.activeBandId = id;
	return true;
}

export const timelineScroll = {
	get activeBandId(): number | null {
		return __state.activeBandId;
	}
};
