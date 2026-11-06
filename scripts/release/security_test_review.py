#!/usr/bin/env python3
"""Synthetic/disposable security review tests; no network, real credentials or builds."""

import io
import json
import os
from pathlib import Path
import shutil
import stat
import subprocess
import sys
import tarfile
import tempfile
import unittest
import zipfile

from security_common import ReviewError, Scanner, digest, reference, write_private_json
from security_export import export_check, export_template
from security_review import report
from security_rights import inventory_rights
from security_sources import scan_archive, scan_history, scan_tree

ROOT = Path(__file__).resolve().parents[2]
SECRET = "sk-proj-" + "syntheticOnlyNeverReal1234567890"
PRIVATE_TEXT = "This is private fixture prose that is deliberately not a detector pattern."


class Fixture(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="mike-content-review-fixture-")
        self.addCleanup(self.temp.cleanup)
        self.base = Path(self.temp.name)
        self.repo = self.base / "repo"
        self.repo.mkdir()
        self.git("init", "-q")
        self.git("config", "user.email", "fixture@example.invalid")
        self.git("config", "user.name", "Synthetic fixture")
        self.write("LICENSE", "Synthetic fixture notice.\n")
        self.write("Cargo.lock", 'version = 4\n[[package]]\nname = "fixture"\nversion = "1.0.0"\n')
        self.write("frontend/package.json", '{"dependencies":{"@fontsource-variable/fixture":"1.0.0"}}')
        self.write("frontend/pnpm-lock.yaml", "lockfileVersion: '9.0'\npackages:\n  '@fontsource-variable/fixture@1.0.0':\n    resolution: {integrity: fixture}\n")
        self.commit()

    def write(self, name, text):
        path = self.repo / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(text)
        return path

    def git(self, *args):
        return subprocess.run(["git", "-c", "core.hooksPath=/dev/null", "-c", "commit.gpgsign=false", *args],
                              cwd=self.repo, capture_output=True, check=True).stdout.decode().strip()

    def commit(self):
        self.git("add", ".")
        self.git("commit", "-qm", "Synthetic fixture only")
        return self.git("rev-parse", "HEAD")

    def tar(self, members, name="candidate.tar.gz"):
        path = self.base / name
        with tarfile.open(path, "w:gz") as archive:
            for filename, data, mode in members:
                entry = tarfile.TarInfo(filename)
                entry.size, entry.mode = len(data), mode
                archive.addfile(entry, io.BytesIO(data))
        return path

    def cli(self, *args):
        return subprocess.run([sys.executable, str(ROOT / "scripts/release/security_review.py"),
                               *map(str, args)], capture_output=True, text=True)


