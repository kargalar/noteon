"""Rebuild Noteon's page data from a Notion Markdown/CSV export.

All pages, blocks, databases and properties are removed (settings are kept) and the
Notion export is imported with its full hierarchy. Attachments from the previous
Notepad import are re-linked to the new pages through the archived workspace's
``notionPageId`` metadata and moved into ``attachments/notion/<page>/<original name>``.
Attachments that cannot be linked are moved to ``attachments/unmatched`` and listed on
an "Eşleşmeyen Ekler" page. A Markdown/JSON report is written after the import.
"""

from __future__ import annotations

import argparse
import csv
import html
import json
import os
import re
import sqlite3
import sys
import time
from collections import Counter, defaultdict
from dataclasses import dataclass, field
from datetime import datetime, timezone
from pathlib import Path
from urllib.parse import unquote

sys.path.insert(0, str(Path(__file__).resolve().parent))
from migrate_properties import classify_property, is_iso_date  # noqa: E402

csv.field_size_limit(1 << 30)

ID_RE = re.compile(r"^(?P<title>.*?)\s*(?P<id>[0-9a-f]{32})(?P<all>_all)?$")
LINK_RE = re.compile(r"(!?)\[([^\]]*)\]\(((?:[^()\s]|\([^()]*\))+)\)")
MONTHS = {m: i for i, m in enumerate(
    ["January", "February", "March", "April", "May", "June", "July", "August",
     "September", "October", "November", "December"], 1)}
NOTION_DATE_RE = re.compile(r"^(?P<month>[A-Z][a-z]+) (?P<day>\d{1,2}), (?P<year>\d{4})")
IMAGE_EXT = {"png", "jpg", "jpeg", "webp", "gif", "bmp", "tif", "tiff"}
VIDEO_EXT = {"mp4", "mov", "m4v", "webm", "mkv", "avi", "wmv"}
UNMATCHED_PAGE_TITLE = "Eşleşmeyen Ekler"
CREATED_COLUMNS = {"created", "created time", "created at", "oluşturulma"}


# --------------------------------------------------------------------------- utils

def utc_stamp() -> str:
    return datetime.now(timezone.utc).strftime("%Y%m%dT%H%M%SZ")


def norm_name(value: str) -> str:
    value = unquote(value).replace("\\", "/").rsplit("/", 1)[-1].lower()
    return re.sub(r"[^0-9a-z.\u00c0-\u024f]", "", value)


def strip_counter(value: str) -> str:
    stem, dot, ext = norm_name(value).rpartition(".")
    if not dot:
        stem, ext = ext, ""
    stem = re.sub(r"(\d+|\(\d+\))$", "", stem) if re.search(r"[a-z]", stem) else stem
    return f"{stem}.{ext}"


def extension(value: str) -> str:
    name = unquote(value).rsplit("/", 1)[-1]
    return name.rpartition(".")[2].lower() if "." in name else ""


def safe_component(value: str, limit: int = 60) -> str:
    cleaned = re.sub(r'[<>:"/\\|?*\x00-\x1f]', "_", value).strip().rstrip(".")
    return (cleaned[:limit].rstrip() or "untitled").rstrip(".")


def is_url(value: str) -> bool:
    if len(value) >= 3 and value[1] == ":" and value[2] in ("\\", "/"):
        return False
    return value.startswith(("http://", "https://", "mailto:", "tel:", "#")) or bool(re.match(r"^[a-zA-Z][a-zA-Z0-9+.-]+://", value))


def parse_notion_date(value: str) -> str | None:
    value = value.strip()
    if is_iso_date(value[:10]) and (len(value) == 10 or not value[10:11].isdigit()):
        return value[:10]
    match = NOTION_DATE_RE.match(value)
    if not match or match["month"] not in MONTHS:
        return None
    candidate = f"{match['year']}-{MONTHS[match['month']]:02d}-{int(match['day']):02d}"
    return candidate if is_iso_date(candidate) else None


def notion_date_timestamp(value: str) -> int | None:
    iso = parse_notion_date(value)
    if not iso:
        return None
    time_match = re.search(r"(\d{1,2}):(\d{2}) ?(AM|PM)", value)
    hour = minute = 0
    if time_match:
        hour = int(time_match[1]) % 12 + (12 if time_match[3] == "PM" else 0)
        minute = int(time_match[2])
    return int(datetime(int(iso[:4]), int(iso[5:7]), int(iso[8:10]), hour, minute).timestamp())


def relation_titles(value: str) -> str:
    return re.sub(r" \((?:[^()]|\([^()]*\))*?\.md\)", "", value)


# ------------------------------------------------------------------ inline styles

INLINE_RE = re.compile(
    r"(?P<bi>\*\*\*(?=\S)(?P<bi_t>.+?)(?<=\S)\*\*\*)"
    r"|(?P<b>\*\*(?=\S)(?P<b_t>.+?)(?<=\S)\*\*)"
    r"|(?P<s>~~(?=\S)(?P<s_t>.+?)(?<=\S)~~)"
    r"|(?P<c>`(?P<c_t>[^`]+)`)"
    r"|(?P<i>(?<![\w*])\*(?=[^\s*])(?P<i_t>.+?)(?<=[^\s*])\*(?![\w*]))"
    r"|(?P<l>\[(?P<l_t>[^\]]*)\]\((?P<l_u>(?:[^()\s]|\([^()]*\))+)\))"
    r"|(?P<e>\\(?P<e_t>[\\`*_{}\[\]()#+\-.!~|>]))"
)


def parse_inline(text: str, link_text) -> tuple[str, list[dict]]:
    chars: list[tuple[str, frozenset]] = []

    def walk(segment: str, styles: frozenset) -> None:
        position = 0
        for match in INLINE_RE.finditer(segment):
            for ch in segment[position:match.start()]:
                chars.append((ch, styles))
            position = match.end()
            if match["bi"]:
                walk(match["bi_t"], styles | {"bold", "italic"})
            elif match["b"]:
                walk(match["b_t"], styles | {"bold"})
            elif match["s"]:
                walk(match["s_t"], styles | {"strikethrough"})
            elif match["c"]:
                for ch in match["c_t"]:
                    chars.append((ch, styles | {"mono"}))
            elif match["i"]:
                walk(match["i_t"], styles | {"italic"})
            elif match["l"]:
                for ch in link_text(match["l_t"], match["l_u"]):
                    chars.append((ch, styles | {"underline"}))
            elif match["e"]:
                chars.append((match["e_t"], styles))
        for ch in segment[position:]:
            chars.append((ch, styles))

    walk(text, frozenset())
    plain = "".join(ch for ch, _ in chars)
    ranges: list[dict] = []
    index = 0
    while index < len(chars):
        styles = chars[index][1]
        end = index
        while end < len(chars) and chars[end][1] == styles:
            end += 1
        if styles:
            style: dict = {name: True for name in styles if name != "mono"}
            if "mono" in styles:
                style["font_family"] = "Monospace"
            ranges.append({"start": index, "end": end, "style": style})
        index = end
    return plain, ranges


