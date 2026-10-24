#!/usr/bin/env python3
"""Import Claude Code, Codex and Pi sessions into MonoCode.

The converted history is written in the block format MonoCode itself stores
(`sessions.blocks_json`), and `provider_session_id` points at the original
session so the next message resumes it. Standard library only (Python 3.9+).
"""
import argparse
import base64
import datetime
import glob
import hashlib
import json
import os
import re
import shlex
import shutil
import sqlite3
import subprocess
import sys
import uuid

HOME = os.path.expanduser("~")
RELEASE_DB = os.path.join(HOME, "Library/Application Support/com.monocode.desktop/monocode.db")
DEV_DB = os.path.join(HOME, "Library/Application Support/Monocode Development/monocode.db")
CLAUDE_ROOT = os.path.join(HOME, ".claude/projects")
CODEX_ROOT = os.path.join(HOME, ".codex/sessions")
PI_ROOT = os.path.join(HOME, ".pi/agent/sessions")
HARNESSES = ("claude", "codex", "pi")
OUTPUT_LIMIT = 64 * 1024  # monocode-core ToolCall::bound_output
REMINDER = re.compile(r"<system-reminder>.*?</system-reminder>\s*", re.S)
# MonoCode prepends this to prompts it sends (crates/monocode-gpui/src/runtime/visual_discovery.rs).
VISUALS = re.compile(r"^Optional native visuals are available; API reference: .*? visual docs\.\n\n", re.S)


def fail(message):
    print("error: " + message, file=sys.stderr)
    sys.exit(1)


def new_id():
    return str(uuid.uuid4())


def ms(stamp):
    """ISO-8601 timestamp or epoch milliseconds to epoch milliseconds."""
    if stamp is None:
        return None
    if isinstance(stamp, (int, float)):
        return int(stamp)
    text = str(stamp).replace("Z", "+00:00")
    try:
        return int(datetime.datetime.fromisoformat(text).timestamp() * 1000)
    except ValueError:
        # Python 3.9 rejects fractional seconds that are not 3 or 6 digits.
        match = re.match(r"(.*\.\d{1,6})\d*(.*)", text)
        if match:
            return int(datetime.datetime.fromisoformat(match.group(1) + match.group(2)).timestamp() * 1000)
        return None


def bound(text):
    """Keep the tail of long output, as MonoCode does for live tool calls."""
    data = text.encode("utf-8")
    if len(data) <= OUTPUT_LIMIT:
        return text
    tail = data[-OUTPUT_LIMIT:].decode("utf-8", "ignore")
    return "… earlier output omitted …\n" + tail


def read_jsonl(path):
    with open(path, encoding="utf-8") as handle:
        for line in handle:
            line = line.strip()
            if line:
                try:
                    yield json.loads(line)
                except json.JSONDecodeError:
                    continue


# ---------------------------------------------------------------- blocks


def tool_kind(name):
    # monocode-core store/shared/blocks.rs `update`
    return {
        "Bash": "execute",
        "commandExecution": "execute",
        "Read": "read",
        "Write": "write",
        "Edit": "edit",
        "fileChange": "edit",
        "Grep": "search",
        "Glob": "search",
        "webSearch": "search",
        "Agent": "agent",
        "Task": "agent",
        "collabAgentToolCall": "agent",
        "subAgentActivity": "agent",
    }.get(name, "other")


def tool_block(call_id, name, raw_input, output="", status="completed", at=None):
    """A tool block shaped like the ones MonoCode writes for native tool calls."""
    native_input = raw_input if isinstance(raw_input, str) else json.dumps(raw_input, ensure_ascii=False)
    try:
        parsed = json.loads(native_input)
    except ValueError:
        parsed = None
    if not isinstance(parsed, dict):
        parsed = {"command": native_input}
    title = None
    for key in ("command", "path", "file_path"):
        value = parsed.get(key)
        if isinstance(value, str) and value:
            title = value
            break
    title = title or name
    kind = tool_kind(name)
    preview = {field: parsed[field] for field in ("path", "query", "title") if parsed.get(field) is not None}
    if kind == "execute":
        preview["title"] = title
    block = {
        "id": new_id(),
        "role": "tool",
        "text": title,
        "tool": {
            "callId": call_id or new_id(),
            "nativeName": name,
            "nativeInput": native_input,
            "kind": kind,
            "title": title,
            "preview": preview,
            "detail": bound(output or ""),
            "status": status,
        },
    }
    if at:
        block["sentAt"] = at
    return block


