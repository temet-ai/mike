#!/usr/bin/env python3
"""Fully mocked Git object adapter; the gh executable and network never run."""

import base64
import copy
import json
from pathlib import Path
import subprocess
from types import SimpleNamespace
import unittest

from common import ReleaseError
from homebrew_github import TapGitHub
from homebrew_proposal import git_blob
from publication_engine import RemoteFailure
from test_publication_github import ScriptedRunner


class HomebrewGitHubTests(unittest.TestCase):
    def adapter(self, replies):
        runner = ScriptedRunner(replies)
        return TapGitHub("fixture/homebrew-tap", runner), runner

    def test_existing_ref_is_read_by_exact_encoded_name(self):
        github, runner = self.adapter([(200, {"ref": "refs/heads/release/main", "object": {"type": "commit", "sha": "a" * 40}})])
        self.assertEqual("a" * 40, github.branch("release/main"))
        self.assertEqual(["gh", "api", "--hostname", "github.com", "--method", "GET",
                          "repos/fixture/homebrew-tap/git/ref/heads/release%2Fmain"], runner.calls[0][0][:7])

    def test_missing_ref_is_not_created(self):
        github, runner = self.adapter([(404, {})])
        self.assertIsNone(github.branch("main"))
        self.assertEqual(1, len(runner.calls))

    def test_wrong_ref_or_object_kind_is_rejected(self):
        for value in ({"ref": "refs/heads/other", "object": {"type": "commit"}},
                      {"ref": "refs/heads/main", "object": {"type": "tag"}}):
            github, runner = self.adapter([(200, value)])
            with self.subTest(value=value), self.assertRaises(ReleaseError):
                github.branch("main")
            self.assertEqual(1, len(runner.calls))

    def test_tree_and_commit_use_only_valid_object_hash_endpoints(self):
        github, runner = self.adapter([(200, {}), (200, {})])
        github.tree("a" * 40)
        github.commit("b" * 40)
        self.assertTrue(runner.calls[0][0][6].endswith("/git/trees/" + "a" * 40 + "?recursive=1"))
        self.assertTrue(runner.calls[1][0][6].endswith("/git/commits/" + "b" * 40))
        for method in (github.tree, github.commit, github.blob):
            with self.assertRaises(ReleaseError):
                method("../other")
        self.assertEqual(2, len(runner.calls))

    def test_blob_decodes_real_bytes_and_verifies_git_identity(self):
        data = b"class Mike < Formula\nend\n"
        sha = git_blob(data)
        reply = {"sha": sha, "size": len(data), "encoding": "base64",
                 "content": base64.encodebytes(data).decode()}
        github, runner = self.adapter([(200, reply)])
        self.assertEqual(data, github.blob(sha))
        self.assertTrue(runner.calls[0][0][6].endswith("/git/blobs/" + sha))

    def test_blob_rejects_bad_encoding_size_bytes_and_metadata(self):
        data = b"formula"
        sha = git_blob(data)
        reply = {"sha": sha, "size": len(data), "encoding": "base64", "content": base64.b64encode(data).decode()}
        for key, value in (("sha", "a" * 40), ("size", 65537), ("size", True), ("size", 1),
                           ("encoding", "utf8"), ("content", "!!!"),
                           ("content", base64.b64encode(b"changed").decode())):
            github, _ = self.adapter([(200, dict(reply, **{key: value}))])
            with self.subTest(key=key, value=value), self.assertRaises(ReleaseError):
                github.blob(sha)

    def test_mutations_send_only_exact_pair_single_parent_and_nonforce_existing_branch(self):
        observed = []
        def runner(args, **kwargs):
            payload = json.loads(Path(args[args.index("--input") + 1]).read_text())
            observed.append((args[5], args[6], payload))
            kwargs["stdout"].write(b'HTTP/2.0 201 Created\n\n{"sha":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"}')
            return SimpleNamespace(returncode=0)
        github = TapGitHub("fixture/homebrew-tap", runner)
        entries = [{"path": "Formula/" + name + ".rb", "mode": "100644", "type": "blob", "content": name}
                   for name in ("mike", "mike-mcp")]
        github.create_tree("b" * 40, entries)
        github.create_commit("a" * 40, "c" * 40, "reviewed marker")
        github.advance_branch("release/main", "a" * 40)
        self.assertEqual({"base_tree": "b" * 40, "tree": entries}, observed[0][2])
        self.assertEqual({"tree": "a" * 40, "parents": ["c" * 40], "message": "reviewed marker"}, observed[1][2])
        self.assertEqual(("PATCH", "repos/fixture/homebrew-tap/git/refs/heads/release%2Fmain",
                          {"sha": "a" * 40, "force": False}), observed[2])
        self.assertFalse(any("/tags/" in endpoint or "/releases" in endpoint for _, endpoint, _ in observed))

    def test_unexpected_formula_payload_never_reaches_network(self):
        entries = [{"path": "Formula/" + name + ".rb", "mode": "100644", "type": "blob", "content": name}
                   for name in ("mike", "mike-mcp")]
        variants = [entries[:1], entries + [entries[0]]]
        for key, value in (("path", "README.md"), ("mode", "100755"), ("type", "tree"),
                           ("content", "x" * 65537), ("url", "https://other.invalid")):
            changed = copy.deepcopy(entries)
            changed[0][key] = value
            variants.append(changed)
        github, runner = self.adapter([])
        for value in variants:
            with self.assertRaises(ReleaseError):
                github.create_tree("a" * 40, value)
        self.assertEqual([], runner.calls)

    def test_denied_or_unknown_mutation_outcomes_stop_without_retry(self):
        for reply in ((403, {"message": "PRIVATE SERVER CONTENT"}), subprocess.TimeoutExpired("gh", 600)):
            github, runner = self.adapter([reply])
            with self.assertRaises(RemoteFailure) as error:
                github.advance_branch("main", "a" * 40)
            self.assertNotIn("PRIVATE SERVER CONTENT", str(error.exception))
            self.assertEqual(1, len(runner.calls))


if __name__ == "__main__":
    unittest.main()
