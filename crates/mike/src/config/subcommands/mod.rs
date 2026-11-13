use anyhow::Result;
use clap::Subcommand;

use crate::config::CliOverrides;

mod args;
mod backfill_flow_admission;
mod backfill_predecessors;
mod backfill_session_prompts;
mod decisions;
mod decisions_import;
mod directives;
pub mod dossier_backfill;
mod dossier_backfill_entries;
mod dossier_parity;
mod dossier_parity_asof;
mod dossier_parity_compare;
mod dossier_render;
mod dossier_render_parity;
mod envelopes;
pub mod eval;
mod guard_precheck_replay;
mod guard_report;
mod import_dotenv;
mod lesson_injections;
mod lesson_relations;
mod migrate;
mod origin_backfill;
mod print_config;
mod reclaim_space;
mod semantic;
mod semantic_backtest;
mod sidecar_origins;
mod steer_questions;
mod sweep;
mod system_prompt_identity;
mod tool_definitions;
mod verifier_replay;

pub(super) use args::*;

#[derive(Debug, Clone, Subcommand)]
pub enum ConfigCommand {
    ImportDotenv(import_dotenv::ImportDotenvArgs),
    PrintConfig(PrintConfigArgs),
    MigrateCheck,
    BackfillSessionPrompts(BackfillSessionPromptsArgs),
    BackfillPredecessors,
    BackfillFlowAdmission,
    BackfillToolDefinitions(BackfillToolDefinitionsArgs),
    BackfillRequestEnvelopes(BackfillRequestEnvelopesArgs),
    BackfillRequestSystemPrompts(BackfillRequestSystemPromptsArgs),
    CompactRequestBodies(CompactRequestBodiesArgs),
    ReclassifySystemPrompts(BackfillRequestSystemPromptsArgs),
    RepairSessionPrompts(RepairSessionPromptsArgs),
    RestampSidecarOrigins(RestampSidecarOriginsArgs),
    DemoteDuplicateFirstTurns(RepairSessionPromptsArgs),
    BackfillLessonInjections(BackfillLessonInjectionsArgs),
    ReclaimSpace(ReclaimSpaceArgs),
    DetectLessonRelations(DetectLessonRelationsArgs),
    ForgetRetractedMigrations,
    SweepOrphanedMessages(SweepOrphanedMessagesArgs),
    VerifierReplay(VerifierReplayArgs),
    DossierBackfill(DossierBackfillArgs),
    DossierParity(DossierParityArgs),
    DossierRender(DossierRenderArgs),
    #[command(subcommand)]
    Eval(eval::EvalCommand),
    #[command(subcommand)]
    Semantic(semantic::SemanticCommand),
    #[command(subcommand)]
    Guard(guard_report::GuardCommand),
    #[command(subcommand)]
    Decisions(decisions::DecisionsCommand),
    #[command(subcommand)]
    SteerQuestions(steer_questions::SteerQuestionsCommand),
    #[command(subcommand)]
    Directives(directives::DirectivesCommand),
    OriginBackfill(OriginBackfillArgs),
}

pub async fn run(cmd: ConfigCommand, cli: CliOverrides) -> Result<()> {
    match cmd {
        ConfigCommand::ImportDotenv(args) => import_dotenv::run(args, cli),
        ConfigCommand::PrintConfig(args) => print_config::print_config(args, cli).await,
        ConfigCommand::MigrateCheck => migrate::migrate_check(cli).await,
        ConfigCommand::BackfillSessionPrompts(args) => {
            backfill_session_prompts::backfill_session_prompts(args, cli).await
        }
        ConfigCommand::BackfillPredecessors => {
            backfill_predecessors::backfill_predecessors(cli).await
        }
        ConfigCommand::BackfillFlowAdmission => {
            backfill_flow_admission::backfill_flow_admission(cli).await
        }
        ConfigCommand::BackfillToolDefinitions(args) => {
            tool_definitions::backfill_tool_definitions(args, cli).await
        }
        ConfigCommand::BackfillRequestEnvelopes(args) => {
            envelopes::backfill_request_envelopes(args, cli).await
        }
        ConfigCommand::BackfillRequestSystemPrompts(args) => {
            system_prompt_identity::backfill_request_system_prompts(args, cli).await
        }
        ConfigCommand::CompactRequestBodies(args) => {
            envelopes::compact_request_bodies(args, cli).await
        }
        ConfigCommand::ReclassifySystemPrompts(args) => {
            system_prompt_identity::reclassify_system_prompts(args, cli).await
        }
        cmd => run_maintenance_command(cmd, cli).await,
    }
}