def text_block(role, text, at=None):
    block = {"id": new_id(), "role": role, "text": text}
    if at:
        block["sentAt"] = at
    return block


def notice(text, at=None):
    return text_block("system", text, at)


class Attachments:
    """Writes inline images next to the database so user blocks can attach them."""

    def __init__(self, directory):
        self.directory = directory
        self.files = []

    def image(self, data, mime):
        extension = {"image/png": ".png", "image/jpeg": ".jpg", "image/gif": ".gif", "image/webp": ".webp"}.get(mime, ".bin")
        raw = base64.b64decode(data)
        name = "image-" + hashlib.sha256(raw).hexdigest()[:16] + extension
        path = os.path.join(self.directory, name)
        self.files.append((path, raw))
        return self.entry(path, mime, len(raw))

    @staticmethod
    def entry(path, mime, size=None):
        item = {"id": path, "path": path, "name": os.path.basename(path), "mimeType": mime, "kind": "image" if mime.startswith("image/") else "file"}
        if size is not None:
            item["size"] = size
        return item

    def write(self):
        if not self.files:
            return
        os.makedirs(self.directory, exist_ok=True)
        for path, raw in self.files:
            with open(path, "wb") as handle:
                handle.write(raw)


def user_block(text, at, attachments=None):
    block = {"id": new_id(), "role": "user", "text": VISUALS.sub("", text)}
    if at:
        block["startedAt"] = at
    if attachments:
        block["attachments"] = attachments
    return block


class Session:
    def __init__(self, harness, provider_id, cwd, source):
        self.harness = harness
        self.provider_id = provider_id
        self.cwd = cwd
        self.source = source
        self.blocks = []
        self.title = None
        self.model = harness + ":default"
        self.created = None
        self.updated = None
        self.attachments = None
        self._turn = None

    def start_turn(self, block):
        self.blocks.append(block)
        self._turn = block

    def finish_turn(self, end):
        turn = self._turn
        if turn is not None and end and turn.get("startedAt") and "durationMs" not in turn:
            turn["durationMs"] = max(0, end - turn["startedAt"])

    def touch(self, at):
        if at:
            self.created = at if self.created is None else min(self.created, at)
            self.updated = at if self.updated is None else max(self.updated, at)

    def first_prompt(self):
        for block in self.blocks:
            if block["role"] == "user" and block["text"].strip():
                return block["text"].strip()
        return ""


def clean_title(text, limit=60):
    text = " ".join(VISUALS.sub("", text).split())
    return text if len(text) <= limit else text[: limit - 1] + "…"


# ---------------------------------------------------------------- Claude Code


def claude_files():
    return glob.glob(os.path.join(CLAUDE_ROOT, "*", "*.jsonl"))


def claude_meta(path):
    cwd, title, first = None, None, None
    for record in read_jsonl(path):
        cwd = cwd or record.get("cwd")
        if record.get("type") == "ai-title":
            title = record.get("aiTitle") or title
        if first is None and record.get("type") == "user" and not record.get("isMeta"):
            content = record.get("message", {}).get("content")
            if isinstance(content, str) and not content.startswith("<"):
                first = content
    return cwd, title or clean_title(first or "")


def claude_result_text(content):
    if isinstance(content, str):
        return content
    parts = []
    for item in content or []:
        if item.get("type") == "text":
            parts.append(item.get("text", ""))
        elif item.get("type") == "image":
            parts.append("[image]")
    return "\n".join(parts)


def claude_command(text):
    """`<command-name>/x</command-name><command-args>y</command-args>` to `/x y`."""
    name = re.search(r"<command-name>(.*?)</command-name>", text, re.S)
    args = re.search(r"<command-args>(.*?)</command-args>", text, re.S)
    if not name:
        return None
    command = name.group(1).strip()
    if args and args.group(1).strip():
        command += " " + args.group(1).strip()
    return command


