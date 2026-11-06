const state = $state<{ activeBandId: number | null }>({
	activeBandId: null
});

export function resetForRoute(): void {
	state.activeBandId = null;
}

export function setActiveBandId(id: number | null): boolean {
	if (state.activeBandId === id) return false;
	state.activeBandId = id;
	return true;
}

export const timelineScroll = {
	get activeBandId(): number | null {
		return state.activeBandId;
	}
};