# ---------------------------------------------------------------------- model

@dataclass
class Node:
    notion_id: str
    path: Path
    kind: str  # "page" or "database"
    title: str
    parent: "Node | None" = None
    children: list["Node"] = field(default_factory=list)
    target_id: int = 0
    is_record: bool = False
    row_values: dict[str, str] = field(default_factory=dict)
    sort_order: int = 0


@dataclass
class Attachment:
    relative_path: str
    name: str
    source: Path
    destination: Path | None = None
    exists: bool = True
    used: bool = False


@dataclass
class Report:
    counts: Counter = field(default_factory=Counter)
    property_kinds: Counter = field(default_factory=Counter)
    downgraded_properties: list[str] = field(default_factory=list)
    hinted_properties: int = 0
    unmatched_links: list[str] = field(default_factory=list)
    weak_matches: list[str] = field(default_factory=list)
    appended_attachments: list[str] = field(default_factory=list)
    missing_files: list[str] = field(default_factory=list)
    unmatched_attachments: list[str] = field(default_factory=list)
    rows_without_page: list[str] = field(default_factory=list)
    records_without_row: list[str] = field(default_factory=list)
    orphan_files: list[str] = field(default_factory=list)
    unresolved_links: list[str] = field(default_factory=list)
    external_media: list[str] = field(default_factory=list)
    converted_tables: list[str] = field(default_factory=list)
    old_pages_missing: list[str] = field(default_factory=list)
    new_pages_without_old: int = 0
    covers: int = 0
    invalid_values: list[str] = field(default_factory=list)


# --------------------------------------------------------------- export scan

class Export:
    def __init__(self, root: Path, report: Report) -> None:
        self.root = root
        self.report = report
        self.nodes: dict[Path, Node] = {}
        self.by_id: dict[str, Node] = {}
        self.all_csv: dict[str, Path] = {}
        self.roots: list[Node] = []

    def scan(self) -> None:
        for path in sorted(self.root.rglob("*")):
            if not path.is_file() or path.suffix.lower() not in {".md", ".csv"}:
                continue
            match = ID_RE.match(path.stem)
            if not match:
                self.report.orphan_files.append(str(path.relative_to(self.root)))
                continue
            notion_id = match["id"]
            if match["all"]:
                self.all_csv[notion_id] = path
                continue
            kind = "database" if path.suffix.lower() == ".csv" else "page"
            title = match["title"]
            if kind == "page":
                first = path.open(encoding="utf-8", errors="ignore").readline().strip()
                if first.startswith("# "):
                    title = first[2:].strip()
            node = Node(notion_id, path, kind, title)
            self.nodes[path] = node
            self.by_id[notion_id] = node
        for notion_id, path in self.all_csv.items():
            if notion_id not in self.by_id:
                node = Node(notion_id, path, "database", ID_RE.match(path.stem)["title"])
                self.nodes[path] = node
                self.by_id[notion_id] = node
        self._link_hierarchy()

    def _owner_of(self, directory: Path) -> Node | None:
        siblings = [n for n in self.nodes.values() if n.path.parent == directory.parent]
        name = directory.name
        for node in siblings:
            short = f"{node.notion_id[:4]}-{node.notion_id[-4:]}"
            if name == f"{ID_RE.match(node.path.stem)['title']} {short}":
                return node
        candidates = [n for n in siblings if ID_RE.match(n.path.stem)["title"] == name]
        if len(candidates) == 1:
            return candidates[0]
        for node in candidates:
            short = f"{node.notion_id[:4]}-{node.notion_id[-4:]}"
            if not (directory.parent / f"{name} {short}").exists():
                linked = node.kind == "database" or self._links_into(node, directory)
                if linked:
                    return node
        return candidates[0] if candidates else None

    def _links_into(self, node: Node, directory: Path) -> bool:
        text = node.path.read_text(encoding="utf-8", errors="ignore")
        for match in LINK_RE.finditer(text):
            target = (node.path.parent / unquote(match[3])).resolve()
            if directory.resolve() in target.parents:
                return True
        return False

    def _link_hierarchy(self) -> None:
        owners: dict[Path, Node | None] = {}
        for path, node in self.nodes.items():
            directory = path.parent
            if directory == self.root:
                self.roots.append(node)
                continue
            if directory not in owners:
                owners[directory] = self._owner_of(directory)
            owner = owners[directory]
            if owner is None:
                self.report.orphan_files.append(str(path.relative_to(self.root)))
                self.roots.append(node)
                continue
            node.parent = owner
            owner.children.append(node)
            if owner.kind == "database":
                node.is_record = True

    def resolve_link(self, base: Path, target: str) -> Node | None:
        path = (base.parent / unquote(target)).resolve()
        node = self.nodes.get(path)
        if node:
            return node
        match = ID_RE.match(path.stem)
        return self.by_id.get(match["id"]) if match else None


# ------------------------------------------------------------- old metadata