def convert_claude(path, attachments):
    records = list(read_jsonl(path))
    provider_id = os.path.splitext(os.path.basename(path))[0]
    cwd = next((r["cwd"] for r in records if r.get("cwd")), None)
    session = Session("claude", provider_id, cwd, path)
    session.attachments = attachments
    tools = {}
    last_assistant = None  # (message id, block) for merging streamed text
    for record in records:
        kind = record.get("type")
        if record.get("isSidechain"):
            continue
        at = ms(record.get("timestamp"))
        if kind == "ai-title" and record.get("aiTitle"):
            session.title = record["aiTitle"]
            continue
        if kind == "system":
            if record.get("subtype") == "turn_duration" and session._turn is not None:
                session._turn.setdefault("durationMs", record.get("durationMs"))
            elif record.get("subtype") == "compact_boundary":
                session.blocks.append(notice("Context compacted", at))
            continue
        if kind not in ("user", "assistant") or record.get("isMeta"):
            continue
        session.touch(at)
        message = record.get("message") or {}
        content = message.get("content")
        if kind == "user":
            if record.get("isCompactSummary"):
                continue  # The compact_boundary notice marks it.
            if isinstance(content, str):
                content = [{"type": "text", "text": content}]
            texts, files = [], []
            for item in content or []:
                if item.get("type") == "tool_result":
                    block = tools.get(item.get("tool_use_id"))
                    if block is not None:
                        block["tool"]["detail"] = bound(claude_result_text(item.get("content")))
                        block["tool"]["status"] = "error" if item.get("is_error") else "completed"
                elif item.get("type") == "text":
                    texts.append(item.get("text", ""))
                elif item.get("type") == "image" and item.get("source", {}).get("type") == "base64":
                    source = item["source"]
                    files.append(attachments.image(source["data"], source.get("media_type", "image/png")))
            text = REMINDER.sub("", "\n".join(texts)).strip()
            if not text and not files:
                continue
            if text.startswith("<task-notification>") or text.startswith("<local-command-"):
                continue
            if text.startswith("<command-"):
                text = claude_command(text) or text
            if text.startswith("[Request interrupted by user"):
                session.blocks.append(notice("Interrupted", at))
                continue
            session.start_turn(user_block(text, at, files))
            last_assistant = None
            continue
        for item in content or []:
            item_type = item.get("type")
            if item_type == "text" and item.get("text", "").strip():
                if last_assistant and last_assistant[0] == message.get("id"):
                    last_assistant[1]["text"] += item["text"]
                else:
                    block = text_block("assistant", item["text"], at)
                    session.blocks.append(block)
                    last_assistant = (message.get("id"), block)
            elif item_type == "thinking" and item.get("thinking", "").strip():
                session.blocks.append(text_block("reasoning", item["thinking"], at))
                last_assistant = None
            elif item_type == "tool_use":
                block = tool_block(item.get("id"), item.get("name", "Tool"), item.get("input") or {}, "", "cancelled", at)
                tools[item.get("id")] = block
                session.blocks.append(block)
                last_assistant = None
    return session


# ---------------------------------------------------------------- Codex


def codex_files():
    return glob.glob(os.path.join(CODEX_ROOT, "**", "rollout-*.jsonl"), recursive=True)


def codex_meta(path):
    cwd, first = None, None
    for record in read_jsonl(path):
        payload = record.get("payload") or {}
        if record.get("type") == "session_meta":
            cwd = payload.get("cwd")
        elif payload.get("type") == "item_completed" and payload.get("item", {}).get("type") == "UserMessage":
            first = codex_text(payload["item"].get("content"))
            break
    return cwd, clean_title(first or "")


def codex_text(content):
    return "\n".join(part.get("text", "") for part in content or [] if part.get("type") in ("text", "Text")).strip()


def codex_path(value):
    return value[len("file://"):] if isinstance(value, str) and value.startswith("file://") else value


def codex_status(item):
    status = item.get("status") or "completed"
    if status == "completed" and item.get("exit_code") not in (None, 0):
        return "error"
    return {"completed": "completed", "failed": "error", "declined": "error"}.get(status, "cancelled" if status in ("cancelled", "canceled", "interrupted", "aborted") else "completed")