class RedactionTests(Fixture):
    def test_ascii_utf16_and_names_never_emit_matches(self):
        scanner = Scanner()
        data = b"\x7fELF\0" + SECRET.encode() + b"\0" + ("postgres://fixture:private@invalid/db " + SECRET).encode("utf-16-le")
        item = scanner.scan(io.BytesIO(data), len(data), "private-" + SECRET + ".png", "fixture")
        self.assertIn("api_token", item["byte_categories"])
        self.assertIn("database_url", item["string_categories"])
        encoded = json.dumps(report(scanner))
        self.assertNotIn(SECRET, encoded)
        self.assertNotIn("fixture:private", encoded)
        self.assertNotIn("private-", encoded)
        self.assertEqual(report(scanner)["PUBLIC01"]["status"], "NOT_RUN")
        self.assertEqual(report(scanner)["PUBLIC02"]["status"], "NOT_RUN")

    def test_chunk_boundary_and_embedded_inventory(self):
        data = b"\0" * (65536 - 8) + SECRET.encode() + b"\0/_app/immutable/app.js\0font.woff2\0sourceMappingURL=app.js.map"
        scanner = Scanner()
        item = scanner.scan(io.BytesIO(data), len(data), "mike", "fixture")
        self.assertIn("api_token", item["byte_categories"])
        self.assertIn("embedded_spa_marker", item["byte_categories"])
        self.assertGreaterEqual(len(item["embedded_resource_refs"]), 3)
        self.assertNotIn("/_app", json.dumps(item))

    def test_private_prose_is_never_certified(self):
        scanner = Scanner()
        scanner.scan(io.BytesIO(PRIVATE_TEXT.encode()), len(PRIVATE_TEXT), "file.txt", "fixture")
        result = report(scanner)
        self.assertFalse(result["qualified"])
        self.assertEqual(result["manual_reviews"]["conversation_and_screenshot_privacy"], "NOT_RUN")
        self.assertNotIn(PRIVATE_TEXT, json.dumps(result))

    def test_bound_and_short_read_are_incomplete(self):
        scanner = Scanner({"file_bytes": 4})
        item = scanner.scan(io.BytesIO(b"not read"), 8, "a", "fixture")
        self.assertEqual(item["status"], "NOT_SCANNED")
        self.assertEqual(scanner.total, 0)
        scanner.scan(io.BytesIO(b"x"), 3, "b", "fixture")
        self.assertEqual(report(scanner)["scan_status"], "INCOMPLETE")

    def test_lfs_payload_is_an_explicit_coverage_gap(self):
        data = b"version https://git-lfs.github.com/spec/v1\noid sha256:" + b"0" * 64 + b"\nsize 123\n"
        scanner = Scanner()
        item = scanner.scan(io.BytesIO(data), len(data), "resource", "fixture")
        self.assertIn("lfs_pointer_payload_not_scanned", item["byte_categories"])
        self.assertIn("lfs_pointer_payload_not_scanned", [item["code"] for item in scanner.gaps])

    def test_lfs_marker_in_source_or_documentation_is_not_a_payload_gap(self):
        marker = b"version https://git-lfs.github.com/spec/v1"
        pointer = marker + b"\noid sha256:" + b"0" * 64 + b"\nsize 123\n"
        cases = [b'pattern = rb"' + marker + b'"\n',
                 b'data = b"' + pointer.replace(b"\n", b"\\n") + b'"\n',
                 b"Example:\n```\n" + pointer + b"```\n",
                 b"\0" * (65536 - 8) + marker + b"\0"]
        for data in cases:
            with self.subTest(size=len(data)):
                scanner = Scanner()
                item = scanner.scan(io.BytesIO(data), len(data), "source.py", "fixture")
                self.assertIn("lfs_pointer_marker", item["byte_categories"])
                self.assertNotIn("lfs_pointer_payload_not_scanned", item["byte_categories"])
                self.assertFalse(scanner.gaps)
                self.assertEqual(report(scanner)["scan_status"], "REVIEW_CANDIDATES")
                self.assertFalse(report(scanner)["qualified"])

    def test_lfs_complete_pointer_variants_keep_the_payload_gap(self):
        oid = b"sha256:" + b"0" * 64
        versions = [b"https://git-lfs.github.com/spec/v1", b"https://hawser.github.com/spec/v1",
                    b"http://git-media.io/v/2"]
        for version in versions:
            pointer = b"version " + version + b"\next-0-fixture " + oid + b"\noid " + oid + b"\nsize 123\n"
            for data in (pointer, pointer.rstrip(b"\n"), pointer.replace(b"\n", b"\r\n")):
                with self.subTest(version=version, size=len(data)):
                    scanner = Scanner()
                    scanner.scan(io.BytesIO(data), len(data), "resource", "fixture")
                    self.assertEqual([item["code"] for item in scanner.gaps],
                                     ["lfs_pointer_payload_not_scanned"])

    def test_lfs_incomplete_or_malformed_pointer_candidates_keep_payload_gaps(self):
        header = b"version https://git-lfs.github.com/spec/v1\n"
        oid = b"oid sha256:" + b"0" * 64 + b"\n"
        cases = [header.rstrip(b"\n"), header, header + oid, header + b"size 123\n",
                 header + b"oid sha256:bad\nsize 123\n",
                 header + oid + b"size -1\n", header + oid + b"size 9223372036854775808\n",
                 header + oid + b"size 123\nsize 456\n", header + oid + b"size 123\n\xff",
                 header + oid + b"size 123\n" + b" " * 1024]
        for data in cases:
            with self.subTest(size=len(data)):
                scanner = Scanner()
                item = scanner.scan(io.BytesIO(data), len(data), "resource", "fixture")
                self.assertIn("lfs_pointer_marker", item["byte_categories"])
                self.assertEqual([item["code"] for item in scanner.gaps],
                                 ["lfs_pointer_payload_not_scanned"])
                self.assertEqual(report(scanner)["scan_status"], "INCOMPLETE")

    def test_lfs_pointer_short_read_keeps_the_scan_incomplete(self):
        data = b"version https://git-lfs.github.com/spec/v1\noid sha256:" + b"0" * 64 + b"\nsize 123\n"
        scanner = Scanner()
        scanner.scan(io.BytesIO(data), len(data) + 1, "resource", "fixture")
        self.assertEqual([item["code"] for item in scanner.gaps],
                         ["short_read", "lfs_pointer_payload_not_scanned"])
        self.assertEqual(report(scanner)["scan_status"], "INCOMPLETE")

    def test_lfs_pointer_fragmented_reads_preserve_detection(self):
        class FragmentedStream(io.BytesIO):
            def read(self, size=-1):
                return super().read(min(size, 7))

        data = b"version https://git-lfs.github.com/spec/v1\noid sha256:" + b"0" * 64 + b"\nsize 123\n"
        scanner = Scanner()
        scanner.scan(FragmentedStream(data), len(data), "resource", "fixture")
        self.assertEqual([item["code"] for item in scanner.gaps], ["lfs_pointer_payload_not_scanned"])

    def test_lfs_header_substrings_do_not_count_as_pointer_header_lines(self):
        marker = b"version https://git-lfs.github.com/spec/v1"
        cases = [marker + b"-not-a-pointer\n", b"See " + marker + b"\n",
                 b'"' + marker + b'"\n']
        for body in cases:
            scanner = Scanner()
            item = scanner.scan(io.BytesIO(body), len(body), "source", "fixture")
            self.assertIn("lfs_pointer_marker", item["byte_categories"])
            self.assertFalse(scanner.gaps)

    def test_lfs_scanner_source_literals_do_not_create_payload_gaps(self):
        scanner = Scanner()
        for name in ("security_common.py", "security_test_review.py"):
            data = (ROOT / "scripts/release" / name).read_bytes()
            scanner.scan(io.BytesIO(data), len(data), name, "fixture")
        self.assertFalse(scanner.gaps)
        self.assertFalse(report(scanner)["qualified"])

    def test_unique_resource_limit_ignores_repeated_reference(self):
        data = b"app.js app.js app.js"
        scanner = Scanner({"resource_refs": 1})
        scanner.scan(io.BytesIO(data), len(data), "fixture", "fixture")
        self.assertFalse(scanner.gaps)
        data = b"app.js other.css"
        scanner.scan(io.BytesIO(data), len(data), "other", "fixture")
        self.assertIn("embedded_resource_reference_limit", [item["code"] for item in scanner.gaps])

    def test_utf16_big_endian_with_odd_alignment(self):
        data = b"x" + ("password=" + SECRET).encode("utf-16-be")
        scanner = Scanner()
        item = scanner.scan(io.BytesIO(data), len(data), "fixture", "fixture")
        self.assertIn("credential_assignment", item["string_categories"])
        self.assertNotIn(SECRET, json.dumps(item))

    def test_private_json_never_overwrites_and_has_restricted_mode(self):
        path = self.base / "report.json"
        write_private_json(path, {"safe": True})
        self.assertEqual(stat.S_IMODE(path.stat().st_mode), 0o600)
        with self.assertRaises(ReviewError):
            write_private_json(path, {"safe": False})
        self.assertTrue(json.loads(path.read_text())["safe"])
        link = self.base / "report-link"
        link.symlink_to(path)
        with self.assertRaises(ReviewError):
            write_private_json(link, {"safe": False})


