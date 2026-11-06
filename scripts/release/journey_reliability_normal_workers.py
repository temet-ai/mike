"""Observe real passive registrations and harvested lessons in the owned database.

Setup uses supported project/location APIs. SQL is read-only and never manufactures
worker output. None of these observations proves every default worker ran.
"""
import hashlib
import json
import time
from urllib.parse import quote

from journey_process import JourneyError, require
from journey_reliability import Budget, CAPTURE_TIMEOUT, MAX_REQUESTS, api_json, owned_database
from journey_wire import exchange

PROJECT_SQL = """SELECT coalesce(json_agg(rows),'[]'::json)::text FROM
 (SELECT p.id,p.project_uuid::text,p.project_path,l.location_uuid::text,
 l.machine_uuid::text,l.root_path,m.name AS machine_name
 FROM projects.locations l JOIN projects.projects p ON p.id=l.project_id
 JOIN projects.machines m ON m.machine_uuid=l.machine_uuid LIMIT 2) rows"""
REGISTRATIONS_SQL = """SELECT coalesce(json_agg(rows ORDER BY session_id),'[]'::json)::text FROM
 (SELECT s.session_uuid::text AS session_id,s.project_id,s.project_resolution,
 s.location_uuid::text,r.project_id AS registration_project_id,r.cwd,r.registered_via,
 r.pid,r.boot_id,r.released_at,r.reaped_at,
 (SELECT count(*) FROM requests.requests q WHERE q.session_id=s.id AND q.project_id IS DISTINCT FROM s.project_id) AS wrong_request_projects
 FROM workspace.session_registrations r JOIN sessions.sessions s ON s.id=r.session_id
 ORDER BY s.session_uuid LIMIT 33) rows"""
LESSONS_SQL = f"""SELECT coalesce(json_agg(rows ORDER BY request_id),'[]'::json)::text FROM
 (SELECT l.id,l.project_id,s.session_uuid::text AS session_id,l.request_id,
 l.topic,l.text,encode(l.text_sha256,'hex') AS text_sha256,k.code AS kind,
 src.code AS source,st.code AS stage,l.source_turn_id,l.agent_id,l.archived_at,
 l.expires_at,l.embedding IS NULL AS embedding_absent,
 q.session_id=s.id AS request_session_matches,q.project_id=l.project_id AS request_project_matches
 FROM memory.lessons l LEFT JOIN sessions.sessions s ON s.id=l.session_id
 LEFT JOIN requests.requests q ON q.id=l.request_id
 JOIN memory.lesson_kinds k ON k.id=l.kind_id
 JOIN memory.lesson_sources src ON src.id=l.source_id
 JOIN memory.lesson_stages st ON st.id=l.stage_id
 ORDER BY l.request_id LIMIT {MAX_REQUESTS + 1}) rows"""


def read_rows(cluster, database, sql):
    owned_database(cluster, database)
    value = json.loads(cluster.sql(database, "BEGIN READ ONLY; SET LOCAL statement_timeout=5000; "
                                  "SET LOCAL lock_timeout=2000; " + sql + "; COMMIT;"))
    require(isinstance(value, list), "NORMAL_DEFAULT_WORKER_ROWS")
    return value


def setup_post(port, path, payload, budget):
    require(path in ("/api/knowledge/projects", "/api/knowledge/locations"),
            "NORMAL_DEFAULT_SETUP_ROUTE")
    body = json.dumps(payload, separators=(",", ":")).encode("ascii")
    raw = (f"POST {path} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\n"
           "Content-Type: application/json\r\nConnection: close\r\n"
           f"Content-Length: {len(body)}\r\n\r\n").encode("ascii") + body
    status, _, response = exchange(port, raw, timeout=min(5, budget.remaining()))
    require(status == 200 and len(response) < 8192, "NORMAL_DEFAULT_SETUP_RESPONSE")
    return json.loads(response)


def setup_project(port, project, budget):
    identity = setup_post(port, "/api/knowledge/projects", {
        "name": "Archive synthetic project", "project_uuid": project["project_uuid"]}, budget)
    require(isinstance(identity, dict) and type(identity.get("id")) is int and identity["id"] > 0
            and identity.get("project_uuid") == project["project_uuid"]
            and identity.get("project_path") == "mike:" + project["project_uuid"],
            "NORMAL_DEFAULT_PROJECT_IDENTITY")
    location = setup_post(port, "/api/knowledge/locations", {
        "project": project["project_uuid"], "machine_uuid": project["machine_uuid"],
        "machine_name": project["machine_name"], "root_path": project["root"]}, budget)
    import uuid
    require(isinstance(location, dict) and location.get("project_uuid") == project["project_uuid"]
            and location.get("project_path") == identity["project_path"]
            and isinstance(location.get("location_uuid"), str), "NORMAL_DEFAULT_LOCATION_IDENTITY")
    require(str(uuid.UUID(location["location_uuid"])) == location["location_uuid"],
            "NORMAL_DEFAULT_LOCATION_IDENTITY")
    project.update(id=identity["id"], path=identity["project_path"], location_uuid=location["location_uuid"])
    return {"supported_api_only": True, "owned_synthetic_location": True,
            "sessions_registered_by_setup": 0, "lessons_written_by_setup": 0}