def convert_codex(path, attachments):
    session = Session("codex", None, None, path)
    session.attachments = attachments
    saw_items = False
    for record in read_jsonl(path):
        payload = record.get("payload") or {}
        at = ms(record.get("timestamp"))
        if record.get("type") == "session_meta":
            session.provider_id = session.provider_id or payload.get("id")
            session.cwd = session.cwd or payload.get("cwd")
            continue
        if record.get("type") != "event_msg":
            continue
        event = payload.get("type")
        if event == "task_complete" or event == "turn_aborted":
            session.finish_turn(at)
            if event == "turn_aborted":
                session.blocks.append(notice("Interrupted", at))
            continue
        if event != "item_completed":
            continue
        saw_items = True
        item = payload.get("item") or {}
        kind = item.get("type")
        at = payload.get("started_at_ms") or at
        session.touch(payload.get("completed_at_ms") or at)
        if kind == "UserMessage":
            files = []
            for part in item.get("content") or []:
                local = codex_path(part.get("path"))
                if part.get("type") in ("localImage", "LocalImage") and local:
                    mime = "image/png" if local.lower().endswith(".png") else "image/jpeg"
                    files.append(Attachments.entry(local, mime))
            text = codex_text(item.get("content"))
            if text or files:
                session.start_turn(user_block(text, at, files))
        elif kind == "AgentMessage":
            text = codex_text(item.get("content"))
            if text:
                session.blocks.append(text_block("assistant", text, at))
        elif kind == "Reasoning":
            text = "\n\n".join(item.get("summary_text") or []) or "\n\n".join(
                part.get("text", "") for part in item.get("raw_content") or [] if isinstance(part, dict)
            )
            if text.strip():
                session.blocks.append(text_block("reasoning", text, at))
        elif kind == "CommandExecution":
            command = item.get("command")
            command = shlex.join(command) if isinstance(command, list) else (command or "")
            raw = {"command": command, "cwd": codex_path(item.get("cwd"))}
            for field in ("parsed_cmd", "exit_code", "duration_ms"):
                if item.get(field) is not None:
                    raw[field] = item[field]
            output = item.get("aggregated_output") or item.get("stdout") or ""
            session.blocks.append(tool_block(item.get("id"), "commandExecution", raw, output, codex_status(item), at))
        elif kind == "FileChange":
            changes = item.get("changes") or {}
            paths = list(changes)
            raw = {"path": paths[0] if paths else "Files", "paths": paths}
            output = "\n".join("{}\n{}".format(p, (c or {}).get("unified_diff", "")) for p, c in changes.items())
            session.blocks.append(tool_block(item.get("id"), "fileChange", raw, output, codex_status(item), at))
        elif kind == "McpToolCall":
            result = item.get("result") or {}
            output = claude_result_text(result.get("content")) if isinstance(result, dict) and "content" in result else json.dumps(result, ensure_ascii=False, indent=2)
            name = "{}:{}".format(item.get("server") or "mcp", item.get("tool") or "tool")
            raw = json.dumps(item.get("arguments"), ensure_ascii=False, indent=2)
            session.blocks.append(tool_block(item.get("id"), name, raw, output, codex_status(item), at))
        elif kind == "WebSearch":
            session.blocks.append(tool_block(item.get("id"), "webSearch", {"query": item.get("query", "")}, "", "completed", at))
        elif kind == "ImageView":
            session.blocks.append(tool_block(item.get("id"), "viewImage", {"path": codex_path(item.get("path"))}, "", "completed", at))
        elif kind == "Extension":
            raw = {"title": item.get("kind") or "extension", "prompt": item.get("revisedPrompt")}
            session.blocks.append(tool_block(item.get("id"), item.get("kind") or "extension", raw, "", codex_status(item), at))
    if not saw_items:
        fail("{} has no item_completed records; this Codex rollout format is not supported".format(path))
    return session


# ---------------------------------------------------------------- Pi


def pi_files():
    return glob.glob(os.path.join(PI_ROOT, "*", "*.jsonl"))