async fn run_maintenance_command(cmd: ConfigCommand, cli: CliOverrides) -> Result<()> {
    match cmd {
        ConfigCommand::RepairSessionPrompts(args) => {
            backfill_session_prompts::repair_session_prompts(args, cli).await
        }
        ConfigCommand::RestampSidecarOrigins(args) => {
            sidecar_origins::restamp_sidecar_origins(args, cli).await
        }
        ConfigCommand::DemoteDuplicateFirstTurns(args) => {
            sidecar_origins::demote_duplicate_first_turns(args, cli).await
        }
        ConfigCommand::BackfillLessonInjections(args) => {
            lesson_injections::backfill_lesson_injections(args, cli).await
        }
        ConfigCommand::ReclaimSpace(args) => reclaim_space::reclaim_space(args, cli).await,
        ConfigCommand::DetectLessonRelations(args) => {
            lesson_relations::detect_lesson_relations(args, cli).await
        }
        ConfigCommand::ForgetRetractedMigrations => migrate::forget_retracted_migrations(cli).await,
        ConfigCommand::SweepOrphanedMessages(args) => {
            sweep::sweep_orphaned_messages(args, cli).await
        }
        ConfigCommand::VerifierReplay(args) => verifier_replay::verifier_replay(args, cli).await,
        cmd => run_judgement_command(cmd, cli).await,
    }
}

async fn run_judgement_command(cmd: ConfigCommand, cli: CliOverrides) -> Result<()> {
    match cmd {
        ConfigCommand::DossierBackfill(args) => dossier_backfill::dossier_backfill(args, cli).await,
        ConfigCommand::DossierParity(args) => dossier_parity::dossier_parity(args, cli).await,
        ConfigCommand::DossierRender(args) => dossier_render::dossier_render(args, cli).await,
        ConfigCommand::Eval(command) => eval::run(command, cli).await,
        ConfigCommand::Semantic(command) => semantic::run(command, cli).await,
        ConfigCommand::Guard(command) => guard_report::run(command, cli).await,
        ConfigCommand::Decisions(command) => decisions::run(command, cli).await,
        ConfigCommand::SteerQuestions(command) => steer_questions::run(command, cli).await,
        ConfigCommand::Directives(command) => directives::run(command, cli).await,
        ConfigCommand::OriginBackfill(args) => origin_backfill::origin_backfill(args, cli).await,
        cmd => unreachable!(
            "run_judgement_command received a variant run() should have handled directly: {cmd:?}"
        ),
    }
}

#[cfg(test)]
mod tests {
    use clap::Parser;

    use super::*;

    #[derive(Debug, Parser)]
    struct TestCli {
        #[command(subcommand)]
        command: ConfigCommand,
    }

    #[test]
    fn backfill_session_prompts_parses_with_default_max_body_bytes() -> Result<()> {
        let cli = TestCli::try_parse_from(["mike", "backfill-session-prompts"])?;
        let ConfigCommand::BackfillSessionPrompts(args) = cli.command else {
            anyhow::bail!("expected BackfillSessionPrompts variant");
        };
        assert_eq!(args.max_body_bytes, DEFAULT_MAX_BODY_BYTES);
        Ok(())
    }

    #[test]
    fn backfill_session_prompts_parses_max_body_bytes_override() -> Result<()> {
        let cli = TestCli::try_parse_from([
            "mike",
            "backfill-session-prompts",
            "--max-body-bytes",
            "123",
        ])?;
        let ConfigCommand::BackfillSessionPrompts(args) = cli.command else {
            anyhow::bail!("expected BackfillSessionPrompts variant");
        };
        assert_eq!(args.max_body_bytes, 123);
        Ok(())
    }

    #[test]
    fn backfill_request_system_prompts_parses_with_default_batch_size() -> Result<()> {
        let cli = TestCli::try_parse_from(["mike", "backfill-request-system-prompts"])?;
        let ConfigCommand::BackfillRequestSystemPrompts(args) = cli.command else {
            anyhow::bail!("expected BackfillRequestSystemPrompts variant");
        };
        assert_eq!(args.batch_size, DEFAULT_SYSTEM_PROMPT_BACKFILL_BATCH_SIZE);
        Ok(())
    }

    #[test]
    fn backfill_request_system_prompts_parses_batch_size_override() -> Result<()> {
        let cli = TestCli::try_parse_from([
            "mike",
            "backfill-request-system-prompts",
            "--batch-size",
            "123",
        ])?;
        let ConfigCommand::BackfillRequestSystemPrompts(args) = cli.command else {
            anyhow::bail!("expected BackfillRequestSystemPrompts variant");
        };
        assert_eq!(args.batch_size, 123);
        Ok(())
    }

    #[test]
    fn sweep_orphaned_messages_defaults_to_dry_run() -> Result<()> {
        let cli = TestCli::try_parse_from(["mike", "sweep-orphaned-messages"])?;
        let ConfigCommand::SweepOrphanedMessages(args) = cli.command else {
            anyhow::bail!("expected SweepOrphanedMessages variant");
        };
        assert!(!args.execute, "default must be dry-run — no --execute flag");
        assert_eq!(args.batch_size, 2000);
        Ok(())
    }