class Legacy:
    """Attachment and schema knowledge from the previous Notepad/Noteon import."""

    def __init__(self, data_dir: Path, archive: Path, report: Report) -> None:
        self.report = report
        self.archive = archive
        self.attachment_root = archive / "attachments"
        self.workspace = sqlite3.connect(f"file:{(archive / 'workspace.sqlite').as_posix()}?mode=ro", uri=True)
        self.noteon = sqlite3.connect(f"file:{(data_dir / 'noteon.sqlite').as_posix()}?mode=ro", uri=True)
        self.attachments_dir = data_dir / "attachments"
        self.existing_on_disk: dict[str, list[Path]] = defaultdict(list)
        if self.attachments_dir.is_dir():
            for root, dirs, files in os.walk(self.attachments_dir):
                for f in files:
                    self.existing_on_disk[f].append(Path(root) / f)
        self.attachments: dict[str, Attachment] = {}
        self.page_attachments: dict[str, list[Attachment]] = defaultdict(list)
        self.covers: dict[str, Attachment] = {}
        self.times: dict[str, tuple[int, int]] = {}
        self.old_titles: dict[str, str] = {}
        self.page_by_workspace: dict[str, str] = {}
        self.property_hints: dict[str, dict[str, tuple[str, str]]] = {}
        self.workspace_column_kinds: dict[str, dict[str, str]] = {}
        self.view_modes: dict[str, tuple[str, int]] = {}
        self.favorites: set[str] = set()
        self._load()

    def attachment(self, relative_path: str, name: str | None, notion_id: str | None = None) -> Attachment:
        relative_path = relative_path.replace("\\", "/")
        existing = self.attachments.get(relative_path)
        if existing:
            if name and existing.name == Path(relative_path).name:
                existing.name = name
            return existing
        source = self.archive / relative_path
        exists = source.is_file()
        destination = None
        if not exists:
            fname = name or Path(relative_path).name
            candidates = self.existing_on_disk.get(fname, [])
            chosen = None
            if notion_id and candidates:
                prefix = notion_id[:8].lower()
                chosen = next((c for c in candidates if prefix in c.parent.name.lower()), None)
            if chosen is None and len(candidates) == 1:
                chosen = candidates[0]
            if chosen and chosen.is_file():
                source = chosen
                destination = chosen
                exists = True
        item = Attachment(relative_path, name or Path(relative_path).name, source, destination=destination, exists=exists)
        self.attachments[relative_path] = item
        return item

    def _visit_body(self, value, found: list[tuple[str, str]]) -> None:
        if isinstance(value, dict):
            if value.get("type") == "attachmentImage":
                attrs = value.get("attrs") or {}
                if attrs.get("relativePath"):
                    found.append((attrs["relativePath"], attrs.get("name") or ""))
            for child in value.values():
                self._visit_body(child, found)
        elif isinstance(value, list):
            for child in value:
                self._visit_body(child, found)

    def _load(self) -> None:
        names: dict[str, str] = {}
        for relative_path, name in self.workspace.execute("SELECT relative_path,name FROM media"):
            names[relative_path.replace("\\", "/")] = name
        refs: dict[str, list[str]] = defaultdict(list)
        for page_id, relative_path in self.workspace.execute("SELECT page_id,relative_path FROM media_refs"):
            refs[page_id].append(relative_path.replace("\\", "/"))

        for page_id, title, legacy, cover in self.workspace.execute("SELECT id,title,legacy,cover FROM pages"):
            data = json.loads(legacy or "{}")
            notion_id = (data.get("notionPageId") or "").replace("-", "").lower()
            ordered: list[tuple[str, str]] = []
            try:
                self._visit_body(json.loads(data.get("body") or "{}"), ordered)
            except json.JSONDecodeError:
                pass
            for item in data.get("attachments") or []:
                if item.get("relativePath"):
                    ordered.append((item["relativePath"], item.get("name") or ""))
            cover_data = json.loads(cover) if cover else None
            snapshot = (data.get("databaseItemSnapshot") or {}).get("coverImage")
            cover_data = cover_data or snapshot
            seen: set[str] = set()
            page_items: list[Attachment] = []
            for relative_path, name in ordered + [(p, "") for p in refs.get(page_id, [])]:
                key = relative_path.replace("\\", "/")
                if key in seen:
                    continue
                seen.add(key)
                page_items.append(self.attachment(key, name or names.get(key), notion_id))
            if isinstance(cover_data, dict) and cover_data.get("relativePath"):
                cover_item = self.attachment(cover_data["relativePath"], cover_data.get("name"), notion_id)
                if notion_id:
                    self.covers[notion_id] = cover_item
            if not notion_id:
                if page_items:
                    for item in page_items:
                        self.page_attachments[f"workspace:{page_id}:{title}"].append(item)
                continue
            self.page_by_workspace[page_id] = notion_id
            self.page_attachments[notion_id].extend(page_items)
            self.old_titles[notion_id] = title or ""
            created = int(data.get("createdAt") or 0) // 1000
            updated = int(data.get("updatedAt") or 0) // 1000
            self.times[notion_id] = (created, updated or created)

        for relative_path, name in names.items():
            self.attachment(relative_path, None)
        for path in self.attachment_root.rglob("*"):
            if path.is_file():
                self.attachment(path.relative_to(self.archive).as_posix(), None)

        for _db_id, page_id, definition in self.workspace.execute(
            "SELECT c.database_id,d.page_id,c.definition FROM columns c JOIN databases d ON d.id=c.database_id"
        ):
            notion_id = self.page_by_workspace.get(page_id)
            if not notion_id:
                continue
            spec = json.loads(definition or "{}")
            self.workspace_column_kinds.setdefault(notion_id, {})[str(spec.get("name", "")).strip()] = spec.get("type", "text")
        self._load_noteon_hints()

    def _paths(self, rows: list[tuple], parent_of) -> dict[object, tuple[str, ...]]:
        titles = {row[0]: row[1] or "" for row in rows}
        cache: dict[object, tuple[str, ...]] = {}

        def path_of(page_id, depth: int = 0) -> tuple[str, ...]:
            if page_id in cache:
                return cache[page_id]
            parent = parent_of.get(page_id)
            prefix = path_of(parent, depth + 1) if parent in titles and depth < 64 else ()
            cache[page_id] = prefix + (titles[page_id],)
            return cache[page_id]

        return {page_id: path_of(page_id) for page_id in titles}

    def _load_noteon_hints(self) -> None:
        workspace_rows = self.workspace.execute("SELECT id,title,parent_id FROM pages").fetchall()
        record_db = dict(self.workspace.execute(
            "SELECT r.page_id,d.page_id FROM records r JOIN databases d ON d.id=r.database_id"))
        workspace_parent = {row[0]: record_db.get(row[0], row[2]) for row in workspace_rows}
        workspace_paths = self._paths(workspace_rows, workspace_parent)
        path_to_notion: dict[tuple[str, ...], str | None] = {}
        for page_id, path in workspace_paths.items():
            notion_id = self.page_by_workspace.get(page_id)
            path_to_notion[path] = None if path in path_to_notion else notion_id

        noteon_rows = self.noteon.execute("SELECT id,title,parent_id,favorite FROM pages WHERE trashed_by IS NULL").fetchall()
        noteon_paths = self._paths(noteon_rows, {row[0]: row[2] for row in noteon_rows})
        path_count = Counter(noteon_paths.values())
        noteon_to_notion = {
            page_id: path_to_notion.get(path)
            for page_id, path in noteon_paths.items()
            if path_count[path] == 1 and path_to_notion.get(path)
        }
        for page_id, _title, _parent, favorite in noteon_rows:
            if favorite and page_id in noteon_to_notion:
                self.favorites.add(noteon_to_notion[page_id])
        for page_id, view_mode, columns in self.noteon.execute("SELECT page_id,view_mode,gallery_columns FROM databases"):
            if page_id in noteon_to_notion:
                self.view_modes[noteon_to_notion[page_id]] = (view_mode, int(columns))
        for db_page, name, kind, options in self.noteon.execute(
            "SELECT database_page_id,name,kind,options FROM database_properties"
        ):
            notion_id = noteon_to_notion.get(db_page)
            if notion_id:
                self.property_hints.setdefault(notion_id, {})[name] = (kind, options)


# ------------------------------------------------------------ media matching