class SourceTests(Fixture):
    def test_tracked_worktree_only_and_dirty_identity(self):
        self.write("tracked.txt", "initial")
        commit = self.commit()
        self.write("tracked.txt", SECRET)
        self.write("untracked.txt", "postgres://untracked:private@invalid/db")
        scanner = Scanner()
        details = scan_tree(self.repo, scanner)
        self.assertEqual(details["source_commit"], commit)
        self.assertTrue(details["tracked_worktree_dirty"])
        self.assertEqual(details["untracked_and_ignored_files"], "NOT_SCANNED")
        self.assertNotIn(reference("untracked.txt"), json.dumps(scanner.inventory))
        self.assertNotIn(SECRET, json.dumps(scanner.inventory))
        self.assertIn("api_token", report(scanner)["candidate_category_counts"])

    def test_symlink_and_symlink_parent_are_never_followed(self):
        outside = self.base / "outside"
        outside.mkdir()
        (outside / "value").write_text(SECRET)
        self.write("directory/value", "tracked harmless")
        (self.repo / "link").symlink_to(outside / "value")
        self.commit()
        shutil.rmtree(self.repo / "directory")
        (self.repo / "directory").symlink_to(outside, target_is_directory=True)
        scanner = Scanner()
        scan_tree(self.repo, scanner)
        self.assertGreaterEqual(sum(item["code"].endswith("symlink_not_followed") for item in scanner.gaps), 2)
        self.assertNotIn(digest(SECRET.encode()), json.dumps(scanner.inventory))

    def test_history_finds_removed_content_and_names(self):
        self.write("captures/secret-session.log", SECRET)
        self.commit()
        self.git("rm", "-q", "captures/secret-session.log")
        self.commit()
        scanner = Scanner()
        details = scan_history(self.repo, scanner)
        self.assertEqual(details["status"], "SCANNED_LOCAL_REACHABLE")
        self.assertIn("api_token", report(scanner)["candidate_category_counts"])
        self.assertTrue(any(item["scope"] == "history_filename" for item in scanner.inventory))
        self.assertNotIn("secret-session.log", json.dumps(scanner.inventory))
        self.assertNotIn(SECRET, json.dumps(scanner.inventory))

    def test_shallow_history_is_explicitly_incomplete(self):
        head = self.git("rev-parse", "HEAD")
        (self.repo / ".git/shallow").write_text(head + "\n")
        scanner = Scanner()
        details = scan_history(self.repo, scanner)
        self.assertTrue(details["shallow"])
        self.assertEqual(details["status"], "INCOMPLETE")
        self.assertIn("shallow_history", [item["code"] for item in scanner.gaps])

    def test_missing_object_never_becomes_complete_or_fetches(self):
        self.write("gone.txt", SECRET)
        self.commit()
        oid = self.git("rev-parse", "HEAD:gone.txt")
        (self.repo / ".git/objects" / oid[:2] / oid[2:]).unlink()
        scanner = Scanner()
        details = scan_history(self.repo, scanner)
        self.assertEqual(details["status"], "INCOMPLETE")
        self.assertTrue(scanner.gaps)
        self.assertNotIn(SECRET, json.dumps(details))

    def test_history_limit_is_incomplete(self):
        scanner = Scanner({"history_objects": 1})
        details = scan_history(self.repo, scanner)
        self.assertEqual(details["status"], "INCOMPLETE")
        self.assertIn("history_object_limit", [item["code"] for item in scanner.gaps])

    def test_history_ref_names_are_redacted_and_inspected(self):
        self.git("branch", "private/" + SECRET)
        scanner = Scanner()
        scan_history(self.repo, scanner)
        self.assertTrue(any(item["scope"] == "history_ref" and "api_token" in item["byte_categories"]
                            for item in scanner.inventory))
        self.assertNotIn(SECRET, json.dumps(report(scanner)))

    def test_rights_inventory_never_invents_font_permission(self):
        self.write("frontend/static/fixture.woff2", "fixture")
        self.commit()
        scanner = Scanner()
        rights = inventory_rights(self.repo, scanner)
        self.assertEqual(rights["status"], "NOT_RUN")
        self.assertEqual(rights["dependency_count"], 3)
        self.assertTrue(any(item["category"] == "font" for item in rights["dependencies"]))
        self.assertTrue(all(item["rights"] == "UNRESOLVED" for item in rights["dependencies"]))
        self.assertEqual(rights["assets_and_fonts"][0]["notice_mapping"], "NOT_RUN")
        self.assertEqual(rights["notices"][0]["status"], "UNREVIEWED")
        self.assertFalse(rights["gaps"])

    def test_unsupported_dependency_lock_is_not_complete(self):
        self.write("frontend/pnpm-lock.yaml", "lockfileVersion: '99.0'\n")
        scanner = Scanner()
        rights = inventory_rights(self.repo, scanner)
        self.assertTrue(rights["gaps"])
        self.assertEqual(report(scanner)["scan_status"], "INCOMPLETE")


