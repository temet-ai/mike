use chrono::{DateTime, Duration, NaiveTime, Utc};

pub(super) fn cosine_matrix(vectors: &[&[f32]]) -> Vec<Vec<f32>> {
    let n = vectors.len();
    let mut sim = vec![vec![0.0f32; n]; n];
    for i in 0..n {
        sim[i][i] = 1.0;
        for j in (i + 1)..n {
            let dot = vectors[i]
                .iter()
                .zip(vectors[j])
                .map(|(a, b)| a * b)
                .sum::<f32>();
            sim[i][j] = dot;
            sim[j][i] = dot;
        }
    }
    sim
}

pub(super) fn star_groups(sim: &[Vec<f32>], threshold: f32) -> Vec<Vec<usize>> {
    let n = sim.len();
    let near = |i: usize, j: usize| i != j && sim[i][j] >= threshold;
    let mut seeds: Vec<usize> = (0..n).collect();
    let degree: Vec<usize> = (0..n)
        .map(|i| (0..n).filter(|&j| near(i, j)).count())
        .collect();
    seeds.sort_by(|a, b| degree[*b].cmp(&degree[*a]).then(a.cmp(b)));
    let mut unassigned = vec![true; n];
    let mut groups = Vec::new();
    for seed in seeds {
        if !unassigned[seed] || degree[seed] == 0 {
            continue;
        }
        let members: Vec<usize> = (0..n).filter(|&j| unassigned[j] && near(seed, j)).collect();
        if members.is_empty() {
            continue;
        }
        let mut group = vec![seed];
        group.extend(members);
        for &m in &group {
            unassigned[m] = false;
        }
        groups.push(group);
    }
    groups
}

pub(super) fn week_starts(now: DateTime<Utc>, weeks: i64) -> Vec<DateTime<Utc>> {
    let tomorrow = now.date_naive().and_time(NaiveTime::MIN).and_utc() + Duration::days(1);
    (0..weeks)
        .rev()
        .map(|i| tomorrow - Duration::weeks(i + 1))
        .collect()
}

pub(super) fn weekly_counts(ats: &[DateTime<Utc>], starts: &[DateTime<Utc>]) -> Vec<u32> {
    let mut counts = vec![0u32; starts.len()];
    for at in ats {
        let slot = starts.iter().rposition(|start| at >= start);
        if let Some(i) = slot
            && *at < starts[i] + Duration::weeks(1)
        {
            counts[i] += 1;
        }
    }
    counts
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn unit(x: f32, y: f32) -> Vec<f32> {
        let norm = (x * x + y * y).sqrt();
        vec![x / norm, y / norm]
    }

    #[test]
    fn cosine_matrix_is_symmetric_with_unit_diagonal() {
        let a = unit(1.0, 0.0);
        let b = unit(1.0, 1.0);
        let sim = cosine_matrix(&[a.as_slice(), b.as_slice()]);
        assert!((sim[0][0] - 1.0).abs() < 1e-6);
        assert!((sim[0][1] - sim[1][0]).abs() < 1e-6);
        assert!((sim[0][1] - std::f32::consts::FRAC_1_SQRT_2).abs() < 1e-3);
    }

    #[test]
    fn star_groups_take_members_near_the_seed_and_never_chain_past_it() {
        let sim = vec![
            vec![1.0, 0.9, 0.5, 0.1],
            vec![0.9, 1.0, 0.9, 0.1],
            vec![0.5, 0.9, 1.0, 0.1],
            vec![0.1, 0.1, 0.1, 1.0],
        ];
        let groups = star_groups(&sim, 0.8);
        assert_eq!(groups, vec![vec![1, 0, 2]]);
    }

    #[test]
    fn star_groups_leave_singletons_out() {
        let sim = vec![vec![1.0, 0.2], vec![0.2, 1.0]];
        assert!(star_groups(&sim, 0.8).is_empty());
    }

    #[test]
    fn week_starts_end_with_the_week_that_holds_today() {
        let now = Utc.with_ymd_and_hms(2026, 10, 16, 14, 0, 0).unwrap();
        let starts = week_starts(now, 2);
        assert_eq!(
            starts,
            vec![
                Utc.with_ymd_and_hms(2026, 10, 3, 0, 0, 0).unwrap(),
                Utc.with_ymd_and_hms(2026, 10, 10, 0, 0, 0).unwrap(),
            ]
        );
        assert!(now < starts[1] + Duration::weeks(1));
    }

    #[test]
    fn weekly_counts_put_each_message_in_its_week_and_drop_older_ones() {
        let now = Utc.with_ymd_and_hms(2026, 10, 16, 14, 0, 0).unwrap();
        let starts = week_starts(now, 2);
        let ats = vec![
            Utc.with_ymd_and_hms(2026, 10, 1, 9, 0, 0).unwrap(),
            Utc.with_ymd_and_hms(2026, 10, 4, 9, 0, 0).unwrap(),
            Utc.with_ymd_and_hms(2026, 10, 16, 9, 0, 0).unwrap(),
            Utc.with_ymd_and_hms(2026, 10, 15, 9, 0, 0).unwrap(),
        ];
        assert_eq!(weekly_counts(&ats, &starts), vec![1, 2]);
    }
}
