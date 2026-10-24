"""Tests for monocode_import.py. Run: python3 -I -m unittest discover -s skills/monocode-import/scripts"""
import base64
import json
import os
import sqlite3
import sys
import tempfile
import unittest

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import monocode_import as mi  # noqa: E402

PNG = base64.b64encode(b"\x89PNG\r\n\x1a\nfake").decode()


def write_jsonl(path, records):
    with open(path, "w", encoding="utf-8") as handle:
        for record in records:
            handle.write(json.dumps(record) + "\n")
    return path


class Fixture(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.dir = self.tmp.name
        self.attachments = mi.Attachments(os.path.join(self.dir, "imported"))

    def tearDown(self):
        self.tmp.cleanup()


class ClaudeTests(Fixture):
    def session(self):
        path = os.path.join(self.dir, "aaaaaaaa-0000-4000-8000-000000000001.jsonl")
        t = "2026-10-23T10:00:00.000Z"
        return write_jsonl(path, [
            {"type": "user", "cwd": "/repo", "timestamp": t, "message": {"role": "user", "content": [
                {"type": "text", "text": "Look at this [Image #1]<system-reminder>hidden</system-reminder>"},
                {"type": "image", "source": {"type": "base64", "media_type": "image/png", "data": PNG}}]}},
            {"type": "assistant", "timestamp": t, "message": {"id": "m1", "content": [{"type": "thinking", "thinking": "Plan it"}]}},
            {"type": "assistant", "timestamp": t, "message": {"id": "m1", "content": [{"type": "text", "text": "Checking."}]}},
            {"type": "assistant", "timestamp": t, "message": {"id": "m1", "content": [
                {"type": "tool_use", "id": "toolu_1", "name": "Bash", "input": {"command": "ls", "description": "List"}}]}},
            {"type": "user", "timestamp": t, "message": {"content": [{"type": "tool_result", "tool_use_id": "toolu_1", "content": "a\nb"}]}},
            {"type": "assistant", "timestamp": t, "message": {"id": "m2", "content": [
                {"type": "tool_use", "id": "toolu_2", "name": "Read", "input": {"file_path": "/repo/x"}}]}},
            {"type": "user", "timestamp": t, "message": {"content": [{"type": "tool_result", "tool_use_id": "toolu_2", "is_error": True, "content": [{"type": "text", "text": "missing"}]}]}},
            {"type": "system", "subtype": "turn_duration", "durationMs": 1234},
            {"type": "user", "isMeta": True, "message": {"content": "meta"}},
            {"type": "user", "timestamp": t, "message": {"content": "<task-notification>done</task-notification>"}},
            {"type": "system", "subtype": "compact_boundary", "timestamp": t},
            {"type": "user", "isCompactSummary": True, "message": {"content": "This session is being continued"}},
            {"type": "user", "timestamp": t, "message": {"content": "<command-name>/review</command-name><command-args>now</command-args>"}},
            {"type": "user", "isSidechain": True, "message": {"content": "subagent prompt"}},
            {"type": "ai-title", "aiTitle": "look-at-image"},
        ])

    def test_converts_messages_tools_and_images(self):
        s = mi.convert_claude(self.session(), self.attachments)
        roles = [b["role"] for b in s.blocks]
        self.assertEqual(roles, ["user", "reasoning", "assistant", "tool", "tool", "system", "user"])
        first = s.blocks[0]
        self.assertEqual(first["text"], "Look at this [Image #1]")
        self.assertEqual(first["durationMs"], 1234)
        self.assertEqual(first["attachments"][0]["kind"], "image")
        bash, read = s.blocks[3]["tool"], s.blocks[4]["tool"]
        self.assertEqual((bash["nativeName"], bash["kind"], bash["title"], bash["detail"], bash["status"]), ("Bash", "execute", "ls", "a\nb", "completed"))
        self.assertEqual(json.loads(bash["nativeInput"])["description"], "List")
        self.assertEqual((read["kind"], read["title"], read["status"], read["detail"]), ("read", "/repo/x", "error", "missing"))
        self.assertEqual(s.blocks[-1]["text"], "/review now")
        self.assertEqual((s.provider_id, s.cwd, s.title), ("aaaaaaaa-0000-4000-8000-000000000001", "/repo", "look-at-image"))


class CodexTests(Fixture):
    def test_converts_items(self):
        path = write_jsonl(os.path.join(self.dir, "rollout-x.jsonl"), [
            {"type": "session_meta", "timestamp": "2026-10-22T10:00:00Z", "payload": {"id": "thread-1", "cwd": "/repo"}},
            {"type": "event_msg", "payload": {"type": "item_completed", "started_at_ms": 1000, "item": {
                "type": "UserMessage", "content": [{"type": "text", "text": "Optional native visuals are available; API reference: \"/x\" visual docs.\n\nFix it"}]}}},
            {"type": "event_msg", "payload": {"type": "item_completed", "started_at_ms": 1100, "item": {"type": "Reasoning", "summary_text": [], "raw_content": []}}},
            {"type": "event_msg", "payload": {"type": "item_completed", "started_at_ms": 1200, "item": {
                "type": "CommandExecution", "id": "c1", "command": ["/bin/zsh", "-lc", "cat a.txt"], "cwd": "file:///repo",
                "parsed_cmd": [{"type": "read", "path": "/repo/a.txt"}], "status": "completed", "exit_code": 1, "aggregated_output": "nope"}}},
            {"type": "event_msg", "payload": {"type": "item_completed", "started_at_ms": 1300, "item": {
                "type": "FileChange", "id": "f1", "changes": {"/repo/a.txt": {"type": "update", "unified_diff": "@@ -1 +1 @@\n-a\n+b\n"}}, "status": "completed"}}},
            {"type": "event_msg", "payload": {"type": "item_completed", "started_at_ms": 1400, "item": {
                "type": "McpToolCall", "id": "m1", "server": "srv", "tool": "go", "arguments": {"x": 1}, "status": "completed",
                "result": {"content": [{"type": "text", "text": "ok"}]}}}},
            {"type": "event_msg", "payload": {"type": "item_completed", "started_at_ms": 1500, "item": {
                "type": "AgentMessage", "content": [{"type": "Text", "text": "Done"}]}}},
            {"type": "event_msg", "timestamp": "1970-01-01T00:00:03Z", "payload": {"type": "task_complete"}},
        ])
        s = mi.convert_codex(path, self.attachments)
        self.assertEqual([b["role"] for b in s.blocks], ["user", "tool", "tool", "tool", "assistant"])
        self.assertEqual(s.blocks[0]["text"], "Fix it")
        self.assertEqual(s.blocks[0]["durationMs"], 2000)
        command = s.blocks[1]["tool"]
        self.assertEqual((command["nativeName"], command["status"], command["detail"]), ("commandExecution", "error", "nope"))
        self.assertEqual(json.loads(command["nativeInput"])["cwd"], "/repo")
        self.assertEqual(json.loads(command["nativeInput"])["command"], "/bin/zsh -lc 'cat a.txt'")
        change = s.blocks[2]["tool"]
        self.assertEqual((change["nativeName"], change["kind"], change["title"]), ("fileChange", "edit", "/repo/a.txt"))
        self.assertIn("+b", change["detail"])
        self.assertEqual((s.blocks[3]["tool"]["nativeName"], s.blocks[3]["tool"]["detail"]), ("srv:go", "ok"))
        self.assertEqual((s.provider_id, s.cwd), ("thread-1", "/repo"))


class PiTests(Fixture):
    def test_follows_the_active_branch(self):
        path = write_jsonl(os.path.join(self.dir, "2026_pi-1.jsonl"), [
            {"type": "session", "id": "pi-1", "cwd": "/repo"},
            {"type": "model_change", "id": "a", "parentId": None, "provider": "prov", "modelId": "m"},
            {"type": "message", "id": "u1", "parentId": "a", "timestamp": "2026-10-23T00:00:00Z", "message": {"role": "user", "content": [{"type": "text", "text": "hi"}], "timestamp": 1000}},
            {"type": "message", "id": "old", "parentId": "u1", "message": {"role": "assistant", "content": [{"type": "text", "text": "abandoned"}]}},
            {"type": "message", "id": "x1", "parentId": "u1", "message": {"role": "assistant", "timestamp": 1500, "durationMs": 500, "content": [
                {"type": "thinking", "thinking": "think"}, {"type": "toolCall", "id": "t1", "name": "bash", "arguments": {"command": "ls"}}]}},
            {"type": "message", "id": "r1", "parentId": "x1", "message": {"role": "toolResult", "toolCallId": "t1", "content": [{"type": "text", "text": "out\n"}]}},
            {"type": "message", "id": "x2", "parentId": "r1", "message": {"role": "assistant", "content": [{"type": "text", "text": "final"}]}},
        ])
        s = mi.convert_pi(path, self.attachments)
        self.assertEqual([(b["role"], b["text"]) for b in s.blocks], [("user", "hi"), ("reasoning", "think"), ("tool", "ls"), ("assistant", "final")])
        self.assertEqual(s.blocks[2]["tool"]["detail"], "out")
        self.assertEqual(s.blocks[0]["durationMs"], 1000)
        self.assertEqual((s.model, s.provider_id), ("pi:prov/m", "pi-1"))


class ImportTests(Fixture):
    def database(self):
        db = os.path.join(self.dir, "monocode.db")
        conn = sqlite3.connect(db)
        conn.execute("""CREATE TABLE sessions (id TEXT PRIMARY KEY, cwd TEXT NOT NULL, harness TEXT NOT NULL, model TEXT NOT NULL,
            model_settings TEXT NOT NULL DEFAULT '{}', runtime_mode TEXT NOT NULL, title TEXT NOT NULL, provider_session_id TEXT,
            blocks_json TEXT NOT NULL DEFAULT '[]', created_at INTEGER NOT NULL, updated_at INTEGER NOT NULL,
            has_user_message INTEGER NOT NULL DEFAULT 0)""")
        conn.commit()
        conn.close()
        workspace = {"projects": ["/other"], "project": "/other"}
        with open(db + ".gpui.json", "w") as handle:
            json.dump({"workspace": json.dumps(workspace), "agent:1": "{}"}, handle)
        return db

    def test_imports_once_backs_up_and_adds_the_project(self):
        db = self.database()
        source = ClaudeTests.session(self)
        mi.main(["import", "--harness", "claude", "--session", source, "--db", db, "--select-project"])
        conn = sqlite3.connect(db)
        row = conn.execute("SELECT cwd, harness, model, runtime_mode, provider_session_id, has_user_message, blocks_json FROM sessions").fetchone()
        conn.close()
        self.assertEqual(row[:6], ("/repo", "claude", "claude:default", "supervised", "aaaaaaaa-0000-4000-8000-000000000001", 1))
        blocks = json.loads(row[6])
        image = blocks[0]["attachments"][0]["path"]
        self.assertTrue(os.path.isfile(image))
        with open(db + ".gpui.json") as handle:
            extras = json.load(handle)
        workspace = json.loads(extras["workspace"])
        self.assertEqual((workspace["projects"], workspace["project"]), (["/other", "/repo"], "/repo"))
        self.assertIn("agent:1", extras)
        self.assertTrue([f for f in os.listdir(self.dir) if f.startswith("monocode.db.bak-")])
        with self.assertRaises(SystemExit):
            mi.main(["import", "--harness", "claude", "--session", source, "--db", db])
        mi.main(["import", "--harness", "claude", "--session", source, "--db", db, "--replace"])
        conn = sqlite3.connect(db)
        self.assertEqual(conn.execute("SELECT count(*) FROM sessions").fetchone()[0], 1)
        conn.close()

    def test_dry_run_writes_nothing(self):
        db = self.database()
        mi.main(["import", "--harness", "claude", "--session", ClaudeTests.session(self), "--db", db, "--dry-run"])
        conn = sqlite3.connect(db)
        self.assertEqual(conn.execute("SELECT count(*) FROM sessions").fetchone()[0], 0)
        conn.close()


if __name__ == "__main__":
    unittest.main()