class ArchiveTests(Fixture):
    def test_archive_bytes_members_binary_strings_and_spa(self):
        archive = self.tar([("mike", b"\x7fELF\0" + SECRET.encode() + b"\0app.js\0font.woff2", 0o755),
                            ("assets/app.js.map", b'{"sourcesContent":["synthetic"]}', 0o644),
                            ("LICENSE", b"fixture", 0o644)])
        scanner = Scanner()
        result = scan_archive(archive, scanner)
        self.assertEqual(result["sha256"], digest(archive.read_bytes()))
        self.assertEqual(len([item for item in scanner.inventory if item["scope"] == "archive_member"]), 3)
        self.assertIn("api_token", report(scanner)["candidate_category_counts"])
        self.assertNotIn(SECRET, json.dumps(report(scanner)))
        self.assertFalse((self.base / "mike").exists())

    def test_unsafe_path_and_symlink_never_extract(self):
        archive = self.base / "unsafe.tar"
        with tarfile.open(archive, "w") as tar:
            entry = tarfile.TarInfo("../../escape")
            entry.size = 2
            tar.addfile(entry, io.BytesIO(b"no"))
            link = tarfile.TarInfo("link")
            link.type, link.linkname = tarfile.SYMTYPE, "/outside/" + SECRET
            tar.addfile(link)
        scanner = Scanner()
        scan_archive(archive, scanner)
        codes = {item["code"] for item in scanner.gaps}
        self.assertIn("unsafe_archive_member_path", codes)
        self.assertIn("archive_link_or_special_file", codes)
        self.assertFalse((self.base / "escape").exists())
        self.assertNotIn(SECRET, json.dumps(report(scanner)))

    def test_duplicate_and_nested_archive_mark_incomplete(self):
        archive = self.tar([("same", b"a", 0o644), ("same", b"b", 0o644), ("nested.zip", b"fixture", 0o644)])
        scanner = Scanner()
        scan_archive(archive, scanner)
        codes = {item["code"] for item in scanner.gaps}
        self.assertIn("duplicate_archive_member", codes)
        self.assertIn("nested_container_not_decoded", codes)

    def test_zipapps_are_explicit_nested_coverage_gaps(self):
        for name in ("mike-maintenance.pyz", "helper.pyzw"):
            with self.subTest(name=name):
                archive = self.tar([(name, b"opaque zipapp fixture", 0o644)])
                scanner = Scanner()
                scan_archive(archive, scanner)
                self.assertIn("nested_container_not_decoded", [item["code"] for item in scanner.gaps])

    def test_zip_scan_and_bomb_bound(self):
        path = self.base / "candidate.zip"
        with zipfile.ZipFile(path, "w", compression=zipfile.ZIP_DEFLATED) as archive:
            archive.writestr("payload", SECRET + "x" * 10000)
        scanner = Scanner({"file_bytes": 2000})
        scan_archive(path, scanner)
        self.assertIn("byte_limit", [item["code"] for item in scanner.gaps])
        self.assertNotIn(SECRET, json.dumps(report(scanner)))

    def test_archive_cli_cannot_return_release_pass(self):
        archive = self.tar([("file.txt", b"synthetic harmless", 0o644)])
        result = self.cli("scan", "--artifact-only", "--archive", archive)
        self.assertIn(result.returncode, (0, 1))
        report_data = json.loads(result.stdout)
        self.assertEqual(report_data["PUBLIC01"]["status"], "NOT_RUN")
        self.assertEqual(report_data["tracked_tree"]["status"], "NOT_RUN")

    def test_archive_header_metadata_and_directory_names_are_inspected(self):
        path = self.base / "metadata.tar"
        with tarfile.open(path, "w", format=tarfile.PAX_FORMAT) as archive:
            directory = tarfile.TarInfo(SECRET + "/")
            directory.type = tarfile.DIRTYPE
            archive.addfile(directory)
            entry = tarfile.TarInfo("plain")
            entry.uname = SECRET
            entry.pax_headers = {"comment": "password=" + SECRET}
            archive.addfile(entry, io.BytesIO())
        scanner = Scanner()
        scan_archive(path, scanner)
        self.assertIn("api_token", report(scanner)["candidate_category_counts"])
        self.assertIn("credential_assignment", report(scanner)["candidate_category_counts"])
        self.assertNotIn(SECRET, json.dumps(report(scanner)))

    def test_oversized_extended_header_fails_bounded(self):
        path = self.base / "metadata.tar"
        with tarfile.open(path, "w", format=tarfile.PAX_FORMAT) as archive:
            entry = tarfile.TarInfo("plain")
            entry.pax_headers = {"comment": "x" * 2048}
            archive.addfile(entry, io.BytesIO())
        scanner = Scanner({"archive_metadata_read_bytes": 1024})
        scan_archive(path, scanner)
        self.assertIn("archive_metadata_read_limit", [item["code"] for item in scanner.gaps])

    def test_trailing_tar_content_is_not_approved(self):
        path = self.base / "trailing.tar"
        with tarfile.open(path, "w") as archive:
            archive.addfile(tarfile.TarInfo("file"), io.BytesIO())
        with path.open("ab") as stream:
            stream.write(SECRET.encode())
        scanner = Scanner()
        scan_archive(path, scanner)
        self.assertIn("archive_trailing_data", [item["code"] for item in scanner.gaps])
        self.assertNotIn(SECRET, json.dumps(report(scanner)))

    def test_corrupt_gzip_footer_is_detected(self):
        path = self.tar([("file", b"hello", 0o644)])
        data = bytearray(path.read_bytes())
        data[-8] ^= 1
        path.write_bytes(data)
        scanner = Scanner()
        scan_archive(path, scanner)
        self.assertIn("archive_decode_failed", [item["code"] for item in scanner.gaps])

    def test_zip_symlink_and_comment_are_not_silently_accepted(self):
        path = self.base / "symlink.zip"
        with zipfile.ZipFile(path, "w") as archive:
            archive.comment = SECRET.encode()
            link = zipfile.ZipInfo("link")
            link.create_system = 3
            link.external_attr = (stat.S_IFLNK | 0o777) << 16
            archive.writestr(link, "../../outside")
        scanner = Scanner()
        scan_archive(path, scanner)
        self.assertIn("archive_link_or_special_file", [item["code"] for item in scanner.gaps])
        self.assertIn("api_token", report(scanner)["candidate_category_counts"])
        self.assertNotIn(SECRET, json.dumps(report(scanner)))

    def test_malformed_archive_errors_are_redacted(self):
        archive = self.base / (SECRET + ".tar")
        archive.write_bytes(b"invalid")
        result = self.cli("scan", "--artifact-only", "--archive", archive)
        self.assertEqual(result.returncode, 2)
        self.assertNotIn(SECRET, result.stdout + result.stderr)
        self.assertIn("archive_decode_failed", result.stdout)