class MediaMatcher:
    def __init__(self, legacy: Legacy, attachments_dir: Path, report: Report) -> None:
        self.legacy = legacy
        self.report = report
        self.attachments_dir = attachments_dir
        self.global_by_name: dict[str, list[Attachment]] = defaultdict(list)
        for item in legacy.attachments.values():
            self.global_by_name[norm_name(item.name)].append(item)
        self.used_destinations: set[str] = set()

    def destination_for(self, item: Attachment, folder: str) -> Path:
        if item.destination is None:
            base = self.attachments_dir / folder
            name = safe_component(item.name, 120)
            stem, dot, ext = name.rpartition(".")
            if not dot:
                stem, ext = name, ""
            candidate = base / name
            counter = 2
            while str(candidate).lower() in self.used_destinations:
                candidate = base / (f"{stem} ({counter}).{ext}" if ext else f"{stem} ({counter})")
                counter += 1
            self.used_destinations.add(str(candidate).lower())
            item.destination = candidate
        return item.destination

    def page_pool(self, notion_id: str) -> list[Attachment]:
        return self.legacy.page_attachments.get(notion_id, [])

    def match(self, pool: list[Attachment], consumed: set[str], target: str, page_label: str) -> Attachment | None:
        key = norm_name(target)
        loose = strip_counter(target)
        ext = extension(target)
        free = [item for item in pool if item.relative_path not in consumed]
        for item in free:
            if norm_name(item.name) == key:
                return item
        for item in free:
            if extension(item.name) == ext and strip_counter(item.name) == loose:
                return item
        for item in pool:
            if norm_name(item.name) == key:
                return item
        global_hits = self.global_by_name.get(key, [])
        if len(global_hits) == 1:
            self.report.weak_matches.append(f"{page_label}: `{unquote(target)}` → başka sayfadaki tek eşleşen dosya `{global_hits[0].name}`")
            return global_hits[0]
        same_ext = [item for item in free if extension(item.name) == ext]
        if len(same_ext) == 1:
            self.report.weak_matches.append(f"{page_label}: `{unquote(target)}` → aynı uzantılı kalan tek ek `{same_ext[0].name}`")
            return same_ext[0]
        return None


# --------------------------------------------------------------- md → blocks

@dataclass
class BlockRow:
    kind: str
    content: str
    checked: int = 0
    styles: list[dict] = field(default_factory=list)


class MarkdownConverter:
    def __init__(self, export: Export, matcher: MediaMatcher, report: Report) -> None:
        self.export = export
        self.matcher = matcher
        self.report = report

    def convert(self, node: Node, property_names: set[str]) -> list[BlockRow]:
        lines = node.path.read_text(encoding="utf-8", errors="ignore").splitlines()
        index = 0
        if lines and lines[0].startswith("# "):
            index = 1
        while index < len(lines) and not lines[index].strip():
            index += 1
        if node.is_record:
            probe = index
            while probe < len(lines) and lines[probe].strip():
                key = lines[probe].split(":", 1)[0].strip()
                if ":" not in lines[probe] or key not in property_names:
                    break
                probe += 1
            if probe > index and (probe == len(lines) or not lines[probe].strip()):
                index = probe
        self.node = node
        self.label = self._label(node)
        self.pool = self.matcher.page_pool(node.notion_id)
        self.consumed: set[str] = set()
        blocks = self._blocks(lines[index:])
        for item in self.pool:
            if item.relative_path in self.consumed or self._is_cover(item):
                continue
            self.consumed.add(item.relative_path)
            self.report.appended_attachments.append(f"{self.label}: `{item.name}`")
            blocks.extend(self._media_blocks(item, item.name))
        return blocks

    def _is_cover(self, item: Attachment) -> bool:
        cover = self.matcher.legacy.covers.get(self.node.notion_id)
        return cover is not None and cover.relative_path == item.relative_path

    @staticmethod
    def _label(node: Node) -> str:
        parts: list[str] = []
        current: Node | None = node
        while current is not None and len(parts) < 4:
            parts.append(current.title or "Untitled")
            current = current.parent
        return " / ".join(reversed(parts))

    # -- inline helpers
    def _link_text(self, text: str, url: str) -> str:
        if url.startswith(("http://", "https://", "mailto:")):
            clean = text.strip()
            return clean if clean == url or not clean else f"{clean} ({url})"
        target = self.export.resolve_link(self.node.path, url)
        if target:
            return text or target.title
        return text or unquote(url).rsplit("/", 1)[-1]

    def _text_block(self, kind: str, raw: str, checked: int = 0) -> BlockRow:
        content, styles = parse_inline(html.unescape(raw), self._link_text)
        return BlockRow(kind, content, checked, styles)

    def _media_blocks(self, item: Attachment, label: str) -> list[BlockRow]:
        folder = safe_component(self.node.title or "Untitled") + f" {self.node.notion_id[:8]}"
        destination = self.matcher.destination_for(item, f"notion/{folder}")
        item.used = True
        if not item.exists:
            self.report.missing_files.append(f"{self.label}: `{item.name}` (eski kayıtta da diskte yoktu)")
            return [BlockRow("text", f"📎 {item.name} — eksik dosya")]
        ext = extension(item.name) or extension(item.relative_path)
        if ext in IMAGE_EXT:
            return [BlockRow("image", str(destination))]
        if ext in VIDEO_EXT:
            return [BlockRow("video", str(destination))]
        return [BlockRow("text", f"📎 {label} — {destination}")]

    def _local_media(self, label: str, target: str) -> list[BlockRow]:
        item = self.matcher.match(self.pool, self.consumed, target, self.label)
        if item is None:
            self.report.unmatched_links.append(f"{self.label}: `{unquote(target)}`")
            return [BlockRow("text", f"📎 {label or unquote(target)} — eşleşmeyen ek")]
        self.consumed.add(item.relative_path)
        return self._media_blocks(item, label or item.name)

    def _link_line(self, line: str) -> list[BlockRow] | None:
        stripped = line.strip()
        match = LINK_RE.fullmatch(stripped)
        if not match:
            return None
        is_image, label, url = match.groups()
        if is_url(url):
            if is_image and url.startswith(("http://", "https://")):
                self.report.external_media.append(f"{self.label}: {url[:140]}")
                return [BlockRow("text", f"🖼 {url}")]
            return None
        if url.lower().endswith((".md", ".csv")):
            target = self.export.resolve_link(self.node.path, url)
            if target is None:
                self.report.unresolved_links.append(f"{self.label}: `{unquote(url)}`")
                return [BlockRow("text", f"↗ {label}")]
            return [BlockRow("text", f"↗ {target.title or label}")]
        return self._local_media(label, url)

    # -- block parser
    def _blocks(self, lines: list[str]) -> list[BlockRow]:
        blocks: list[BlockRow] = []
        paragraph: list[str] = []
        index = 0

        def flush() -> None:
            if paragraph:
                blocks.extend(self._paragraph("\n".join(paragraph)))
                paragraph.clear()

        while index < len(lines):
            raw = lines[index]
            line = raw.strip()
            if not line:
                flush()
                index += 1
                continue
            if line.startswith("```"):
                flush()
                end = index + 1
                while end < len(lines) and not lines[end].strip().startswith("```"):
                    end += 1
                blocks.append(BlockRow("code", "\n".join(l.rstrip() for l in lines[index + 1:end])))
                index = end + 1
                continue
            if line.startswith("<aside>"):
                flush()
                end = index
                while end < len(lines) and "</aside>" not in lines[end]:
                    end += 1
                inner = [l.strip() for l in lines[index:end + 1]]
                inner = [re.sub(r"</?aside>", "", l).strip() for l in inner]
                inner = [l for l in inner if l and not l.startswith("<img ")]
                text_lines: list[str] = []
                for part in inner:
                    media = self._link_line(part)
                    if media is not None and media and media[0].kind in {"image", "video", "database"}:
                        blocks.extend(media)
                    else:
                        text_lines.append(part)
                blocks.append(self._text_block("callout", "\n".join(text_lines)))
                index = end + 1
                continue
            if line.startswith("|") and line.endswith("|"):
                flush()
                end = index
                rows: list[str] = []
                while end < len(lines) and lines[end].strip().startswith("|"):
                    cells = [c.strip() for c in lines[end].strip().strip("|").split("|")]
                    if not all(re.fullmatch(r":?-{3,}:?", c) for c in cells if c):
                        rows.append(" │ ".join(cells))
                    end += 1
                self.report.converted_tables.append(self.label)
                block = self._text_block("text", "\n".join(rows))
                block.styles.append({"start": 0, "end": len(block.content), "style": {"font_family": "Monospace"}})
                blocks.append(block)
                index = end
                continue
            if re.fullmatch(r"-{3,}|\*{3,}|_{3,}", line):
                flush()
                blocks.append(BlockRow("divider", ""))
                index += 1
                continue
            heading = re.match(r"^(#{1,6}) (.*)$", line)
            if heading:
                flush()
                level = min(3, len(heading[1]))
                blocks.append(self._text_block(f"h{level}", heading[2]))
                index += 1
                continue
            todo = re.match(r"^[-*+] \[([ xX])\]\s?(.*)$", line)
            if todo:
                flush()
                blocks.append(self._text_block("todo", todo[2].strip(), int(todo[1] in "xX")))
                index += 1
                continue
            bullet = re.match(r"^[-*+] (.*)$", line)
            if bullet:
                flush()
                blocks.extend(self._list_item("bullet", bullet[1]))
                index += 1
                continue
            numbered = re.match(r"^\d+[.)] (.*)$", line)
            if numbered:
                flush()
                blocks.extend(self._list_item("numbered", numbered[1]))
                index += 1
                continue
            if line.startswith(">"):
                flush()
                quote: list[str] = []
                while index < len(lines) and lines[index].strip().startswith(">"):
                    quote.append(re.sub(r"^>\s?", "", lines[index].strip()))
                    index += 1
                while quote and not quote[-1].strip():
                    quote.pop()
                blocks.append(self._text_block("quote", "\n".join(quote)))
                continue
            if line.startswith("<img ") or line in {"<details>", "</details>"}:
                index += 1
                continue
            paragraph.append(line)
            index += 1
        flush()
        return blocks

    def _list_item(self, kind: str, text: str) -> list[BlockRow]:
        media = self._link_line(text)
        if media is not None:
            return media
        return [self._text_block(kind, text)]

    def _paragraph(self, text: str) -> list[BlockRow]:
        if "\n" not in text:
            media = self._link_line(text)
            if media is not None:
                return media
        output: list[BlockRow] = []
        extra: list[BlockRow] = []
        kept_lines: list[str] = []
        for line in text.split("\n"):
            media = self._link_line(line)
            if media is not None:
                if kept_lines:
                    output.append(self._text_block("text", "\n".join(kept_lines)))
                    kept_lines = []
                output.extend(media)
                continue
            for match in LINK_RE.finditer(line):
                is_image, label, url = match.groups()
                if is_url(url):
                    if is_image and url.startswith(("http://", "https://")):
                        self.report.external_media.append(f"{self.label}: {url[:140]}")
                    continue
                if url.lower().endswith((".md", ".csv")):
                    continue
                if is_image or extension(url) or any(norm_name(item.name) == norm_name(url) for item in self.pool):
                    extra.extend(self._local_media(label, url))
            kept_lines.append(line)
        if kept_lines:
            output.append(self._text_block("text", "\n".join(kept_lines)))
        return output + extra


