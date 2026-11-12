"""Offline normal-default preparation contracts; no candidate or service runs."""
import copy
import json
from pathlib import Path
import re
import tomllib
import unittest
from unittest.mock import patch

import journey_reliability as reliability
import journey_reliability_defaults as defaults
from journey_process import JourneyError
from journey_reliability_isolation import FEATURES
from journey_reliability_profiles import NORMAL_ENABLED, NORMAL_PROFILE
from test_journey_reliability import OfflineTests
from test_journey_reliability_profiles import BUDGETS

PORTS = dict(zip(defaults.ENDPOINT_ROLES, range(14000, 14006)))
DATABASE = "recovery_fixture_0123456789abcdef_0"
ROOT = Path(__file__).resolve().parents[2]


def feature_response():
    return {"features": [{"key": key, "enabled": key in NORMAL_ENABLED,
                          "scope": scope, "applies": applies}
                         for key, (scope, applies) in FEATURES.items()]}


class DefaultConfigurationTests(OfflineTests):
    def test_configuration_relocates_endpoints_and_preserves_all_other_defaults(self):
        encoded = defaults.normal_default_config(PORTS, DATABASE)
        value = tomllib.loads(encoded.decode("ascii"))
        self.assertEqual(value, {
            "proxy_listen_addr": "127.0.0.1:14000", "inspect_listen_addr": "127.0.0.1:14001",
            "spa_bind": "127.0.0.1:14002",
            "spa_origins": ["http://127.0.0.1:14002", "http://localhost:14002"],
            "upstream_url": "http://127.0.0.1:14003",
            "database_url": f"postgres://postgres@127.0.0.1:14005/{DATABASE}?sslmode=disable",
            "steerer": {"openrouter_base_url": "http://127.0.0.1:14004"},
        })
        for field in ("smoke_run", "local_database_url", "persist_enabled", "db_resync_enabled",
                      "db_fallback_enabled", "max_requests", "max_in_flight", "max_store_bytes",
                      "retention_sweep", "lesson_worker", "harvester", "embedding"):
            self.assertNotIn(field, value)
        self.assertNotIn("enabled", encoded.decode())
        self.assertNotIn("api_key", encoded.decode())
        self.assertEqual(defaults.normal_default_config(PORTS, DATABASE), encoded)

    def test_ports_require_exact_roles_numeric_loopback_ports_and_no_collisions(self):
        invalid = [None, [], {}, dict(PORTS, extra=14007),
                   {key: value for key, value in PORTS.items() if key != "postgres"}]
        invalid.extend(dict(PORTS, proxy=value) for value in
                       (True, 0, 1023, 65536, "14000", 14000.0, "0.0.0.0:14000", PORTS["services"]))
        for ports in invalid:
            with self.subTest(ports=ports), self.assertRaises(JourneyError):
                defaults.normal_default_config(ports, DATABASE)

    def test_database_names_cannot_supply_urls_secrets_options_or_toml(self):
        for database in (None, [], "postgres", "operator", "postgres://user:secret@host/db",
                         DATABASE + "?host=remote", DATABASE + "\nsmoke_run = true", DATABASE + "/x",
                         "recovery_fixture_ABCDEF0123456789_0", "recovery_fixture_0123456789abcdef_0000000"):
            with self.subTest(database=database), self.assertRaisesRegex(JourneyError, "DATABASE_NAME"):
                defaults.normal_default_config(PORTS, database)

    def test_renderer_does_not_change_its_inputs(self):
        ports = copy.deepcopy(PORTS)
        defaults.normal_default_config(ports, DATABASE)
        self.assertEqual(ports, PORTS)


