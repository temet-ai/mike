#!/usr/bin/env python3
"""Mocked gh API adapter tests. The gh executable and network never run."""

import hashlib
import json
import os
from pathlib import Path
import subprocess
import tempfile
from types import SimpleNamespace
import unittest
from unittest.mock import patch

from common import ReleaseError
from publication_engine import RemoteFailure
from publication_github import GitHub


class ScriptedRunner:
    def __init__(self, replies):
        self.replies = list(replies)
        self.calls = []

    def __call__(self, args, **kwargs):
        self.calls.append((args, kwargs))
        if not self.replies:
            raise AssertionError("unexpected API call")
        reply = self.replies.pop(0)
        if isinstance(reply, Exception):
            raise reply
        status, value = reply
        raw = value if isinstance(value, bytes) else json.dumps(value).encode()
        if "--include" in args:
            raw = f"HTTP/2.0 {status} Fixture\r\ncontent-type: application/json\r\n\r\n".encode() + raw
        kwargs["stdout"].write(raw)
        return SimpleNamespace(returncode=0 if 200 <= status < 300 else 1)


class GitHubTests(unittest.TestCase):
    def adapter(self, replies):
        runner = ScriptedRunner(replies)
        return GitHub("fixture/releases", runner), runner

    def test_repository_read_uses_fixed_host_and_discards_unrelated_environment(self):
        github, runner = self.adapter([(200, {"full_name": "fixture/releases"})])
        with patch.dict(os.environ, {"GH_HOST": "attacker.invalid", "GH_HTTP_DEBUG": "1",
                                     "MIKE_SECRET": "private", "DATABASE_URL": "private"}):
            self.assertEqual("fixture/releases", github.repository()["full_name"])
        args, kwargs = runner.calls[0]
        self.assertEqual(["gh", "api", "--hostname", "github.com", "--method", "GET", "repos/fixture/releases"], args[:7])
        self.assertNotIn("GH_HOST", kwargs["env"])
        self.assertNotIn("GH_HTTP_DEBUG", kwargs["env"])
        self.assertNotIn("DATABASE_URL", kwargs["env"])
        self.assertNotIn("MIKE_SECRET", kwargs["env"])
        self.assertIn("preexec_fn", kwargs)

    def test_missing_tag_returns_none_no_ref_create(self):
        github, runner = self.adapter([(404, {"message": "not found"})])
        self.assertIsNone(github.tag_commit("v0.1.0"))
        self.assertEqual(1, len(runner.calls))
        self.assertEqual("GET", runner.calls[0][0][5])

    def test_annotated_tag_resolves_only_named_existing_tag(self):
        github, runner = self.adapter([(200, {"ref": "refs/tags/v0.1.0", "object": {"type": "tag", "sha": "a" * 40}}),
                                      (200, {"object": {"type": "commit", "sha": "b" * 40}})])
        self.assertEqual("b" * 40, github.tag_commit("v0.1.0"))
        self.assertTrue(runner.calls[1][0][6].endswith("/git/tags/" + "a" * 40))

    def test_invalid_annotated_sha_cannot_inject_endpoint(self):
        github, runner = self.adapter([(200, {"ref": "refs/tags/v0.1.0", "object": {"type": "tag", "sha": "../other"}})])
        with self.assertRaises(ReleaseError):
            github.tag_commit("v0.1.0")
        self.assertEqual(1, len(runner.calls))

    def test_release_lookup_includes_drafts_and_paginated_results(self):
        first = [{"tag_name": f"v9.{n}.0"} for n in range(100)]
        github, runner = self.adapter([(200, first), (200, [{"tag_name": "v0.1.0", "draft": True, "id": 17}])])
        self.assertEqual(17, github.release("v0.1.0")["id"])
        self.assertIn("page=2", runner.calls[-1][0][6])

    def test_ambiguous_matching_releases_fail_closed(self):
        github, _ = self.adapter([(200, [{"tag_name": "v0.1.0"}, {"tag_name": "v0.1.0"}])])
        with self.assertRaises(ReleaseError):
            github.release("v0.1.0")

    def test_assets_are_paginated(self):
        github, runner = self.adapter([(200, [{"id": n} for n in range(100)]), (200, [{"id": 100}])])
        self.assertEqual(101, len(github.assets(17)))
        self.assertIn("page=2", runner.calls[-1][0][6])

    def test_asset_verification_hashes_body_not_api_metadata(self):
        data = b"same qualified bytes"
        github, runner = self.adapter([(200, data)])
        self.assertEqual(hashlib.sha256(data).hexdigest(), github.asset_sha256(3, len(data)))
        self.assertIn("Accept: application/octet-stream", runner.calls[0][0])
        self.assertNotIn("--include", runner.calls[0][0])

    def test_download_wrong_size_or_denied_fails_without_retry(self):
        for reply, size in (((200, b"changed"), 100), ((403, b"denied"), 6)):
            github, runner = self.adapter([reply])
            with self.subTest(reply=reply), self.assertRaises((ReleaseError, RemoteFailure)):
                github.asset_sha256(3, size)
            self.assertEqual(1, len(runner.calls))

    def test_upload_uses_only_fixed_upload_origin_and_snapshot(self):
        github, runner = self.adapter([(201, {"id": 4})])
        with tempfile.TemporaryDirectory() as temporary:
            path = Path(temporary) / "bytes"
            path.write_bytes(b"archive")
            github.upload(17, "mike+preview.tar.gz", path, hashlib.sha256(b"archive").hexdigest())
            args = runner.calls[0][0]
            self.assertEqual("https://uploads.github.com/repos/fixture/releases/releases/17/assets?name=mike%2Bpreview.tar.gz", args[6])
            self.assertIn(str(path), args)
            self.assertNotIn("--clobber", args)

    def test_snapshot_changed_before_upload_makes_no_request(self):
        github, runner = self.adapter([])
        with tempfile.TemporaryDirectory() as temporary:
            path = Path(temporary) / "bytes"
            path.write_bytes(b"archive")
            with self.assertRaises(ReleaseError):
                github.upload(17, "archive", path, "c" * 64)
        self.assertEqual([], runner.calls)

    def test_denied_api_status_never_retries_or_changes_permissions(self):
        github, runner = self.adapter([(403, {"message": "PRIVATE SERVER CONTENT"})])
        with self.assertRaises(RemoteFailure) as error:
            github.create_draft({"draft": True})
        self.assertNotIn("PRIVATE SERVER CONTENT", str(error.exception))
        self.assertEqual(1, len(runner.calls))

    def test_timeout_is_unknown_outcome_without_automatic_retry(self):
        github, runner = self.adapter([subprocess.TimeoutExpired("gh", 600)])
        with self.assertRaisesRegex(RemoteFailure, "outcome unknown"):
            github.publish(17)
        self.assertEqual(1, len(runner.calls))

    def test_publish_only_changes_draft_and_disables_latest_promotion(self):
        observed = []
        def runner(args, **kwargs):
            path = Path(args[args.index("--input") + 1])
            observed.append(json.loads(path.read_text()))
            kwargs["stdout"].write(b"HTTP/2.0 200 OK\n\n{}")
            return SimpleNamespace(returncode=0)
        GitHub("fixture/releases", runner).publish(17)
        self.assertEqual([{"draft": False, "make_latest": "false"}], observed)

    def test_invalid_repository_cannot_route_elsewhere(self):
        for name in ("https://other.invalid/repo", "../repo", "owner/repo?query=1", "owner/repo/extra"):
            with self.subTest(name=name), self.assertRaises(ReleaseError):
                GitHub(name, lambda *args: self.fail("network"))


if __name__ == "__main__":
    unittest.main()
