use super::{FeatureOverlays, OverlaySection};

#[derive(Debug, Clone, PartialEq)]
struct Probe {
    limit: usize,
}

impl Default for Probe {
    fn default() -> Self {
        Self { limit: 3 }
    }
}

impl OverlaySection for Probe {}

#[test]
fn overlay_get_missing_returns_default() {
    assert_eq!(FeatureOverlays::default().get::<Probe>(), Probe::default());
}

#[test]
fn overlay_update_is_read_back() {
    let mut overlays = FeatureOverlays::default();
    overlays.update::<Probe>(|probe| probe.limit = 9);
    assert_eq!(overlays.get::<Probe>().limit, 9);
}

#[test]
fn overlay_equality_treats_missing_as_default() {
    let empty = FeatureOverlays::default();
    let mut defaulted = FeatureOverlays::default();
    defaulted.update::<Probe>(|_| {});
    assert_eq!(empty, defaulted);
    let mut changed = FeatureOverlays::default();
    changed.update::<Probe>(|probe| probe.limit = 4);
    assert_ne!(empty, changed);
    assert_ne!(changed, empty);
}