class DefaultObservationTests(OfflineTests):
    def test_complete_unmutated_default_feature_response_is_accepted(self):
        response = feature_response()
        before = copy.deepcopy(response)
        self.assertEqual(defaults.validate_default_features(response),
                         {key: key in NORMAL_ENABLED for key in FEATURES})
        self.assertEqual(response, before)
        response["features"].reverse()
        self.assertEqual(defaults.validate_default_features(response),
                         {key: key in NORMAL_ENABLED for key in FEATURES})

    def test_any_default_change_fails_including_disabled_defaults_becoming_enabled(self):
        for index in range(len(FEATURES)):
            response = feature_response()
            response["features"][index]["enabled"] = not response["features"][index]["enabled"]
            with self.subTest(index=index), self.assertRaisesRegex(JourneyError, "FEATURE_CHANGED"):
                defaults.validate_default_features(response)

    def test_partial_duplicate_extra_and_malformed_feature_observations_fail(self):
        examples = [None, [], {}, {"features": []}, {"features": tuple(feature_response()["features"])},
                    dict(feature_response(), unknown=True)]
        missing = feature_response()
        missing["features"].pop()
        examples.append(missing)
        duplicate = feature_response()
        duplicate["features"][-1] = duplicate["features"][0]
        examples.append(duplicate)
        for field, value in (("key", "unknown"), ("key", []), ("enabled", 1),
                             ("scope", "session_wrong"), ("applies", "immediate_wrong"), ("extra", True)):
            bad = feature_response()
            bad["features"][0][field] = value
            examples.append(bad)
        for response in examples:
            with self.subTest(response=response), self.assertRaises(JourneyError):
                defaults.validate_default_features(response)

    def test_observation_parser_and_config_do_not_open_runtime_admission(self):
        defaults.normal_default_config(PORTS, DATABASE)
        defaults.validate_default_features(feature_response())
        with patch.object(reliability, "verify_binding") as binding, \
             patch.object(reliability, "owned_cluster") as cluster, \
             patch.object(reliability, "owned_child") as child:
            report = reliability.run_reliability_journey(None, {}, None, "",
                profile=NORMAL_PROFILE, resource_budgets=BUDGETS)
        binding.assert_not_called()
        cluster.assert_not_called()
        child.assert_not_called()
        self.assertEqual(report["failure_code"], "RELIABILITY_NORMAL_DEFAULT_ISOLATED_RUNNER_REQUIRED")
        self.assertEqual(report["reliability"]["workload_state"], "NOT_RUN")


class SourceDependencyTests(OfflineTests):
    def test_worker_inventory_covers_all_current_serve_spawns(self):
        source = (ROOT / "crates/mike/src/serve/workers.rs").read_text()
        spawns = re.findall(r"crate::([a-z_:]+)\(state\);", source)
        self.assertEqual(len(spawns), len(set(spawns)))
        self.assertEqual(set(spawns), set(defaults.WORKER_CONDITIONS))
        self.assertIn("launch::fleet::display::spawn_fleet_display", defaults.WORKER_CONDITIONS)
        self.assertIn("proxy::remote_control::spawn_repush_sweep", defaults.WORKER_CONDITIONS)

    def test_dependency_plan_is_only_source_facts_and_fresh_copies(self):
        contract = defaults.default_dependency_contract()
        self.assertEqual(contract["state"], "NOT_RUN")
        self.assertFalse(contract["candidate_defaults_observed"])
        self.assertFalse(contract["endpoint_ownership_observed"])
        self.assertFalse(contract["feature_api_mutations_allowed"])
        self.assertFalse(contract["embedding_service"]["compiled_enabled"])
        self.assertNotIn("PASSED", json.dumps(contract))
        sources = [contract["background_worker_entrypoint"],
                   contract["embedding_service"]["source"], *contract["workspace_registration"]["source"]]
        for path in sources:
            self.assertTrue((ROOT / path).is_file(), path)
        contract["background_worker_conditions"].clear()
        self.assertTrue(defaults.default_dependency_contract()["background_worker_conditions"])

    def test_disabled_embedding_and_absent_secondary_database_are_compiled_defaults(self):
        embedding = (ROOT / "crates/mike/src/analysis/embeddings/mod.rs").read_text()
        self.assertRegex(embedding.split("impl Default for EmbeddingConfig", 1)[1], r"enabled:\s*false")
        config = (ROOT / "crates/mike/src/config/proxy_config_defaults.rs").read_text()
        block = config.split("impl Default for ProxyConfig", 1)[1]
        self.assertRegex(block, r"local_database_url:\s*None")
        self.assertRegex(block, r"db_resync_enabled:\s*true")
        self.assertRegex(block, r"db_fallback_enabled:\s*true")
        self.assertRegex(block, r"smoke_run:\s*false")

    def test_boot_loader_resets_embedding_so_renderer_omits_ineffective_url_override(self):
        contract = defaults.default_dependency_contract()["embedding_service"]
        source = (ROOT / contract["source"]).read_text()
        compiled_url = re.search(r'const DEFAULT_INFINITY_URL: &str = "([^"]+)";', source).group(1)
        self.assertEqual(contract["compiled_url"], compiled_url)
        self.assertEqual(compiled_url, "http://127.0.0.1:7997")
        loader = (ROOT / contract["loader_source"]).read_text()
        reset = "config.embedding = EmbeddingConfig::default();"
        self.assertGreater(loader.index(reset), loader.index("let mut config: ProxyConfig"))
        after_reset = loader.split(reset, 1)[1].split("Ok(config)", 1)[0]
        self.assertNotIn("config.embedding.url", after_reset)
        self.assertTrue(contract["boot_loader_resets_embedding"])
        self.assertFalse(contract["toml_url_override_effective"])
        self.assertNotIn("embedding.url", defaults.ENDPOINT_OVERRIDES)
        self.assertNotIn("embedding", tomllib.loads(defaults.normal_default_config(PORTS, DATABASE).decode()))


if __name__ == "__main__":
    unittest.main()