def pi_branch(records):
    """Pi sessions are trees; follow the newest entry back to the root."""
    entries = [r for r in records if r.get("id") and r.get("type") != "session"]
    if not entries:
        return []
    by_id = {r["id"]: r for r in entries}
    path, node, seen = [], entries[-1], set()
    while node is not None and node["id"] not in seen:
        seen.add(node["id"])
        path.append(node)
        node = by_id.get(node.get("parentId"))
    return list(reversed(path))


def pi_meta(path):
    records = list(read_jsonl(path))
    header = next((r for r in records if r.get("type") == "session"), {})
    first = ""
    for record in pi_branch(records):
        message = record.get("message") or {}
        if record.get("type") == "message" and message.get("role") == "user":
            first = VISUALS.sub("", "\n".join(p.get("text", "") for p in message.get("content") or [] if p.get("type") == "text"))
            break
    return header.get("cwd"), clean_title(first)


def convert_pi(path, attachments):
    records = list(read_jsonl(path))
    header = next((r for r in records if r.get("type") == "session"), {})
    session = Session("pi", header.get("id"), header.get("cwd"), path)
    session.attachments = attachments
    tools = {}
    for record in pi_branch(records):
        at = ms(record.get("timestamp"))
        kind = record.get("type")
        if kind == "model_change" and record.get("provider") and record.get("modelId"):
            session.model = "pi:{}/{}".format(record["provider"], record["modelId"])
            continue
        if kind == "compaction":
            session.blocks.append(notice("Context compacted", at))
            continue
        if kind != "message":
            continue
        session.touch(at)
        message = record.get("message") or {}
        role = message.get("role")
        content = message.get("content")
        if isinstance(content, str):
            content = [{"type": "text", "text": content}]
        if role == "user":
            texts, files = [], []
            for part in content or []:
                if part.get("type") == "text":
                    texts.append(part.get("text", ""))
                elif part.get("type") == "image" and part.get("data"):
                    files.append(attachments.image(part["data"], part.get("mimeType", "image/png")))
            text = "\n".join(texts).strip()
            if text or files:
                session.start_turn(user_block(text, ms(message.get("timestamp")) or at, files))
        elif role == "assistant":
            for part in content or []:
                if part.get("type") == "text" and part.get("text", "").strip():
                    session.blocks.append(text_block("assistant", part["text"], at))
                elif part.get("type") == "thinking" and part.get("thinking", "").strip():
                    session.blocks.append(text_block("reasoning", part["thinking"], at))
                elif part.get("type") == "toolCall":
                    raw = json.dumps(part.get("arguments") or {}, ensure_ascii=False, indent=2)
                    block = tool_block(part.get("id"), part.get("name", "Tool"), raw, "", "cancelled", at)
                    tools[part.get("id")] = block
                    session.blocks.append(block)
            if session._turn is not None and message.get("timestamp") and message.get("durationMs"):
                session._turn["durationMs"] = max(0, message["timestamp"] + message["durationMs"] - session._turn.get("startedAt", message["timestamp"]))
        elif role == "toolResult":
            block = tools.get(message.get("toolCallId"))
            if block is not None:
                text = "\n".join(p.get("text", "") for p in content or [] if p.get("type") == "text")
                block["tool"]["detail"] = bound(text.rstrip("\n"))
                block["tool"]["status"] = "error" if message.get("isError") else "completed"
    return session


# ---------------------------------------------------------------- sources

CONVERTERS = {"claude": convert_claude, "codex": convert_codex, "pi": convert_pi}
FILES = {"claude": claude_files, "codex": codex_files, "pi": pi_files}
META = {"claude": claude_meta, "codex": codex_meta, "pi": pi_meta}


def find_source(harness, session):
    if os.path.isfile(session):
        return os.path.abspath(session)
    matches = [p for p in FILES[harness]() if session in os.path.basename(p)]
    if not matches:
        fail("no {} session file matches {!r}".format(harness, session))
    if len(matches) > 1:
        fail("{!r} matches several {} sessions:\n  {}".format(session, harness, "\n  ".join(sorted(matches))))
    return matches[0]


def default_db():
    return RELEASE_DB


def sidecar(db):
    return db + ".gpui.json"


# ---------------------------------------------------------------- database