# ---------------------------------------------------------- property schema

def read_csv(path: Path) -> tuple[list[str], list[list[str]]]:
    with path.open(encoding="utf-8-sig", newline="") as handle:
        rows = list(csv.reader(handle))
    if not rows:
        return [], []
    return rows[0], rows[1:]


def convert_value(kind: str, raw: str) -> str | None:
    value = raw.strip()
    if not value:
        return "false" if kind == "checkbox" else ""
    if kind == "date":
        return parse_notion_date(value)
    if kind == "checkbox":
        lowered = value.lower()
        if lowered in {"yes", "true", "evet", "1"}:
            return "true"
        if lowered in {"no", "false", "hayır", "0"}:
            return "false"
        return None
    if kind == "number":
        cleaned = re.sub(r"[\s$€₺£%]", "", value)
        if re.fullmatch(r"-?\d{1,3}(,\d{3})+(\.\d+)?", cleaned):
            cleaned = cleaned.replace(",", "")
        cleaned = cleaned.replace(",", ".")
        if cleaned.lower() in {"notrated", "n/a", "none", "-", "yok"}:
            return ""
        try:
            number = float(cleaned)
        except ValueError:
            return None
        return cleaned if number == number and abs(number) != float("inf") else None
    if kind == "multi-select":
        return "|".join(part.strip() for part in re.split(r",(?=\s)|\|", value) if part.strip())
    if kind in {"select", "status"}:
        return value
    return relation_titles(value)


def build_schema(name: str, values: list[str], hint: tuple[str, str] | None, workspace_kind: str | None) -> tuple[str, str, bool]:
    cleaned = [relation_titles(v) for v in values]
    if hint:
        kind, options = hint
        hinted = True
    else:
        normalized = [parse_notion_date(v) or v for v in cleaned]
        kind, options, _ = classify_property(name, normalized, workspace_kind or "text", "")
        hinted = False
    return kind, options, hinted


# ------------------------------------------------------------------ importer

