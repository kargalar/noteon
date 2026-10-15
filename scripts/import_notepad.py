"""Merge the active Notepad workspace into Noteon without modifying the source.

The source workspace database is snapshotted with SQLite's backup API. Attachments
are hard-linked because both applications live on the same NTFS volume; this keeps
the full library available in Noteon's import folder without duplicating 45+ GB.
"""

from __future__ import annotations

import argparse
import json
import os
import shutil
import sqlite3
import sys
from collections import defaultdict
from datetime import datetime, timezone
from pathlib import Path
from typing import Any


PROPERTY_KINDS = {"text", "number", "date", "status", "select", "multi-select", "checkbox"}
NODE_KINDS = {
    "paragraph": "text",
    "heading": "h",
    "bulletList": "bullet",
    "orderedList": "numbered",
    "taskList": "todo",
    "codeBlock": "code",
    "blockquote": "quote",
    "horizontalRule": "divider",
    "attachmentImage": "text",
    "databaseTable": "database",
    "pageLink": "text",
    "smartLink": "text",
    "callout": "callout",
}


def parse_args() -> argparse.Namespace:
    appdata = Path(os.environ["APPDATA"])
    local_appdata = Path(os.environ["LOCALAPPDATA"])
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--source",
        type=Path,
        default=appdata / "com.kargalar.notepad/workspaces/import-3dccca531225ffc610bf",
    )
    parser.add_argument(
        "--target-db",
        type=Path,
        default=local_appdata / "Kargalar/Noteon/data/noteon.sqlite",
    )
    parser.add_argument("--dry-run", action="store_true")
    return parser.parse_args()


def utc_stamp() -> str:
    return datetime.now(timezone.utc).strftime("%Y%m%dT%H%M%SZ")