def db_in_use(db):
    try:
        out = subprocess.run(["lsof", "-t", "--", db], capture_output=True, text=True).stdout.strip()
    except FileNotFoundError:
        return []
    return [pid for pid in out.split() if pid]


def backup(path, stamp):
    if os.path.exists(path):
        target = "{}.bak-{}".format(path, stamp)
        shutil.copy2(path, target)
        return target
    return None


def backup_db(db, stamp):
    target = "{}.bak-{}".format(db, stamp)
    source = sqlite3.connect(db)
    dest = sqlite3.connect(target)
    with dest:
        source.backup(dest)
    source.close()
    dest.close()
    return target


def existing(conn, harness, provider_id):
    row = conn.execute("SELECT id, title FROM sessions WHERE harness=? AND provider_session_id=?", (harness, provider_id)).fetchone()
    return row


def add_project(path, cwd, select):
    """Add `cwd` to the native workspace's project list in the .gpui.json sidecar."""
    if not os.path.exists(path):
        return "no sidecar; open the project in MonoCode once"
    with open(path, encoding="utf-8") as handle:
        extras = json.load(handle)
    raw = extras.get("workspace")
    if not raw:
        return "sidecar has no workspace yet; open the project in MonoCode once"
    workspace = json.loads(raw)
    projects = workspace.setdefault("projects", [])
    note = "already listed"
    if cwd not in projects:
        projects.append(cwd)
        note = "added"
    if select:
        workspace["project"] = cwd
        workspace["active"] = None
        note += ", selected"
    extras["workspace"] = json.dumps(workspace, ensure_ascii=False, separators=(",", ":"))
    temp = path + ".tmp-import"
    with open(temp, "w", encoding="utf-8") as handle:
        json.dump(extras, handle, ensure_ascii=False, separators=(",", ":"))
    os.replace(temp, path)
    return note


def summarize(session):
    counts = {}
    for block in session.blocks:
        counts[block["role"]] = counts.get(block["role"], 0) + 1
    images = sum(len(b.get("attachments") or []) for b in session.blocks)
    return ", ".join("{} {}".format(v, k) for k, v in sorted(counts.items())) + ", {} attachments".format(images)


def cmd_list(args):
    rows = []
    for path in FILES[args.harness]():
        try:
            cwd, title = META[args.harness](path)
        except Exception as error:  # noqa: BLE001 - listing must not stop on one bad file
            cwd, title = None, "<unreadable: {}>".format(error)
        if args.cwd and os.path.realpath(cwd or "") != os.path.realpath(args.cwd):
            continue
        rows.append((os.path.getmtime(path), path, cwd, title))
    for mtime, path, cwd, title in sorted(rows, reverse=True)[: args.limit]:
        stamp = datetime.datetime.fromtimestamp(mtime).strftime("%Y-%m-%d %H:%M")
        print("{}  {}\n    cwd: {}\n    title: {}".format(stamp, path, cwd, title))