def project_database(cluster, database, project):
    rows = read_rows(cluster, database, PROJECT_SQL)
    expected = {"id": project["id"], "project_uuid": project["project_uuid"],
                "project_path": project["path"], "location_uuid": project["location_uuid"],
                "machine_uuid": project["machine_uuid"], "root_path": project["root"],
                "machine_name": project["machine_name"]}
    require(rows == [expected], "NORMAL_DEFAULT_PROJECT_DURABILITY")
    return {"owned_project_and_location_durable": True}


def validate_registrations(rows, sessions, project):
    require(len(rows) <= len(sessions), "NORMAL_DEFAULT_REGISTRATION_COUNT")
    observed = set()
    for row in rows:
        require(isinstance(row, dict) and row.get("session_id") in sessions
                and row["session_id"] not in observed, "NORMAL_DEFAULT_REGISTRATION_SESSION")
        require(row.get("project_id") == project["id"]
                and row.get("registration_project_id") == project["id"]
                and row.get("location_uuid") == project["location_uuid"]
                and row.get("project_resolution") == "operator"
                and row.get("cwd") == project["root"] and row.get("registered_via") == "passive"
                and type(row.get("wrong_request_projects")) is int and row["wrong_request_projects"] == 0,
                "NORMAL_DEFAULT_REGISTRATION_PROVENANCE")
        require(all(key in row and row[key] is None for key in
                    ("pid", "boot_id", "released_at", "reaped_at")), "NORMAL_DEFAULT_REGISTRATION_NOT_PASSIVE")
        observed.add(row["session_id"])
    return observed == set(sessions)


def validate_lessons(rows, expected, project):
    require(len(rows) <= len(expected), "NORMAL_DEFAULT_LESSON_COUNT")
    observed, ids = set(), set()
    for row in rows:
        require(isinstance(row, dict) and type(row.get("id")) is int and row["id"] > 0
                and row["id"] not in ids and type(row.get("request_id")) is int
                and row["request_id"] in expected and row["request_id"] not in observed,
                "NORMAL_DEFAULT_LESSON_IDENTITY")
        item = expected[row["request_id"]]
        require(row.get("session_id") == item["session_id"] and row.get("project_id") == project["id"]
                and row.get("topic") == item["topic"] and row.get("text") == item["text"]
                and row.get("text_sha256") == hashlib.sha256(item["text"].strip().lower().encode()).hexdigest()
                and row.get("kind") == "convention" and row.get("source") == "harvester"
                and row.get("stage") == "raw" and row.get("request_session_matches") is True
                and row.get("request_project_matches") is True and row.get("embedding_absent") is True,
                "NORMAL_DEFAULT_LESSON_PROVENANCE")
        require(all(key in row and row[key] is None for key in
                    ("source_turn_id", "agent_id", "archived_at", "expires_at")),
                "NORMAL_DEFAULT_LESSON_UNEXPECTED_MUTATION")
        observed.add(row["request_id"])
        ids.add(row["id"])
    return observed == set(expected)


def worker_database(cluster, database, sessions, expected, project, budget):
    deadline = Budget(min(CAPTURE_TIMEOUT, budget.remaining()))
    while True:
        deadline.now()
        registrations = read_rows(cluster, database, REGISTRATIONS_SQL)
        lessons = read_rows(cluster, database, LESSONS_SQL)
        deadline.now()
        # Registration and host-location binding are separate transactions. A
        # pending partial row may become complete; contradictory final data fails.
        try:
            complete = (validate_registrations(registrations, sessions, project)
                        and validate_lessons(lessons, expected, project))
        except JourneyError:
            if deadline.deadline - time.monotonic() <= 0.1:
                raise
            complete = False
        if complete:
            return {"registrations": len(registrations), "harvested_lessons": len(lessons),
                    "exact_registration_and_lesson_provenance": True,
                    "worker_rows_sha256": hashlib.sha256(json.dumps(
                        [registrations, lessons], sort_keys=True, separators=(",", ":")).encode()).hexdigest()}
        time.sleep(min(0.05, deadline.remaining()))


def worker_api(port, fixtures, project, sessions, budget):
    roster = api_json(port, "/api/workspace/projects/" + quote(project["path"], safe="") + "/roster", budget)
    require(isinstance(roster, dict) and roster.get("project_id") == project["id"]
            and roster.get("project_path") == project["path"] and isinstance(roster.get("roster"), list)
            and len(roster["roster"]) == len(sessions), "NORMAL_DEFAULT_ROSTER_API")
    observed = set()
    for row in roster["roster"]:
        require(isinstance(row, dict) and row.get("session_id") in sessions
                and row["session_id"] not in observed and row.get("cwd") == project["root"]
                and row.get("registered_via") == "passive" and row.get("released_at") is None
                and row.get("channel_present") is False, "NORMAL_DEFAULT_ROSTER_API")
        observed.add(row["session_id"])
    for fixture in fixtures:
        rows = api_json(port, "/api/lessons?project=" + quote(project["path"], safe="")
                        + "&topic=" + fixture.lesson_topic, budget)
        require(isinstance(rows, list) and len(rows) == 1 and isinstance(rows[0], dict)
                and rows[0].get("project_id") == project["id"]
                and rows[0].get("topic") == fixture.lesson_topic and rows[0].get("text") == fixture.lesson_text
                and rows[0].get("kind") == "convention" and rows[0].get("source_turn_id") is None,
                "NORMAL_DEFAULT_LESSON_API")
    return {"roster_api_matches": True, "harvested_lesson_api_matches": True}
