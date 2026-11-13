#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Coverage {
    Tool(&'static str),
    AggregatedInto(&'static str),
    Resource(&'static str),
    Exempt { reason: &'static str },
}

#[derive(Debug, Clone, Copy)]
pub struct RouteEntry {
    pub method: &'static str,
    pub path: &'static str,
    pub coverage: Coverage,
}

pub const fn route(method: &'static str, path: &'static str, coverage: Coverage) -> RouteEntry {
    RouteEntry {
        method,
        path,
        coverage,
    }
}