class ExportTests(Fixture):
    def template(self):
        return export_template(self.repo, self.git("rev-parse", "HEAD"), ["LICENSE"], Scanner())

    def test_template_is_hash_pinned_and_unapproved(self):
        template = self.template()
        self.assertEqual(template["files"][0]["sha256"], digest((self.repo / "LICENSE").read_bytes()))
        self.assertEqual(template["files"][0]["review"]["privacy"]["status"], "NOT_RUN")
        scanner = Scanner()
        result = export_check(self.repo, template, scanner)
        self.assertEqual(result["mapping_validation"], "PASS")
        self.assertTrue(result["missing_review_claims"])
        self.assertEqual(result["release_approval"], "NOT_RUN")
        self.assertFalse(result["export_written"])

    def test_source_snapshot_does_not_follow_worktree_edits(self):
        template = self.template()
        self.write("LICENSE", "changed working tree only")
        result = export_check(self.repo, template, Scanner())
        self.assertEqual(result["mapping_validation"], "PASS")
        self.assertEqual(result["source_commit"], template["source_commit"])

    def test_modified_hash_duplicate_and_unsafe_paths_rejected(self):
        for change in ("hash", "duplicate", "unsafe", "missing"):
            with self.subTest(change=change):
                template = self.template()
                if change == "hash": template["files"][0]["sha256"] = "0" * 64
                if change == "duplicate": template["files"].append(template["files"][0])
                if change == "unsafe": template["files"][0]["path"] = "../LICENSE"
                if change == "missing": template["files"][0]["path"] = "absent"
                with self.assertRaises(ReviewError):
                    export_check(self.repo, template, Scanner())

    def test_export_rejects_symlink_git_metadata_and_directories(self):
        (self.repo / "link").symlink_to("LICENSE")
        self.commit()
        for name in ("link", ".git/config", "frontend", "*.json"):
            with self.subTest(name=name), self.assertRaises(ReviewError):
                export_template(self.repo, self.git("rev-parse", "HEAD"), [name], Scanner())

    def test_archive_matches_exact_allowlist_and_hashes(self):
        template = self.template()
        template["source_archive_prefix"] = "reviewed-v1/"
        archive = self.tar([("reviewed-v1/LICENSE", (self.repo / "LICENSE").read_bytes(), 0o644)])
        result = export_check(self.repo, template, Scanner(), archive)
        self.assertEqual(result["archive_validation"]["status"], "PASS")
        self.assertEqual(result["archive_validation"]["automatic_host_generated_archive"], "NOT_VERIFIED")
        self.assertEqual(result["release_approval"], "NOT_RUN")

    def test_extra_archive_file_or_mutation_is_rejected(self):
        template = self.template()
        archive = self.tar([("LICENSE", (self.repo / "LICENSE").read_bytes(), 0o644), ("private.txt", b"extra", 0o644)])
        with self.assertRaisesRegex(ReviewError, "membership"):
            export_check(self.repo, template, Scanner(), archive)
        archive = self.tar([("LICENSE", b"changed", 0o644)])
        with self.assertRaisesRegex(ReviewError, "hash"):
            export_check(self.repo, template, Scanner(), archive)

    def test_export_archive_executable_mode_must_match(self):
        template = self.template()
        archive = self.tar([("LICENSE", (self.repo / "LICENSE").read_bytes(), 0o755)])
        with self.assertRaisesRegex(ReviewError, "mode"):
            export_check(self.repo, template, Scanner(), archive)

    def test_template_cli_is_private_and_explicitly_unapproved(self):
        path = self.base / "private-allowlist.json"
        result = self.cli("export-template", "--repo", self.repo, "--source", self.git("rev-parse", "HEAD"),
                          "--path", "LICENSE", "--output", path)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(json.loads(result.stdout)["status"], "UNAPPROVED_TEMPLATE")
        self.assertEqual(stat.S_IMODE(path.stat().st_mode), 0o600)
        self.assertFalse(json.loads(path.read_text())["publication_authorized"])
        result = self.cli("export-check", "--repo", self.repo, "--allowlist", path)
        self.assertEqual(result.returncode, 1, result.stderr)
        self.assertEqual(json.loads(result.stdout)["PUBLIC02"]["status"], "NOT_RUN")

    def test_recorded_approvals_still_cannot_authorize_publication(self):
        template = self.template()
        approved = {"status": "APPROVED", "evidence_reference": "private-review-123"}
        template["files"][0]["review"] = {"privacy": approved, "rights": approved}
        template["approvals"] = {key: approved for key in ("source_owner", "legal", "distribution")}
        template["destination"] = {"repository": "example.invalid/reviewed", "visibility": "public", "access_review": approved}
        template["generated_source_archive_review"] = approved
        result = export_check(self.repo, template, Scanner())
        self.assertFalse(result["missing_review_claims"])
        self.assertEqual(result["recorded_review_claims"], "UNVERIFIED")
        self.assertFalse(result["publication_authorized"])
        self.assertEqual(result["release_approval"], "NOT_RUN")

    def test_cli_invalid_allowlist_does_not_echo_private_content(self):
        path = self.base / "allowlist.json"
        path.write_text('{"secret": "' + SECRET + '"')
        result = self.cli("export-check", "--repo", self.repo, "--allowlist", path)
        self.assertEqual(result.returncode, 2)
        self.assertNotIn(SECRET, result.stdout + result.stderr)
        self.assertNotIn(str(path), result.stderr)


if __name__ == "__main__":
    unittest.main()
