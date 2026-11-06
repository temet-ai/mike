use std::collections::{BTreeSet, HashMap};

use super::embedder::cosine;

pub const CLUSTER_VERSION: &str = "cluster-v1";
pub const NEIGHBOURS: usize = 10;

#[derive(Debug, Clone, PartialEq)]
pub struct Member {
    pub item_id: i64,
    pub session_id: i64,
    pub vector: Vec<f32>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Group {
    pub member_item_ids: Vec<i64>,
    pub session_count: usize,
    pub medoid_item_id: i64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GroupRules {
    pub floor: f64,
    pub min_members: usize,
    pub min_sessions: usize,
}

fn top_neighbours(members: &[Member], sims: &[Vec<f64>], i: usize, floor: f64) -> BTreeSet<usize> {
    let mut order: Vec<usize> = (0..members.len())
        .filter(|j| *j != i && sims[i][*j] >= floor)
        .collect();
    order.sort_by(|a, b| sims[i][*b].total_cmp(&sims[i][*a]).then(a.cmp(b)));
    order.into_iter().take(NEIGHBOURS).collect()
}

fn similarity_matrix(members: &[Member]) -> Vec<Vec<f64>> {
    members
        .iter()
        .map(|a| {
            members
                .iter()
                .map(|b| cosine(&a.vector, &b.vector))
                .collect()
        })
        .collect()
}

fn find(parent: &mut [usize], i: usize) -> usize {
    let mut root = i;
    while parent[root] != root {
        root = parent[root];
    }
    parent[i] = root;
    root
}

fn components(n: usize, edges: &[(usize, usize)]) -> Vec<Vec<usize>> {
    let mut parent: Vec<usize> = (0..n).collect();
    for (a, b) in edges {
        let (ra, rb) = (find(&mut parent, *a), find(&mut parent, *b));
        if ra != rb {
            parent[ra] = rb;
        }
    }
    let mut groups: HashMap<usize, Vec<usize>> = HashMap::new();
    for i in 0..n {
        let root = find(&mut parent, i);
        groups.entry(root).or_default().push(i);
    }
    let mut out: Vec<Vec<usize>> = groups.into_values().filter(|g| g.len() > 1).collect();
    out.sort();
    out
}

fn medoid(indices: &[usize], sims: &[Vec<f64>]) -> usize {
    indices
        .iter()
        .copied()
        .max_by(|a, b| {
            let mean = |i: usize| {
                indices
                    .iter()
                    .filter(|j| **j != i)
                    .map(|j| sims[i][*j])
                    .sum::<f64>()
            };
            mean(*a).total_cmp(&mean(*b)).then(b.cmp(a))
        })
        .unwrap_or(indices[0])
}

pub fn mutual_groups(members: &[Member], rules: GroupRules) -> Vec<Group> {
    let sims = similarity_matrix(members);
    let tops: Vec<BTreeSet<usize>> = (0..members.len())
        .map(|i| top_neighbours(members, &sims, i, rules.floor))
        .collect();
    let edges: Vec<(usize, usize)> = (0..members.len())
        .flat_map(|i| {
            tops[i]
                .iter()
                .filter(move |j| **j > i)
                .map(move |j| (i, *j))
        })
        .filter(|(i, j)| tops[*j].contains(i))
        .collect();
    components(members.len(), &edges)
        .into_iter()
        .filter_map(|indices| {
            let sessions: BTreeSet<i64> = indices.iter().map(|i| members[*i].session_id).collect();
            let keep = indices.len() >= rules.min_members && sessions.len() >= rules.min_sessions;
            keep.then(|| Group {
                member_item_ids: indices.iter().map(|i| members[*i].item_id).collect(),
                session_count: sessions.len(),
                medoid_item_id: members[medoid(&indices, &sims)].item_id,
            })
        })
        .collect()
}