def cmd_import(args):
    source = find_source(args.harness, args.session)
    db = os.path.abspath(args.db or default_db())
    if not os.path.exists(db):
        fail("database not found: " + db)
    session_id = new_id()
    attachments = Attachments(os.path.join(os.path.dirname(db), "imported", session_id))
    session = CONVERTERS[args.harness](source, attachments)
    if not session.provider_id:
        fail("could not read the provider session id from " + source)
    cwd = os.path.abspath(args.cwd or session.cwd or "")
    if not session.cwd and not args.cwd:
        fail("the session does not record its directory; pass --cwd")
    title = args.title or session.title or clean_title(session.first_prompt()) or "Imported {} session".format(args.harness)
    if args.title is None and session.title and re.fullmatch(r"[a-z0-9]+(-[a-z0-9]+)+", session.title):
        title = session.title.replace("-", " ").capitalize()
    now = int(datetime.datetime.now().timestamp() * 1000)
    created = session.created or now
    updated = session.updated or created
    print("source:   " + source)
    print("harness:  {}  provider session: {}".format(args.harness, session.provider_id))
    print("project:  " + cwd)
    print("title:    " + title)
    print("history:  " + summarize(session))
    print("database: " + db)
    conn = sqlite3.connect(db)
    columns = {row[1] for row in conn.execute("PRAGMA table_info(sessions)")}
    for required in ("id", "cwd", "harness", "model", "runtime_mode", "title", "provider_session_id", "blocks_json", "created_at", "updated_at"):
        if required not in columns:
            fail("unsupported MonoCode database: sessions.{} is missing".format(required))
    found = existing(conn, args.harness, session.provider_id)
    conn.close()
    if found and not args.replace:
        fail("already imported as session {} ({!r}); pass --replace to overwrite it".format(found[0], found[1]))
    if args.dry_run:
        if args.dump:
            with open(args.dump, "w", encoding="utf-8") as handle:
                json.dump(session.blocks, handle, ensure_ascii=False, indent=1)
            print("blocks written to " + args.dump)
        print("dry run: nothing written")
        return
    users = db_in_use(db)
    if users:
        fail("the database is open in process {}; quit MonoCode (and the Tauri app) first".format(", ".join(users)))
    stamp = datetime.datetime.now().strftime("%Y%m%d-%H%M%S")
    print("backup:   " + backup_db(db, stamp))
    side = backup(sidecar(db), stamp)
    if side:
        print("backup:   " + side)
    attachments.write()
    blocks = json.dumps(session.blocks, ensure_ascii=False, separators=(",", ":"))
    has_user = any(b["role"] == "user" for b in session.blocks)
    conn = sqlite3.connect(db)
    with conn:
        if found:
            conn.execute("DELETE FROM sessions WHERE id=?", (found[0],))
        fields = {
            "id": session_id,
            "cwd": cwd,
            "harness": args.harness,
            "model": args.model or session.model,
            "model_settings": "{}",
            "runtime_mode": args.mode,
            "title": title,
            "provider_session_id": session.provider_id,
            "blocks_json": blocks,
            "created_at": created,
            "updated_at": updated,
        }
        if "has_user_message" in columns:
            fields["has_user_message"] = int(has_user)
        names = ",".join(fields)
        marks = ",".join("?" for _ in fields)
        conn.execute("INSERT INTO sessions({}) VALUES({})".format(names, marks), list(fields.values()))
    stored = conn.execute("SELECT length(blocks_json) FROM sessions WHERE id=?", (session_id,)).fetchone()
    conn.close()
    print("session:  {} ({} bytes of history)".format(session_id, stored[0]))
    print("project list: " + add_project(sidecar(db), cwd, args.select_project))
    print("done. Open MonoCode, pick the project and the session; the next message resumes it.")


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    sub = parser.add_subparsers(dest="command", required=True)
    listing = sub.add_parser("list", help="list sessions a harness has saved")
    listing.add_argument("--harness", choices=HARNESSES, required=True)
    listing.add_argument("--cwd", help="only sessions recorded in this directory")
    listing.add_argument("--limit", type=int, default=15)
    listing.set_defaults(run=cmd_list)
    imp = sub.add_parser("import", help="import one session into a MonoCode database")
    imp.add_argument("--harness", choices=HARNESSES, required=True)
    imp.add_argument("--session", required=True, help="session id, part of a file name, or a path")
    imp.add_argument("--db", help="MonoCode database (default: the installed app's monocode.db)")
    imp.add_argument("--dev", action="store_true", help="use the development database")
    imp.add_argument("--cwd", help="project directory (default: the session's own)")
    imp.add_argument("--title", help="session title")
    imp.add_argument("--model", help="model id, for example claude:default")
    imp.add_argument("--mode", default="supervised", choices=["read-only", "supervised", "auto-accept-edits", "auto", "full-access"], help="runtime mode")
    imp.add_argument("--select-project", action="store_true", help="open MonoCode on this project")
    imp.add_argument("--replace", action="store_true", help="replace an earlier import of this session")
    imp.add_argument("--dry-run", action="store_true", help="convert and report without writing")
    imp.add_argument("--dump", help="with --dry-run, write the converted blocks to this JSON file")
    imp.set_defaults(run=cmd_import)
    args = parser.parse_args(argv)
    if getattr(args, "dev", False):
        args.db = args.db or DEV_DB
    args.run(args)


if __name__ == "__main__":
    main()
