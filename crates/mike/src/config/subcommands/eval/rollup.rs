use anyhow::{Context, Result};
use chrono::{Duration, NaiveDate, Utc};

use super::args_metrics::{EvalRatesArgs, EvalRollupArgs};
use crate::config::CliOverrides;
use crate::eval_metrics::EVAL_ROLLUP_SQL_VERSION;
use crate::store::persistence::eval_metrics::{EvalRateRow, Family, RatesFilter, RollupRequest};

fn days(args: &EvalRollupArgs) -> Vec<NaiveDate> {
    match (args.day, args.from, args.to) {
        (Some(day), _, _) => vec![day],
        (None, Some(from), Some(to)) => from.iter_days().take_while(|d| *d <= to).collect(),
        _ => vec![Utc::now().date_naive()],
    }
}

pub(super) async fn rollup(args: EvalRollupArgs, cli: CliOverrides) -> Result<()> {
    let persist = super::connect(cli, true).await?;
    let mode = if args.day.is_some() {
        "manual"
    } else {
        "backfill"
    };
    for day in days(&args) {
        let report = persist
            .run_eval_rollup(RollupRequest {
                for_day: day,
                mode,
                sql_version: EVAL_ROLLUP_SQL_VERSION,
                execute: args.execute,
            })
            .await
            .with_context(|| format!("eval rollup for {day} failed"))?;
        if args.execute {
            let rows = super::rollup_improvement::rollup_action_effects(&persist, day)
                .await
                .with_context(|| format!("action-effect rollup for {day} failed"))?;
            println!("{day}: wrote {rows} action_effect row(s)");
        }
        if report.already_rolled {
            println!("{day}: already rolled at {EVAL_ROLLUP_SQL_VERSION}, skipped");
            continue;
        }
        let verb = if report.executed {
            "wrote"
        } else {
            "would write"
        };
        for (family, rows) in &report.per_family {
            println!("{day}: {verb} {rows} {family} row(s)");
        }
        if args.verbose && !report.executed {
            let rows = persist
                .eval_rates_adhoc(Family::Accuracy, None, crate::eval_metrics::day_start(day))
                .await?;
            print_rows(&rows, false)?;
        }
    }
    Ok(())
}

fn family(name: &str) -> Family {
    match name {
        "label_mix" => Family::LabelMix,
        "base_rate" => Family::BaseRate,
        "steer_calibration" => Family::SteerCalibration,
        _ => Family::Accuracy,
    }
}

fn keep(row: &EvalRateRow, args: &EvalRatesArgs) -> bool {
    args.role.as_deref().is_none_or(|r| row.role == r)
        && args.class.as_deref().is_none_or(|c| row.class == c)
        && args
            .label_source
            .as_deref()
            .is_none_or(|s| row.label_source.as_deref() == Some(s))
}

pub(super) fn print_rows(rows: &[EvalRateRow], json: bool) -> Result<()> {
    if json {
        println!("{}", serde_json::to_string_pretty(rows)?);
        return Ok(());
    }
    for r in rows {
        println!(
            "{} {} {} role={} axis={} class={} tier={:?} split={:?} source={:?} bundle={:?} dim={:?}={:?} k={} n={} value={:?} ci=[{:?}, {:?}] weighted={} n_eff={:?}",
            r.family,
            r.metric,
            r.population,
            r.role,
            r.axis,
            r.class,
            r.label_tier,
            r.label_split,
            r.label_source,
            r.bundle_label,
            r.dimension,
            r.dimension_value,
            r.k,
            r.n,
            r.value,
            r.ci_low,
            r.ci_high,
            r.weighted,
            r.n_effective
        );
    }
    Ok(())
}

pub(super) async fn rates(args: EvalRatesArgs, cli: CliOverrides) -> Result<()> {
    let adhoc = args.from.is_some() || args.to.is_some();
    let persist = super::connect(cli, adhoc).await?;
    let rows = if adhoc {
        let end = args.to.unwrap_or_else(Utc::now);
        persist
            .eval_rates_adhoc(family(&args.family), args.from, end)
            .await?
    } else {
        let filter = RatesFilter {
            family: Some(args.family.clone()),
            role: args.role.clone(),
            class: args.class.clone(),
            label_source: args.label_source.clone(),
            latest: true,
            limit: 1_000,
            ..RatesFilter::default()
        };
        persist.list_eval_rates(&filter).await?.0
    };
    let kept: Vec<EvalRateRow> = rows.into_iter().filter(|r| keep(r, &args)).collect();
    print_rows(&kept, args.json)
}

pub(super) fn step_instants(
    from: chrono::DateTime<Utc>,
    to: chrono::DateTime<Utc>,
    step: &str,
) -> Vec<chrono::DateTime<Utc>> {
    let delta = if step == "1h" {
        Duration::hours(1)
    } else {
        Duration::days(1)
    };
    let mut out = Vec::new();
    let mut at = from;
    while at <= to {
        out.push(at);
        at += delta;
    }
    out
}
