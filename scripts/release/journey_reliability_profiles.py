"""Explicit reliability profiles and owner-supplied acceptance budgets.

Normal-default execution is deliberately NOT implemented by reusing the disabled
feature workload. Its dependency contract is retained as actionable NOT_RUN
evidence, and admission fails before a database or candidate is started.
"""
import math

from journey_process import require
from journey_reliability_isolation import FEATURES

SYNTHETIC_PROFILE = "synthetic_isolation_overrides"
NORMAL_PROFILE = "normal_defaults"
NORMAL_ENABLED = frozenset({
    "lesson_worker", "harvester", "db_resync", "retention_sweep", "channel_link_sweep",
    "workspace_capture_registration", "fleet_auto_start", "prompt_assistant", "semantic_context",
    "launch_brief", "client_serves_self_identity",
})
BUDGET_FIELDS = ("rss_peak_bytes", "fd_peak", "broadcast_backlog_peak", "active_streams_peak",
                 "drain_seconds", "responsiveness_seconds")


def reliability_options(profile=SYNTHETIC_PROFILE, resource_budgets=None):
    require(profile in (SYNTHETIC_PROFILE, NORMAL_PROFILE), "RELIABILITY_PROFILE_INVALID")
    require(profile != NORMAL_PROFILE or resource_budgets is not None,
            "RELIABILITY_NORMAL_DEFAULT_EXPLICIT_BUDGETS_REQUIRED")
    if resource_budgets is None:
        return {"profile": profile, "resource_budgets": None}
    require(isinstance(resource_budgets, dict) and set(resource_budgets) == set(BUDGET_FIELDS),
            "RELIABILITY_RESOURCE_BUDGET_FIELDS")
    for key in BUDGET_FIELDS[:4]:
        value = resource_budgets[key]
        minimum = 0 if key == "broadcast_backlog_peak" else 1
        require(type(value) is int and minimum <= value <= 2 ** 53,
                "RELIABILITY_RESOURCE_BUDGET_VALUE")
    for key in BUDGET_FIELDS[4:]:
        value = resource_budgets[key]
        require(type(value) in (int, float) and 0 < value <= 30 and math.isfinite(value),
                "RELIABILITY_RESOURCE_BUDGET_VALUE")
    return {"profile": profile, "resource_budgets": dict(resource_budgets)}


def normal_default_contract():
    """Reviewed source facts, not observed candidate defaults or blanket egress claims."""
    from journey_reliability_defaults import default_dependency_contract

    return {
        "state": "NOT_RUN", "implementation_state": "ISOLATED_RUNNER_IMPLEMENTED_UNQUALIFIED",
        "feature_defaults_source": "crates/mike-base/src/feature_toggles/mod.rs",
        "source_reviewed_feature_defaults": {key: key in NORMAL_ENABLED for key in FEATURES},
        "candidate_effective_defaults_verified": False,
        "feature_api_mutations_allowed": False,
        "implemented_offline_contracts": {
            "state": "IMPLEMENTED_OFFLINE", "candidate_runtime_evidence": False,
            "config_renderer": "journey_reliability_defaults.normal_default_config",
            "feature_response_validator": "journey_reliability_defaults.validate_default_features",
        },
        "source_reviewed_dependency_plan": default_dependency_contract(),
        "required_dependency_contracts": [
            {"keys": ["lesson_worker", "harvester", "retention_sweep", "channel_link_sweep"],
             "dependency": "Owned PG19 only; lesson consolidation may ask the embedding service, whose compiled default is disabled.",
             "source": ["crates/mike-lessons/src/lesson_worker/mod.rs", "crates/mike-lessons/src/memory_harvester/mod.rs",
                        "crates/mike/src/retention_worker.rs", "crates/mike-features/src/channel_link_sweep/mod.rs"]},
            {"keys": ["semantic_context"], "dependency": "PG19 and embedding state; the boot loader resets embedding to its compiled disabled default at http://127.0.0.1:7997. TOML cannot relocate that URL; do not enable embedding to make a stub useful.",
             "source": ["crates/mike-knowledge/src/semantic/worker.rs", "crates/mike-base/src/analysis/embeddings/mod.rs",
                        "crates/mike-base/src/config/load/mod.rs"]},
            {"keys": ["db_resync"], "dependency": "Both database URLs must be owned if configured; absent local_database_url legitimately skips resync. Do not disable its toggle.",
             "source": ["crates/mike-features/src/resync/worker.rs", "crates/mike-base/src/config/proxy_config_defaults.rs"]},
            {"keys": ["workspace_capture_registration"], "dependency": "Requires an owned synthetic project marker and durable workspace/session checks; the legacy synthetic Cohort has no project marker; the isolated normal fixture supplies one. No operator project or launch metadata.",
             "source": ["crates/mike-features/src/proxy/engine/mod.rs", "crates/mike-features/src/proxy/engine/workspace_register.rs"]},
            {"keys": ["fleet_auto_start", "prompt_assistant"], "dependency": "Operator-triggered features remain enabled; this workload does not invoke launch/assistant actions and must never expose real CLI credentials.",
             "source": ["crates/mike-launch/src/api/fleet/definitions.rs", "crates/mike-base/src/feature_toggles/tests.rs"]},
            {"keys": ["launch_brief"], "dependency": "Delivers a spawned session's launch brief only; this workload spawns no session, launch or Remote Control stream.",
             "source": ["crates/mike-launch/src/launch/brief_delivery.rs", "crates/mike-launch/src/launch/fleet/brief_version.rs"]},
            {"keys": ["client_serves_self_identity"], "dependency": "Only makes the proxy stand aside from the self-identity reminder when a client serves it; this workload has no such client.",
             "source": ["crates/mike-features/src/reminders/client_context.rs", "crates/mike-base/src/feature_toggles/meta.rs"]},
        ],
        "required_execution": [
            {"code": "ENGINE_VERIFIED_ISOLATED_RUNNER_REQUIRED",
             "detail": "Use the explicit Linux isolated entry with a local digest-pinned trusted image. Actual engine, namespace and cgroup admission must pass before candidate bytes run; private HOME/PATH or caller-provided flags cannot establish isolation."},
            {"code": "NORMAL_DEFAULT_RUNTIME_EVIDENCE_REQUIRED",
             "detail": "The isolated workload implements default checks, trusted project/harvester triggers and durable provenance assertions. Actual candidate execution, real duration and resource evidence remain NOT_RUN until observed; operator-triggered fleet/assistant coverage is separate."},
        ],
        "prohibited_substitutions": ["disable_features", "smoke_run", "paid_provider", "real_credentials",
                                     "stub_latency_as_real_provider_performance"],
        "baseline_comparison": {"state": "NOT_RUN", "reason": "No normal-default candidate workload was admitted."},
    }


def require_normal_default_isolation(report):
    report["normal_default_contract"] = normal_default_contract()
    report["workload_state"] = "NOT_RUN"
    report["host_admission"] = "BLOCKED_DIRECT_HOST_EXECUTION"
    require(False, "RELIABILITY_NORMAL_DEFAULT_ISOLATED_RUNNER_REQUIRED")