    #[test]
    fn sweep_orphaned_messages_parses_execute_and_batch_size_override() -> Result<()> {
        let cli = TestCli::try_parse_from([
            "mike",
            "sweep-orphaned-messages",
            "--execute",
            "--batch-size",
            "500",
        ])?;
        let ConfigCommand::SweepOrphanedMessages(args) = cli.command else {
            anyhow::bail!("expected SweepOrphanedMessages variant");
        };
        assert!(args.execute);
        assert_eq!(args.batch_size, 500);
        Ok(())
    }

    #[test]
    fn dossier_backfill_defaults_to_dry_run_over_every_stage() -> Result<()> {
        let cli = TestCli::try_parse_from(["mike", "dossier-backfill"])?;
        let ConfigCommand::DossierBackfill(args) = cli.command else {
            anyhow::bail!("expected DossierBackfill variant");
        };
        assert!(!args.execute, "default must be dry-run — no --execute flag");
        assert_eq!(args.stage, "all");
        assert_eq!(args.batch_sessions, 50);
        assert_eq!(args.from, "2026-07-27T00:00:00Z");
        Ok(())
    }

    #[test]
    fn dossier_backfill_accepts_the_steering_facts_stage() -> Result<()> {
        let cli =
            TestCli::try_parse_from(["mike", "dossier-backfill", "--stage", "steering-facts"])?;
        let ConfigCommand::DossierBackfill(args) = cli.command else {
            anyhow::bail!("expected DossierBackfill variant");
        };
        let opts = dossier_backfill::BackfillOptions::from_args(&args)?;
        assert_eq!(opts.stage, dossier_backfill::BackfillStage::SteeringFacts);
        assert!(!opts.execute);
        Ok(())
    }

    #[test]
    fn dossier_backfill_rejects_an_unknown_stage() {
        let result = TestCli::try_parse_from(["mike", "dossier-backfill", "--stage", "everything"]);
        assert!(result.is_err());
    }

    #[test]
    fn dossier_backfill_parses_execute_stage_and_window() -> Result<()> {
        let cli = TestCli::try_parse_from([
            "mike",
            "dossier-backfill",
            "--stage",
            "facts",
            "--execute",
            "--from",
            "2026-09-01T00:00:00Z",
            "--until",
            "2026-09-10T00:00:00Z",
            "--batch-sessions",
            "10",
        ])?;
        let ConfigCommand::DossierBackfill(args) = cli.command else {
            anyhow::bail!("expected DossierBackfill variant");
        };
        assert!(args.execute);
        let opts = dossier_backfill::BackfillOptions::from_args(&args)?;
        assert_eq!(opts.stage, dossier_backfill::BackfillStage::Facts);
        assert_eq!(opts.batch_sessions, 10);
        assert!(opts.until.is_some());
        Ok(())
    }

    #[test]
    fn dossier_backfill_options_reject_a_malformed_timestamp() -> Result<()> {
        let cli = TestCli::try_parse_from(["mike", "dossier-backfill", "--from", "yesterday"])?;
        let ConfigCommand::DossierBackfill(args) = cli.command else {
            anyhow::bail!("expected DossierBackfill variant");
        };
        assert!(dossier_backfill::BackfillOptions::from_args(&args).is_err());
        Ok(())
    }

    #[test]
    fn verifier_replay_defaults_to_dry_run() -> Result<()> {
        let cli = TestCli::try_parse_from(["mike", "verifier-replay", "--set", "items.jsonl"])?;
        let ConfigCommand::VerifierReplay(args) = cli.command else {
            anyhow::bail!("expected VerifierReplay variant");
        };
        assert!(!args.execute, "default must be dry-run — no --execute flag");
        assert_eq!(args.arm, "verifier-input-v1");
        assert_eq!(args.max_usd, DEFAULT_REPLAY_MAX_USD);
        Ok(())
    }

    #[test]
    fn verifier_replay_execute_requires_an_output_file() {
        let result = TestCli::try_parse_from([
            "mike",
            "verifier-replay",
            "--set",
            "items.jsonl",
            "--execute",
        ]);
        assert!(result.is_err());
    }

    #[test]
    fn verifier_replay_rejects_an_unknown_arm() {
        let result = TestCli::try_parse_from([
            "mike",
            "verifier-replay",
            "--set",
            "items.jsonl",
            "--arm",
            "verifier-input-v9",
        ]);
        assert!(result.is_err());
    }

    #[test]
    fn verifier_replay_parity_mode_excludes_execute_and_a_set() -> Result<()> {
        let parity = TestCli::try_parse_from([
            "mike",
            "verifier-replay",
            "--parity-recorded",
            "recorded.jsonl",
            "--out",
            "parity.json",
        ])?;
        assert!(matches!(parity.command, ConfigCommand::VerifierReplay(_)));
        let with_set = TestCli::try_parse_from([
            "mike",
            "verifier-replay",
            "--parity-recorded",
            "recorded.jsonl",
            "--out",
            "parity.json",
            "--set",
            "items.jsonl",
        ]);
        assert!(with_set.is_err());
        let neither = TestCli::try_parse_from(["mike", "verifier-replay"]);
        assert!(neither.is_err());
        Ok(())
    }
}
