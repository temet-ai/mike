use anyhow::{Result, anyhow};

use super::args_metrics::{EvalAlarmsBacktestArgs, EvalAlarmsListArgs, parse_instant};
use super::rollup::step_instants;
use crate::config::CliOverrides;
use crate::eval_metrics::{resolve_eval_settings, thresholds};
use crate::store::persistence::eval_metrics::{AlarmKind, AlarmState, AlarmsFilter};

pub(super) async fn backtest(args: EvalAlarmsBacktestArgs, cli: CliOverrides) -> Result<()> {
    let kind = AlarmKind::parse(&args.kind)
        .ok_or_else(|| anyhow!("unknown alarm kind {:?}", args.kind))?;
    let from = parse_instant(&args.from).map_err(anyhow::Error::msg)?;
    let to = parse_instant(&args.to).map_err(anyhow::Error::msg)?;
    let persist = super::connect(cli, false).await?;
    let settings = resolve_eval_settings(&persist.eval_setting_rows().await?);
    let steps = step_instants(from, to, &args.step);
    let rows = persist
        .backtest_eval_alarms(kind, &steps, &thresholds(&settings))
        .await?;
    for row in rows {
        println!(
            "{} {} fires={} {}",
            row.at.to_rfc3339(),
            row.subject,
            row.fires,
            serde_json::to_string(&row.stats)?
        );
    }
    Ok(())
}

pub(super) async fn list(args: EvalAlarmsListArgs, cli: CliOverrides) -> Result<()> {
    let persist = super::connect(cli, false).await?;
    let filter = AlarmsFilter {
        state: AlarmState::parse(&args.state).unwrap_or_default(),
        limit: 1_000,
        ..AlarmsFilter::default()
    };
    let (alarms, total) = persist.list_eval_alarms(&filter).await?;
    println!("{total} alarm episode(s)");
    for alarm in alarms {
        println!("{}", serde_json::to_string_pretty(&alarm)?);
    }
    Ok(())
}