class Importer:
    def __init__(self, args: argparse.Namespace) -> None:
        self.args = args
        self.data_dir: Path = args.data_dir
        self.db_path = self.data_dir / "noteon.sqlite"
        self.archive = self.data_dir / "imports" / args.archive_name
        self.report = Report()
        self.stamp = utc_stamp()
        self.attachments_dir = self.data_dir / "attachments"
        self.export = Export(args.export, self.report)
        self.legacy = Legacy(self.data_dir, self.archive, self.report)
        self.matcher = MediaMatcher(self.legacy, self.attachments_dir, self.report)
        self.converter = MarkdownConverter(self.export, self.matcher, self.report)
        self.page_blocks: dict[str, list[BlockRow]] = {}
        self.schemas: dict[str, list[dict]] = {}
        self.unmatched_items: list[Attachment] = []

    # -- planning
    def plan(self) -> None:
        print("Scanning export…", flush=True)
        self.export.scan()
        print(f"  {len(self.export.nodes):,} pages/databases", flush=True)
        self._plan_databases()
        print("Converting Markdown…", flush=True)
        for count, node in enumerate(self._walk(), 1):
            if node.kind == "page":
                names = self._property_names(node)
                self.page_blocks[node.notion_id] = self.converter.convert(node, names)
            if count % 1000 == 0:
                print(f"  {count:,} converted", flush=True)
        self._plan_unmatched()
        self._calculate_counts()

    def _calculate_counts(self) -> None:
        self.report.counts["page"] = sum(1 for n in self.export.nodes.values() if n.kind == "page" and not n.is_record)
        self.report.counts["database"] = sum(1 for n in self.export.nodes.values() if n.kind == "database")
        self.report.counts["record"] = sum(1 for n in self.export.nodes.values() if n.is_record)
        if self.unmatched_items:
            self.report.counts["page"] += 1
        self.report.counts["block"] = sum(len(b) for b in self.page_blocks.values())
        for blocks in self.page_blocks.values():
            for b in blocks:
                self.report.counts[f"block:{b.kind}"] += 1
        self.report.covers = sum(
            1 for nid in self.export.by_id
            if nid in self.legacy.covers and self.legacy.covers[nid].exists
        )

    def _walk(self):
        stack = list(reversed(self.export.roots))
        while stack:
            node = stack.pop()
            yield node
            stack.extend(reversed(node.children))

    def _property_names(self, node: Node) -> set[str]:
        if node.is_record and node.parent:
            return {prop["name"] for prop in self.schemas.get(node.parent.notion_id, [])} | set(node.row_values)
        return set()

    def _plan_databases(self) -> None:
        for node in list(self.export.nodes.values()):
            if node.kind != "database":
                continue
            data_path = self.export.all_csv.get(node.notion_id, node.path)
            header, rows = read_csv(data_path)
            visible, _ = read_csv(node.path) if node.path != data_path else (header, rows)
            if not header:
                self.schemas[node.notion_id] = []
                continue
            title_column = header[0]
            order = [h for h in visible if h in header and h != title_column]
            order += [h for h in header if h not in order and h != title_column]
            columns = {h: [row[i] if i < len(row) else "" for row in rows] for i, h in enumerate(header)}
            hints = self.legacy.property_hints.get(node.notion_id, {})
            workspace_kinds = self.legacy.workspace_column_kinds.get(node.notion_id, {})
            schema: list[dict] = []
            for position, name in enumerate(order):
                values = columns[name]
                hint = hints.get(name)
                kind, options, hinted = build_schema(name, values, hint, workspace_kinds.get(name))
                self.report.hinted_properties += int(hinted)
                schema.append({"name": name, "kind": kind, "options": options, "position": position})
            self._fit_values(node, schema, columns)
            self.schemas[node.notion_id] = schema
            self._assign_rows(node, header, rows)

    def _fit_values(self, node: Node, schema: list[dict], columns: dict[str, list[str]]) -> None:
        for prop in schema:
            values = columns[prop["name"]]
            converted = [convert_value(prop["kind"], v) for v in values]
            if any(c is None for c, v in zip(converted, values)):
                bad = [v for c, v in zip(converted, values) if c is None][:3]
                self.report.downgraded_properties.append(
                    f"{node.title} → `{prop['name']}`: {prop['kind']} → text (uyumsuz değerler: {', '.join(map(repr, bad))})")
                prop["kind"], prop["options"] = "text", ""
            if prop["kind"] in {"select", "status", "multi-select"}:
                options = [line.strip() for line in prop["options"].splitlines() if line.strip()]
                known = {line.split("::")[0].strip() for line in options}
                for raw in values:
                    value = convert_value(prop["kind"], raw) or ""
                    parts = value.split("|") if prop["kind"] == "multi-select" else [value]
                    for part in parts:
                        if part and part not in known:
                            options.append(part)
                            known.add(part)
                if prop["kind"] == "status" and not options:
                    options = ["To do", "In progress", "Done"]
                prop["options"] = "\n".join(options)
            self.report.property_kinds[prop["kind"]] += 1

    def _assign_rows(self, node: Node, header: list[str], rows: list[list[str]]) -> None:
        records: dict[str, list[Node]] = defaultdict(list)
        for child in node.children:
            if child.kind == "page":
                records[child.title.strip()].append(child)
        title_index = 0
        ordered: list[Node] = []
        for position, row in enumerate(rows):
            title = (row[title_index] if row else "").strip()
            candidates = records.get(title) or records.get(re.sub(r"\s+", " ", title))
            values = {h: (row[i] if i < len(row) else "") for i, h in enumerate(header) if i != title_index}
            if candidates:
                record = candidates.pop(0)
            else:
                record = Node(f"row:{node.notion_id}:{position}", node.path, "page", title, parent=node, is_record=True)
                self.report.rows_without_page.append(f"{node.title} → `{title or 'Untitled'}`")
                self.page_blocks[record.notion_id] = []
            record.row_values = values
            record.sort_order = position
            ordered.append(record)
        leftovers = [child for child in node.children if child not in ordered]
        for offset, child in enumerate(leftovers, len(ordered)):
            if child.kind == "page":
                self.report.records_without_row.append(f"{node.title} → `{child.title}`")
                child.row_values = self._values_from_md(child, header)
            child.sort_order = offset
        node.children = ordered + leftovers

    @staticmethod
    def _values_from_md(node: Node, header: list[str]) -> dict[str, str]:
        values: dict[str, str] = {}
        names = set(header[1:])
        lines = node.path.read_text(encoding="utf-8", errors="ignore").splitlines()[1:]
        for line in lines:
            if not line.strip():
                if values:
                    break
                continue
            key, sep, value = line.partition(":")
            if not sep or key.strip() not in names:
                break
            values[key.strip()] = value.strip()
        return values

    def _plan_unmatched(self) -> None:
        present = set(self.export.by_id)
        for notion_id, items in self.legacy.page_attachments.items():
            if notion_id not in present and not notion_id.startswith("workspace:") and items:
                self.report.old_pages_missing.append(self.legacy.old_titles.get(notion_id, notion_id))
        for item in self.legacy.attachments.values():
            if item.used:
                continue
            cover_owner = next((nid for nid, c in self.legacy.covers.items() if c is item and nid in present), None)
            if cover_owner:
                folder = safe_component(self.export.by_id[cover_owner].title or "Untitled") + f" {cover_owner[:8]}"
                self.matcher.destination_for(item, f"notion/{folder}")
                item.used = True
                continue
            if not item.exists:
                continue
            self.matcher.destination_for(item, "unmatched")
            self.unmatched_items.append(item)
            self.report.unmatched_attachments.append(f"`{item.name}` ({item.relative_path})")
        self.report.new_pages_without_old = sum(
            1 for nid in present if nid not in self.legacy.times)

    # -- execution
    def move_files(self) -> list[tuple[str, str]]:
        moves: list[tuple[str, str]] = []
        for item in self.legacy.attachments.values():
            if item.destination is None or not item.exists:
                continue
            if item.source == item.destination:
                continue
            if not item.source.is_file():
                if item.destination.is_file():
                    continue
                self.report.missing_files.append(f"`{item.name}` taşıma sırasında bulunamadı")
                continue
            item.destination.parent.mkdir(parents=True, exist_ok=True)
            os.replace(item.source, item.destination)
            moves.append((str(item.source), str(item.destination)))
        return moves

    def write_database(self) -> None:
        connection = sqlite3.connect(self.db_path)
        connection.execute("PRAGMA foreign_keys=ON")
        try:
            connection.execute("BEGIN IMMEDIATE")
            self._reset(connection)
            for root_order, node in enumerate(self.export.roots):
                node.sort_order = root_order
            for node in self._walk():
                self._insert_node(connection, node)
            self._insert_unmatched_page(connection)
            self._insert_blocks(connection)
            if self.export.roots:
                primary = next((r for r in self.export.roots if "notebook" in r.title.lower()), max(self.export.roots, key=lambda r: len(r.children)))
                connection.execute("INSERT OR REPLACE INTO settings(key,value) VALUES('last_page',?)", (str(primary.target_id),))
            connection.execute("INSERT INTO page_search(page_search) VALUES('rebuild')")
            connection.execute("INSERT INTO block_search(block_search) VALUES('rebuild')")
            problems = connection.execute("PRAGMA foreign_key_check").fetchall()
            if problems:
                raise RuntimeError(f"Foreign key check failed: {problems[:3]}")
            connection.execute("COMMIT")
        except Exception:
            if connection.in_transaction:
                connection.execute("ROLLBACK")
            raise
        finally:
            connection.close()

    def _reset(self, connection: sqlite3.Connection) -> None:
        cols = [r[1] for r in connection.execute("PRAGMA table_info(pages)").fetchall()]
        if "full_width" not in cols:
            connection.execute("ALTER TABLE pages ADD COLUMN full_width INTEGER NOT NULL DEFAULT 0 CHECK(full_width IN (0,1))")
            connection.execute("PRAGMA user_version = 12")
        for table in ("custom_page_property_values", "custom_page_properties", "page_property_values",
                      "database_properties", "databases", "blocks", "pages"):
            connection.execute(f"DELETE FROM {table}")
        connection.execute("DELETE FROM sqlite_sequence WHERE name IN ('pages','blocks','database_properties','custom_page_properties')")
        connection.execute("DELETE FROM settings WHERE key IN ('last_page','dock_layout') OR key LIKE 'notepad-import:%'")
        connection.execute("INSERT INTO settings(key,value) VALUES(?,?)", (f"notion-import:{self.stamp}", str(self.args.export)))

    def _times(self, node: Node) -> tuple[int, int]:
        now = int(time.time())
        legacy = self.legacy.times.get(node.notion_id)
        if legacy and legacy[0]:
            return legacy
        for key, value in node.row_values.items():
            if key.strip().lower() in CREATED_COLUMNS:
                stamp = notion_date_timestamp(value)
                if stamp:
                    return stamp, stamp
        return now, now

    def _insert_node(self, connection: sqlite3.Connection, node: Node) -> None:
        created, updated = self._times(node)
        parent_id = node.parent.target_id if node.parent else None
        icon = "▦" if node.kind == "database" else "📄"
        cover = self.legacy.covers.get(node.notion_id)
        cover_path = str(cover.destination) if cover and cover.destination and cover.exists else None
        self.report.covers += int(cover_path is not None)
        full_width = 1 if "notebook" in (node.title or "").lower() else 0
        cursor = connection.execute(
            "INSERT INTO pages(parent_id,title,icon,favorite,cover_image,sort_order,full_width,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?)",
            (parent_id, node.title, icon, int(node.notion_id in self.legacy.favorites), cover_path,
             node.sort_order, full_width, created, updated),
        )
        node.target_id = int(cursor.lastrowid)
        if node.kind == "database":
            self._insert_database(connection, node)
        elif node.is_record and node.parent:
            self._insert_values(connection, node)
        if not node.is_record and node.kind == "page":
            for order, child in enumerate(node.children):
                child.sort_order = order

    def _insert_database(self, connection: sqlite3.Connection, node: Node) -> None:
        view_mode, columns = self.legacy.view_modes.get(node.notion_id, ("table", 3))
        connection.execute(
            "INSERT INTO databases(page_id,view_mode,sort_key,sort_descending,gallery_columns) VALUES(?,?,'manual',0,?)",
            (node.target_id, view_mode, columns),
        )
        node.property_ids = {}
        for prop in self.schemas.get(node.notion_id, []):
            cursor = connection.execute(
                "INSERT INTO database_properties(database_page_id,name,kind,options,position) VALUES(?,?,?,?,?)",
                (node.target_id, prop["name"], prop["kind"], prop["options"], prop["position"]),
            )
            node.property_ids[prop["name"]] = (int(cursor.lastrowid), prop["kind"])
        self.page_blocks.setdefault(node.notion_id, [])

    def _insert_values(self, connection: sqlite3.Connection, node: Node) -> None:
        property_ids = getattr(node.parent, "property_ids", {})
        for name, (property_id, kind) in property_ids.items():
            raw = node.row_values.get(name, "")
            value = convert_value(kind, raw)
            if value is None:
                self.report.invalid_values.append(f"{node.parent.title} / {node.title} → `{name}` = {raw!r}")
                value = ""
            if value or kind == "checkbox":
                connection.execute(
                    "INSERT INTO page_property_values(page_id,property_id,value) VALUES(?,?,?)",
                    (node.target_id, property_id, value),
                )

    def _insert_unmatched_page(self, connection: sqlite3.Connection) -> None:
        if not self.unmatched_items:
            return
        cursor = connection.execute(
            "INSERT INTO pages(parent_id,title,icon,sort_order) VALUES(NULL,?,'📎',?)",
            (UNMATCHED_PAGE_TITLE, len(self.export.roots)),
        )
        page_id = int(cursor.lastrowid)
        blocks: list[BlockRow] = [BlockRow("text", "Yeni Notion export'unda hiçbir sayfayla eşleşmeyen ekler.")]
        for item in sorted(self.unmatched_items, key=lambda i: i.name.lower()):
            ext = extension(item.name) or extension(item.relative_path)
            kind = "image" if ext in IMAGE_EXT else "video" if ext in VIDEO_EXT else None
            blocks.append(BlockRow(kind, str(item.destination)) if kind else
                          BlockRow("text", f"📎 {item.name} — {item.destination}"))
        self._write_blocks(connection, page_id, blocks)

    def _resolve_database_block(self, block: BlockRow) -> BlockRow:
        if block.kind == "database" and block.content.startswith("@db:"):
            target = self.export.by_id.get(block.content[4:])
            if target and target.target_id:
                return BlockRow("database", str(target.target_id))
            return BlockRow("text", "↗ database")
        return block

    def _insert_blocks(self, connection: sqlite3.Connection) -> None:
        print("Writing blocks…", flush=True)
        for node in self._walk():
            blocks = [self._resolve_database_block(b) for b in self.page_blocks.get(node.notion_id, [])]
            self._write_blocks(connection, node.target_id, blocks)

    def _write_blocks(self, connection: sqlite3.Connection, page_id: int, blocks: list[BlockRow]) -> None:
        if not blocks:
            blocks = [BlockRow("text", "")]
        connection.executemany(
            "INSERT INTO blocks(page_id,position,kind,content,checked,text_styles) VALUES(?,?,?,?,?,?)",
            [(page_id, position, b.kind, b.content, b.checked,
              json.dumps(b.styles, ensure_ascii=False) if b.styles else "[]")
             for position, b in enumerate(blocks)],
        )

    def cleanup_archive(self) -> None:
        root = self.legacy.attachment_root
        for directory in sorted((p for p in root.rglob("*") if p.is_dir()), key=lambda p: len(p.parts), reverse=True):
            if not any(directory.iterdir()):
                directory.rmdir()
        if root.is_dir() and not any(root.iterdir()):
            root.rmdir()

    # -- report
    def write_report(self, moves: list[tuple[str, str]], dry_run: bool) -> Path:
        out_dir = self.data_dir / "imports" / f"notion-{self.stamp}"
        out_dir.mkdir(parents=True, exist_ok=True)
        r = self.report
        matched_files = sum(1 for i in self.legacy.attachments.values() if i.destination and i not in self.unmatched_items)

        def section(title: str, items: list[str], limit: int = 400) -> str:
            if not items:
                return f"## {title}\n\nYok.\n"
            body = "\n".join(f"- {item}" for item in items[:limit])
            more = f"\n- … ve {len(items) - limit:,} tane daha (tam liste JSON raporda)" if len(items) > limit else ""
            return f"## {title} ({len(items):,})\n\n{body}{more}\n"

        tables = Counter(r.converted_tables)
        lines = [
            f"# Notion import raporu — {self.stamp}{' (DRY RUN)' if dry_run else ''}",
            "",
            f"- Kaynak: `{self.args.export}`",
            f"- Sayfa: **{r.counts['page']:,}**, veritabanı: **{r.counts['database']:,}**, kayıt: **{r.counts['record']:,}**",
            f"- Blok: **{r.counts['block']:,}** ({', '.join(f'{k[6:]}: {v:,}' for k, v in sorted(r.counts.items()) if k.startswith('block:'))})",
            f"- Property tipleri: {', '.join(f'{k}: {v}' for k, v in sorted(r.property_kinds.items()))}",
            f"- Eski Noteon şemasından tipi/seçenekleri alınan property: {r.hinted_properties:,}",
            f"- Eşleşen ve taşınan ek dosyası: **{matched_files:,}**, taşınan dosya: {len(moves):,}",
            f"- Eşleşmeyen ek (→ `attachments/unmatched` + \"{UNMATCHED_PAGE_TITLE}\" sayfası): **{len(r.unmatched_attachments):,}**",
            f"- Kapak görseli atanan sayfa: {r.covers:,}",
            f"- Eski veride karşılığı olmayan yeni Notion sayfası: {r.new_pages_without_old:,}",
            "",
            section("Tipi text'e düşürülen property'ler", r.downgraded_properties),
            section("Geçersiz property değerleri (boş bırakıldı)", r.invalid_values),
            section("Eşleşmeyen ek bağlantıları (sayfada 'eşleşmeyen ek' satırı olarak duruyor)", r.unmatched_links),
            section("Zayıf eşleşmeler (kontrol önerilir)", r.weak_matches),
            section("Gövdede bağlantısı olmayan, sayfa sonuna eklenen ekler", r.appended_attachments),
            section("Eksik dosyalar", r.missing_files),
            section("Eşleşmeyen ekler", r.unmatched_attachments),
            section("Yeni export'ta bulunmayan eski sayfalar (ekleri eşleşmeyenlere gitti)", r.old_pages_missing),
            section("CSV satırı olup .md sayfası olmayan kayıtlar (boş kayıt oluşturuldu)", r.rows_without_page),
            section("CSV'de satırı olmayan kayıt sayfaları (değerler .md'den okundu)", r.records_without_row),
            section("Çözülemeyen sayfa bağlantıları", r.unresolved_links),
            section("Harici (URL) görseller — yerel dosya olmadığı için metin olarak kaldı", r.external_media),
            section("Metne dönüştürülen tablolar (sayfa başına)", [f"{k} ({v})" for k, v in tables.items()]),
            section("Hiyerarşiye bağlanamayan dosyalar (köke eklendi)", r.orphan_files),
        ]
        report_path = out_dir / "import-report.md"
        report_path.write_text("\n".join(lines), encoding="utf-8")
        payload = {key: (dict(value) if isinstance(value, Counter) else value) for key, value in r.__dict__.items()}
        payload["moves"] = moves
        (out_dir / "import-report.json").write_text(json.dumps(payload, ensure_ascii=False, indent=1), encoding="utf-8")
        return report_path

    def run(self) -> None:
        self.plan()
        if self.args.dry_run:
            path = self.write_report([], True)
            print(f"Dry run report: {path}")
            return
        print("Moving attachments…", flush=True)
        moves = self.move_files()
        manifest = self.data_dir / "imports" / f"notion-{self.stamp}"
        manifest.mkdir(parents=True, exist_ok=True)
        (manifest / "moves.json").write_text(json.dumps(moves, ensure_ascii=False), encoding="utf-8")
        try:
            print("Writing database…", flush=True)
            self.write_database()
        except Exception:
            print("Database write failed; moving attachments back…", file=sys.stderr, flush=True)
            for source, destination in reversed(moves):
                os.replace(destination, source)
            raise
        self.cleanup_archive()
        path = self.write_report(moves, False)
        print(f"Done. Report: {path}")


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--export", type=Path, required=True)
    parser.add_argument("--data-dir", type=Path, default=Path(os.environ["LOCALAPPDATA"]) / "Kargalar/Noteon/data")
    parser.add_argument("--archive-name", default="notepad-20261015T122857Z")
    parser.add_argument("--dry-run", action="store_true")
    return parser.parse_args()


def main() -> int:
    sys.stdout.reconfigure(encoding="utf-8")
    sys.stderr.reconfigure(encoding="utf-8")
    Importer(parse_args()).run()
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