def file_digest(path: Path) -> str:
    import hashlib

    digest = hashlib.sha256()
    with path.open("rb") as source:
        for chunk in iter(lambda: source.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def filetime_to_unix_seconds(value: int | None) -> int:
    if value is None:
        return 0
    return max(0, int(value) // 1000)


def json_value(value: str | None, default: Any) -> Any:
    if not value:
        return default
    try:
        return json.loads(value)
    except (TypeError, json.JSONDecodeError):
        return default


def text_from_node(node: Any, media_paths: set[str]) -> str:
    if not isinstance(node, dict):
        return ""
    node_type = node.get("type", "")
    attrs = node.get("attrs") or {}
    if node_type == "attachmentImage":
        path = attrs.get("relativePath") or attrs.get("src") or ""
        name = attrs.get("name") or Path(path).name or "attachment"
        if path:
            media_paths.add(path.replace("\\", "/"))
        return f"[Ek: {name} — {path}]" if path else f"[Ek: {name}]"
    if node_type == "databaseTable":
        return "[Database]"
    if node_type == "pageLink":
        return str(attrs.get("title") or attrs.get("pageId") or "[Page link]")
    if node_type == "smartLink":
        return str(attrs.get("label") or attrs.get("title") or attrs.get("href") or "[Link]")
    if node_type == "text":
        return str(node.get("text", ""))
    return "".join(text_from_node(child, media_paths) for child in node.get("content", []))


def blocks_from_json(content: str, known_databases: dict[str, str]) -> list[tuple[str, str, int]]:
    root = json_value(content, {})
    if not isinstance(root, dict):
        return [("text", str(content), 0)]
    nodes = root.get("content", [root])
    output: list[tuple[str, str, int]] = []

    def flatten(node: Any, forced_kind: str | None = None, checked: int = 0) -> None:
        if not isinstance(node, dict):
            return
        kind_name = node.get("type", "paragraph")
        attrs = node.get("attrs") or {}
        current_kind = forced_kind or NODE_KINDS.get(kind_name, "text")
        current_checked = checked
        if kind_name == "heading":
            level = max(1, min(3, int(attrs.get("level", 1) or 1)))
            current_kind = f"h{level}"
        if kind_name == "taskItem":
            current_kind = "todo"
            current_checked = int(bool(attrs.get("checked", False)))

        if kind_name in {"bulletList", "orderedList", "taskList", "blockColumns", "blockColumn", "doc"}:
            child_kind = {"bulletList": "bullet", "orderedList": "numbered", "taskList": "todo"}.get(kind_name)
            for child in node.get("content", []):
                flatten(child, child_kind or forced_kind, current_checked)
            return
        if kind_name in {"listItem", "taskItem"}:
            for child in node.get("content", []):
                flatten(child, "todo" if kind_name == "taskItem" else forced_kind, current_checked)
            return

        media_paths: set[str] = set()
        text = text_from_node(node, media_paths).strip()
        if kind_name == "databaseTable":
            database_id = attrs.get("databaseId") or attrs.get("database_id")
            text = known_databases.get(str(database_id), "") if database_id else ""
            if not text:
                text = "Database (details are stored in imports/notepad*/workspace.sqlite)"
        if kind_name == "horizontalRule":
            text = ""
        output.append((current_kind, text, current_checked))

    for node in nodes:
        flatten(node)
    return output or [("text", "", 0)]


def media_paths_from_json(content: str) -> set[str]:
    root = json_value(content, {})
    found: set[str] = set()

    def visit(value: Any) -> None:
        if isinstance(value, dict):
            if value.get("type") == "attachmentImage":
                attrs = value.get("attrs") or {}
                path = attrs.get("relativePath") or attrs.get("src")
                if path:
                    found.add(str(path).replace("\\", "/"))
            for child in value.values():
                visit(child)
        elif isinstance(value, list):
            for child in value:
                visit(child)

    visit(root)
    return found


def normalize_property_value(kind: str, raw: str) -> str:
    value = json_value(raw, raw)
    if kind == "multi-select" and isinstance(value, list):
        return " | ".join(str(item) for item in value)
    if kind == "date" and isinstance(value, dict):
        start = value.get("start", "")
        end = value.get("end", "")
        return f"{start} – {end}" if end else str(start)
    if isinstance(value, (dict, list)):
        return json.dumps(value, ensure_ascii=False)
    if isinstance(value, bool):
        return "true" if value else "false"
    return str(value)


def options_text(options: Any) -> str:
    if isinstance(options, list):
        return "\n".join(str(item.get("name", item)) if isinstance(item, dict) else str(item) for item in options)
    return ""


def hardlink_tree(source: Path, target: Path) -> tuple[int, int, int]:
    target.mkdir(parents=True, exist_ok=True)
    linked = missing = bytes_linked = 0
    for path in source.rglob("*"):
        if not path.is_file():
            continue
        relative = path.relative_to(source)
        destination = target / relative
        destination.parent.mkdir(parents=True, exist_ok=True)
        try:
            os.link(path, destination)
            linked += 1
            bytes_linked += path.stat().st_size
        except FileNotFoundError:
            missing += 1
        if linked and linked % 500 == 0:
            print(f"Attachments: created {linked:,} hard links…", flush=True)
    return linked, missing, bytes_linked


def make_backup(connection: sqlite3.Connection, backup_path: Path) -> None:
    backup_path.parent.mkdir(parents=True, exist_ok=True)
    destination = sqlite3.connect(backup_path)
    try:
        connection.backup(destination)
    finally:
        destination.close()


def ensure_target_schema(connection: sqlite3.Connection) -> None:
    version = int(connection.execute("PRAGMA user_version").fetchone()[0])
    if version > 6:
        raise RuntimeError(f"The Noteon database uses unsupported schema version {version}.")
    if version == 5:
        connection.execute(
            "ALTER TABLE databases ADD COLUMN gallery_columns INTEGER NOT NULL DEFAULT 3 CHECK(gallery_columns BETWEEN 1 AND 6)"
        )
        connection.execute("UPDATE databases SET view_mode='table' WHERE view_mode='blog'")
        connection.execute("PRAGMA user_version=6")
        version = 6
    if version != 6:
        raise RuntimeError(
            f"The Noteon database uses schema version {version}; open and close Noteon once before importing, or update the schema."
        )


def migration_counts(source: sqlite3.Connection) -> dict[str, int]:
    names = ("pages", "databases", "records", "blocks", "columns", "cells", "media", "media_refs")
    return {name: int(source.execute(f"SELECT COUNT(*) FROM {name}").fetchone()[0]) for name in names}


def ensure_source(source_dir: Path) -> tuple[Path, Path]:
    database = source_dir / "workspace.sqlite"
    attachments = source_dir / "attachments"
    if not database.is_file():
        raise FileNotFoundError(f"Source workspace database not found: {database}")
    if not attachments.is_dir():
        raise FileNotFoundError(f"Source attachments directory not found: {attachments}")
    return database, attachments


def validate_schema(connection: sqlite3.Connection) -> None:
    required = {
        "pages", "databases", "records", "blocks", "columns", "cells", "views", "media", "media_refs"
    }
    present = {row[0] for row in connection.execute("SELECT name FROM sqlite_master WHERE type='table'")}
    missing = required - present
    if missing:
        raise RuntimeError(f"The source database is missing expected schema items: {', '.join(sorted(missing))}")


def run_import(source_dir: Path, target_db: Path, dry_run: bool) -> None:
    source_db, source_attachments = ensure_source(source_dir)
    source_connection = sqlite3.connect(f"file:{source_db.as_posix()}?mode=ro", uri=True)
    source_connection.row_factory = sqlite3.Row
    validate_schema(source_connection)
    counts = migration_counts(source_connection)
    attachment_files = [path for path in source_attachments.rglob("*") if path.is_file()]
    attachment_bytes = sum(path.stat().st_size for path in attachment_files)
    missing_files = [path for path in attachment_files if not path.exists()]
    import hashlib

    fingerprint = hashlib.sha256()
    for candidate in (source_db, Path(f"{source_db}-wal")):
        if candidate.is_file():
            fingerprint.update(candidate.name.encode("utf-8"))
            fingerprint.update(bytes.fromhex(file_digest(candidate)))
    source_fingerprint = fingerprint.hexdigest()
    marker = f"notepad-import:{source_fingerprint}"
    print(json.dumps({
        "source": str(source_dir),
        "source_fingerprint": source_fingerprint,
        "counts": counts,
        "attachment_files": len(attachment_files),
        "attachment_bytes": attachment_bytes,
        "attachment_missing_on_disk": len(missing_files),
        "dry_run": dry_run,
    }, ensure_ascii=False, indent=2))
    if dry_run:
        source_connection.close()
        return

    target_db.parent.mkdir(parents=True, exist_ok=True)
    target_connection = sqlite3.connect(target_db)
    target_connection.execute("PRAGMA foreign_keys=ON")
    existing = target_connection.execute("SELECT value FROM settings WHERE key=?", (marker,)).fetchone()
    if existing:
        source_connection.close()
        target_connection.close()
        raise RuntimeError("This Notepad workspace has already been imported into this Noteon database.")

    timestamp = utc_stamp()
    data_dir = target_db.parent
    archive_dir = data_dir / "imports" / f"notepad-{timestamp}"
    backup_path = data_dir / "backups" / f"pre-notepad-import-{timestamp}.sqlite"
    archive_dir.mkdir(parents=True, exist_ok=False)
    try:
        make_backup(target_connection, backup_path)
        print(f"Created Noteon backup: {backup_path}", flush=True)
        archive_db = sqlite3.connect(archive_dir / "workspace.sqlite")
        try:
            source_connection.backup(archive_db)
        finally:
            archive_db.close()
        for filename in ("legacy-notes.json", "import-report.json"):
            source_file = source_dir / filename
            if source_file.is_file():
                shutil.copy2(source_file, archive_dir / filename)
        linked, missing, linked_bytes = hardlink_tree(source_attachments, archive_dir / "attachments")
    print(f"Attachments ready: {linked:,} files, {linked_bytes:,} bytes; {missing:,} files missing.", flush=True)
        ensure_target_schema(target_connection)
        target_connection.commit()

        page_rows = source_connection.execute(
            "SELECT id,parent_id,title,icon,created_at,updated_at,deleted_at,database_id,properties,cover FROM pages ORDER BY created_at,id"
        ).fetchall()
        db_rows = source_connection.execute("SELECT id,page_id,title FROM databases").fetchall()
    print(f"Mapping pages and databases: {len(page_rows):,} pages, {len(db_rows):,} databases…", flush=True)
        records = source_connection.execute("SELECT page_id,database_id,position FROM records ORDER BY database_id,position").fetchall()
        record_database = {row["page_id"]: row["database_id"] for row in records}
        source_db_by_id = {row["id"]: row["page_id"] for row in db_rows}
        source_db_title = {row["id"]: row["title"] for row in db_rows}

        database_to_source_page = {row["id"]: row["page_id"] for row in db_rows}
        page_to_source_database = {page_id: db_id for db_id, page_id in database_to_source_page.items()}
        target_connection.execute("BEGIN IMMEDIATE")
        page_map: dict[str, int] = {}
        for row in page_rows:
            cursor = target_connection.execute(
                "INSERT INTO pages(parent_id,title,icon,favorite,cover,created_at,updated_at) VALUES(NULL,?,?,0,0,?,?)",
                (row["title"] or "", row["icon"] or "📄", filetime_to_unix_seconds(row["created_at"]), filetime_to_unix_seconds(row["updated_at"])),
            )
            page_map[row["id"]] = int(cursor.lastrowid)
        for row in page_rows:
            source_id = row["id"]
            page_id = page_map[source_id]
            source_parent = record_database.get(source_id)
            if source_parent:
                parent_source_page = database_to_source_page.get(source_parent)
            else:
                parent_source_page = row["parent_id"]
            parent_target = page_map.get(parent_source_page) if parent_source_page else None
            target_connection.execute("UPDATE pages SET parent_id=? WHERE id=?", (parent_target, page_id))

        # Trash each deleted subtree under its highest deleted ancestor.
        page_by_source = {row["id"]: row for row in page_rows}
        for row in page_rows:
            if row["deleted_at"] is None:
                continue
            root = row
            while root["parent_id"] in page_by_source and page_by_source[root["parent_id"]]["deleted_at"] is not None:
                root = page_by_source[root["parent_id"]]
            target_connection.execute("UPDATE pages SET trashed_by=? WHERE id=?", (page_map[root["id"]], page_map[row["id"]]))

        for source_db_id, source_page in database_to_source_page.items():
            target_page = page_map[source_page]
            target_connection.execute("INSERT INTO databases(page_id,view_mode,sort_key,sort_descending,gallery_columns) VALUES(?,'table','created_at',0,3)", (target_page,))

        property_map: dict[str, int] = {}
        col_rows = source_connection.execute("SELECT id,database_id,position,definition FROM columns ORDER BY database_id,position").fetchall()
        column_kinds: dict[str, str] = {}
        used_names: dict[str, set[str]] = defaultdict(set)
        for col in col_rows:
            definition = json_value(col["definition"], {})
            if not isinstance(definition, dict):
                definition = {}
            kind = definition.get("type", "text")
            kind = kind if kind in PROPERTY_KINDS else "text"
            column_kinds[col["id"]] = kind
            name = str(definition.get("name") or "Property").strip() or "Property"
            db_page = page_map[database_to_source_page[col["database_id"]]]
            unique_name = name
            suffix = 2
            while unique_name in used_names[db_page]:
                unique_name = f"{name} ({suffix})"
                suffix += 1
            used_names[db_page].add(unique_name)
            cursor = target_connection.execute(
                "INSERT INTO database_properties(database_page_id,name,kind,options,position) VALUES(?,?,?,?,?)",
                (db_page, unique_name, kind, options_text(definition.get("options")), int(col["position"])),
            )
            property_map[col["id"]] = int(cursor.lastrowid)

        cell_rows = source_connection.execute("SELECT page_id,column_id,value FROM cells").fetchall()
        for cell in cell_rows:
            if cell["page_id"] not in page_map or cell["column_id"] not in property_map:
                continue
            kind = column_kinds.get(cell["column_id"], "text")
            target_connection.execute(
                "INSERT OR REPLACE INTO page_property_values(page_id,property_id,value) VALUES(?,?,?)",
                (page_map[cell["page_id"]], property_map[cell["column_id"]], normalize_property_value(kind, cell["value"])),
            )

        for row in page_rows:
            props = json_value(row["properties"], [])
            if not isinstance(props, list):
                props = []
            page_id = page_map[row["id"]]
            names: set[str] = set()
            for position, prop in enumerate(props):
                if not isinstance(prop, dict):
                    continue
                name = str(prop.get("name") or "Property").strip() or "Property"
                unique_name = name
                suffix = 2
                while unique_name in names:
                    unique_name = f"{name} ({suffix})"
                    suffix += 1
                names.add(unique_name)
                raw_value = prop.get("value", "")
                kind = prop.get("type", "text")
                kind = kind if kind in PROPERTY_KINDS else "text"
                cursor = target_connection.execute(
                    "INSERT INTO custom_page_properties(page_id,name,kind,options,position) VALUES(?,?,?,?,?)",
                    (page_id, unique_name, kind, "", position),
                )
                prop_id = int(cursor.lastrowid)
                target_connection.execute(
                    "INSERT INTO custom_page_property_values(page_id,property_id,value) VALUES(?,?,?)",
                    (page_id, prop_id, normalize_property_value(kind, json.dumps(raw_value, ensure_ascii=False))),
                )
            cover = json_value(row["cover"], None)
            if isinstance(cover, dict):
                relative_path = str(cover.get("relativePath", "")).replace("\\", "/")
                if relative_path and "Notepad kapak" not in names:
                    cover_name = str(cover.get("name") or Path(relative_path).name)
                    cover_value = f"{cover_name} — {archive_dir / relative_path}"
                    cursor = target_connection.execute(
                        "INSERT INTO custom_page_properties(page_id,name,kind,options,position) VALUES(?,'Notepad kapak','text','',?)",
                        (page_id, len(names)),
                    )
                    target_connection.execute(
                        "INSERT INTO custom_page_property_values(page_id,property_id,value) VALUES(?,?,?)",
                        (page_id, int(cursor.lastrowid), cover_value),
                    )

        blocks_by_page: dict[str, list[sqlite3.Row]] = defaultdict(list)
        for row in source_connection.execute("SELECT page_id,position,content FROM blocks ORDER BY page_id,position"):
            blocks_by_page[row["page_id"]].append(row)
        media_refs_by_page: dict[str, set[str]] = defaultdict(set)
        for row in source_connection.execute("SELECT page_id,relative_path FROM media_refs"):
            media_refs_by_page[row["page_id"]].add(row["relative_path"].replace("\\", "/"))
        media_names = {
            row["relative_path"].replace("\\", "/"): row["name"]
            for row in source_connection.execute("SELECT relative_path,name FROM media")
        }
        missing_media = {
            row["relative_path"].replace("\\", "/")
            for row in source_connection.execute("SELECT relative_path FROM media WHERE missing=1")
        }
        for source_page_id, target_page_id in page_map.items():
            source_database_id = page_to_source_database.get(source_page_id)
            db_target_id = page_map[source_page_id] if source_database_id else None
            inserted = 0
            visible_media: set[str] = set()
            for source_block in blocks_by_page.get(source_page_id, []):
                visible_media.update(media_paths_from_json(source_block["content"]))
                mapped_blocks = blocks_from_json(source_block["content"], source_db_by_id)
                for kind, content, checked in mapped_blocks:
                    content = content.replace(
                        "attachments/",
                        f"{archive_dir / 'attachments'}{os.sep}",
                    )
                    if kind == "database" and db_target_id is not None:
                        content = str(db_target_id)
                    target_connection.execute(
                        "INSERT INTO blocks(page_id,position,kind,content,checked) VALUES(?,?,?,?,?)",
                        (target_page_id, inserted, kind, content, checked),
                    )
                    inserted += 1
            for relative_path in sorted(media_refs_by_page.get(source_page_id, set()) - visible_media):
                name = media_names.get(relative_path, Path(relative_path).name)
                missing_label = "Eksik ek" if relative_path in missing_media else "Ek"
                content = f"[{missing_label}: {name} — {archive_dir / relative_path}]"
                target_connection.execute(
                    "INSERT INTO blocks(page_id,position,kind,content) VALUES(?,?,'text',?)",
                    (target_page_id, inserted, content),
                )
                inserted += 1
            if inserted == 0:
                target_connection.execute("INSERT INTO blocks(page_id,position,kind,content) VALUES(?,0,'text','')", (target_page_id,))
    print(f"Processed blocks and properties: {counts['blocks']:,} source blocks…", flush=True)

        target_connection.execute("INSERT INTO settings(key,value) VALUES(?,?)", (marker, timestamp))
        target_connection.execute("COMMIT")

        manifest = {
            "source_workspace": str(source_dir),
            "source_database_sha256": source_fingerprint,
            "imported_at_utc": timestamp,
            "source_counts": counts,
            "linked_attachment_count": linked,
            "linked_attachment_bytes": linked_bytes,
            "missing_attachment_count": missing,
            "missing_media_rows": int(source_connection.execute("SELECT COUNT(*) FROM media WHERE missing=1").fetchone()[0]),
            "source_media_rows": [dict(row) for row in source_connection.execute("SELECT * FROM media")],
            "source_media_references": [dict(row) for row in source_connection.execute("SELECT * FROM media_refs")],
            "database_views": [dict(row) for row in source_connection.execute("SELECT * FROM views")],
        }
        (archive_dir / "migration-manifest.json").write_text(json.dumps(manifest, ensure_ascii=False, indent=2), encoding="utf-8")
        integrity = target_connection.execute("PRAGMA foreign_key_check").fetchall()
        if integrity:
        raise RuntimeError(f"Post-import foreign key validation failed: {integrity[:3]}")
        print(json.dumps({
            "result": "success",
            "archive": str(archive_dir),
            "backup": str(backup_path),
            "imported_pages": len(page_map),
            "imported_databases": len(db_rows),
            "imported_records": len(records),
            "imported_blocks": sum(len(value) for value in blocks_by_page.values()),
            "linked_attachments": linked,
            "missing_attachments": missing,
            "source_counts": counts,
        }, ensure_ascii=False, indent=2), flush=True)
    except Exception:
        if target_connection.in_transaction:
            target_connection.execute("ROLLBACK")
        raise
    finally:
        source_connection.close()
        target_connection.close()


def main() -> int:
    if hasattr(sys.stdout, "reconfigure"):
        sys.stdout.reconfigure(encoding="utf-8")
    if hasattr(sys.stderr, "reconfigure"):
        sys.stderr.reconfigure(encoding="utf-8")
    args = parse_args()
    try:
        run_import(args.source, args.target_db, args.dry_run)
    except Exception as error:  # provide a concise command-line failure
        print(f"Import failed: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
