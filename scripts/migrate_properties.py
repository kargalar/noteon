"""Migrate and normalize all properties in Noteon database.

1. Creates a full snapshot backup before making any changes.
2. Classifies and updates database_properties and page_property_values.
3. Classifies and updates custom_page_properties and custom_page_property_values.
4. Validates 100% of rows against Noteon's exact property validation rules.
"""

from __future__ import annotations

import os
import re
import sys
import sqlite3
from datetime import datetime, timezone
from pathlib import Path
from collections import Counter, defaultdict

sys.stdout.reconfigure(encoding='utf-8')


def utc_stamp() -> str:
    return datetime.now(timezone.utc).strftime("%Y%m%dT%H%M%SZ")


def is_iso_date(value: str) -> bool:
    if not value or not value.isascii():
        return False
    parts = value.split('-')
    if len(parts) != 3:
        return False
    y, m, d = parts
    if len(y) != 4 or len(m) != 2 or len(d) != 2:
        return False
    try:
        year = int(y)
        month = int(m)
        day = int(d)
    except ValueError:
        return False
    if month < 1 or month > 12:
        return False
    if month in (1, 3, 5, 7, 8, 10, 12):
        days = 31
    elif month in (4, 6, 9, 11):
        days = 30
    elif month == 2:
        if year % 4 == 0 and (year % 100 != 0 or year % 400 == 0):
            days = 29
        else:
            days = 28
    else:
        return False
    return 1 <= day <= days


def validate_property_value(kind: str, raw_options: str, value: str) -> bool:
    if kind == "number":
        if not value:
            return True
        try:
            f = float(value)
            return not (f != f or f == float('inf') or f == float('-inf'))
        except ValueError:
            return False
    elif kind == "date":
        if not value:
            return True
        return is_iso_date(value)
    elif kind == "checkbox":
        return value in ("true", "false")
    elif kind in ("select", "status"):
        if not value:
            return True
        opts = [line.strip() for line in raw_options.splitlines() if line.strip()]
        return value in opts
    elif kind == "multi-select":
        if not value:
            return True
        opts = [line.strip() for line in raw_options.splitlines() if line.strip()]
        parts = [p.strip() for p in value.split('|') if p.strip()]
        return all(p in opts for p in parts)
    return True


def normalize_date(val: str) -> str:
    if not val:
        return ""
    val = val.strip()
    if '–' in val:
        candidate = val.split('–')[0].strip()
        if is_iso_date(candidate):
            return candidate
    if ' - ' in val:
        candidate = val.split(' - ')[0].strip()
        if is_iso_date(candidate):
            return candidate
    if is_iso_date(val):
        return val
    m = re.match(r'^(\d{1,2})\.(\d{1,2})\.(\d{4})$', val)
    if m:
        d, mo, y = m.groups()
        candidate = f"{y}-{int(mo):02d}-{int(d):02d}"
        if is_iso_date(candidate):
            return candidate
    return val


def classify_property(name: str, values: list[str], current_kind: str, current_options: str):
    clean_vals = [v.strip() for v in values if v is not None and v.strip() != '']
    unique_vals = sorted(list(set(clean_vals)))
    lower_vals = {v.lower() for v in unique_vals}
    lower_name = name.strip().lower()

    # 0. Always text
    if lower_name in {'name', 'title', 'başlık', 'isim', 'ad', 'not', 'note', 'notes', 'açıklama', 'description', 'anlam', 'adress', 'address', 'url', 'link', 'password', 'şifre', 'highlights', 'succes bar', 'column 2'}:
        return 'text', '', {v: v for v in unique_vals}
    if 'kapak' in lower_name or 'cover' in lower_name:
        return 'text', '', {v: v for v in unique_vals}

    # 1. Existing date property or date by name/values
    date_names = {'tarih', 'date', 'zaman', 'time', 'created', 'created time', 'updated', 'due', 'due date', 'deadline', 'doğum günü', 'birthday', 'travel dates', 'start date', 'end date', 'created date'}
    all_dates = clean_vals and all(is_iso_date(normalize_date(v)) for v in clean_vals)
    if current_kind == 'date' or (lower_name in date_names and (not clean_vals or all_dates)) or all_dates:
        if clean_vals:
            val_map = {v: normalize_date(v) for v in unique_vals if is_iso_date(normalize_date(v))}
            return 'date', '', val_map
        elif current_kind == 'date' or lower_name in date_names:
            return 'date', '', {}

    # 2. Checkbox
    boolean_values = {'yes', 'no', 'true', 'false', 'evet', 'hayır', '1', '0'}
    known_checkbox_names = {
        'completed', 'checkbox', 'favori', 'favorite', 'gizli içerik',
        'sinemada', 'kendime', 'warning', 'yaşanlanlar', 'yüklenemekmiş',
        'videolar', 'on wishlist', 'whishlist', 'ana kampüs'
    }
    is_bool_vals = lower_vals and lower_vals.issubset(boolean_values)
    if is_bool_vals or (lower_name in known_checkbox_names and (not lower_vals or lower_vals.issubset(boolean_values))):
        val_map = {}
        for v in unique_vals:
            lv = v.lower()
            if lv in {'yes', 'true', 'evet', '1'}:
                val_map[v] = 'true'
            elif lv in {'no', 'false', 'hayır', '0'}:
                val_map[v] = 'false'
            else:
                val_map[v] = 'false'
        return 'checkbox', '', val_map

    # 3. Number
    def is_num(val):
        try:
            f = float(val.replace(',', '.'))
            return not (len(val) == 10 and val[4] == '-' and val[7] == '-')
        except ValueError:
            return False

    all_nums = clean_vals and all(is_num(v) for v in clean_vals)
    known_number_names = {
        'puan', 'point', 'rating', 'skor', 'score', 'average', 'benim gibi',
        'güvenilir', 'güzel', 'kültürlü', 'ne kadar seviyorum', 'saf', 'zeki',
        'iyi insan', 'completed units', 'total units', 'total pages', 'current page',
        'curiosity', 'yorumlar', 'progress', 'i̇slam', 'gani', 'happy', 'page/duration'
    }
    if current_kind == 'number' or all_nums or (lower_name in known_number_names and (not clean_vals or all_nums)):
        val_map = {}
        for v in unique_vals:
            if is_num(v):
                val_map[v] = v.replace(',', '.')
            elif v.lower() in {'not rated', 'n/a', 'none', '-', 'yok'}:
                val_map[v] = ""
            else:
                val_map[v] = ""
        return 'number', '', val_map

    # 4. Multi-select
    multi_select_names = {'reaction', 'persons', 'recommender/birlikte', 'type', 'tür', 'tags', 'etiket', 'etiketler'}
    has_delimiters = any(',' in v or '|' in v for v in clean_vals)
    if current_kind == 'multi-select' or lower_name in multi_select_names or has_delimiters:
        all_tokens = set()
        for v in clean_vals:
            tokens = [t.strip() for t in re.split(r'[,|]', v) if t.strip()]
            all_tokens.update(tokens)
        if len(all_tokens) <= 60 and all(len(t) < 40 and '\n' not in t for t in all_tokens) and (has_delimiters or lower_name in multi_select_names):
            sorted_opts = sorted(list(all_tokens))
            val_map = {}
            for v in unique_vals:
                tokens = [t.strip() for t in re.split(r'[,|]', v) if t.strip()]
                val_map[v] = '|'.join(tokens)
            return 'multi-select', '\n'.join(sorted_opts), val_map

    # 5. Status
    status_names = {'status', 'durum', 'aşama', 'stage', 'state', 'itinerary status'}
    if current_kind == 'status' or lower_name in status_names:
        opts = [o.strip() for o in current_options.splitlines() if o.strip()]
        for v in unique_vals:
            if v and v not in opts:
                opts.append(v)
        if not opts:
            opts = ["To do", "In progress", "Done"]
        return 'status', '\n'.join(opts), {v: v for v in unique_vals}

    # 6. Select
    if current_kind == 'select' or (clean_vals and len(unique_vals) <= 15 and len(clean_vals) >= 2):
        if all(len(v) < 40 and '\n' not in v and not v.startswith('http') and not v.endswith('.md)') for v in unique_vals):
            opts = [o.strip() for o in current_options.splitlines() if o.strip()]
            for v in unique_vals:
                if v and v not in opts:
                    opts.append(v)
            return 'select', '\n'.join(opts), {v: v for v in unique_vals}

    return 'text', '', {v: v for v in unique_vals}


def main() -> None:
    data_dir = Path(os.environ["LOCALAPPDATA"]) / "Kargalar/Noteon/data"
    target_db = data_dir / "noteon.sqlite"
    if not target_db.is_file():
        print(f"Error: database not found at {target_db}")
        sys.exit(1)

    # 1. Snapshot backup
    stamp = utc_stamp()
    backup_path = data_dir / "backups" / f"pre-properties-update-{stamp}.sqlite"
    backup_path.parent.mkdir(parents=True, exist_ok=True)
    print(f"Creating snapshot backup: {backup_path}...")
    source_conn = sqlite3.connect(target_db)
    backup_conn = sqlite3.connect(backup_path)
    try:
        source_conn.backup(backup_conn)
    finally:
        backup_conn.close()
        source_conn.close()
    print("Backup completed successfully.")

    # 2. Open connection for migration
    conn = sqlite3.connect(target_db)
    conn.execute("PRAGMA foreign_keys = ON")

    # Initial stats
    initial_db_kinds = Counter(r[0] for r in conn.execute("SELECT kind FROM database_properties").fetchall())
    initial_custom_kinds = Counter(r[0] for r in conn.execute("SELECT kind FROM custom_page_properties").fetchall())

    print("\nInitial database_properties kinds:", dict(initial_db_kinds))
    print("Initial custom_page_properties kinds:", dict(initial_custom_kinds))

    conn.execute("BEGIN IMMEDIATE")

    # Process database_properties
    db_props = conn.execute("SELECT id, database_page_id, name, kind, options FROM database_properties").fetchall()
    db_prop_by_parent_name = {}

    db_updated_count = 0
    val_updated_count = 0

    for pid, db_id, name, kind, options in db_props:
        rows = conn.execute("SELECT page_id, value FROM page_property_values WHERE property_id=?", (pid,)).fetchall()
        vals = [r[1] for r in rows if r[1] != '']
        new_kind, new_options, val_map = classify_property(name, vals, kind, options)

        db_prop_by_parent_name[(db_id, name)] = (new_kind, new_options, val_map)

        if new_kind != kind or new_options != options:
            conn.execute(
                "UPDATE database_properties SET kind=?, options=? WHERE id=?",
                (new_kind, new_options, pid),
            )
            db_updated_count += 1

        # If checkbox, ensure every record in database has a row with 'true' or 'false'
        if new_kind == "checkbox":
            # Get all record pages for this database
            records = conn.execute("SELECT id FROM pages WHERE parent_id=?", (db_id,)).fetchall()
            existing_page_ids = {r[0] for r in rows}
            for rec_id, in records:
                if rec_id not in existing_page_ids:
                    conn.execute(
                        "INSERT INTO page_property_values(page_id, property_id, value) VALUES(?, ?, 'false')",
                        (rec_id, pid),
                    )
                    val_updated_count += 1

        # Update values
        for page_id, old_val in rows:
            if new_kind == "checkbox":
                lv = old_val.strip().lower()
                new_val = "true" if lv in {"yes", "true", "evet", "1"} else "false"
            elif new_kind == "multi-select":
                parts = [t.strip() for t in re.split(r'[,|]', old_val) if t.strip()]
                new_val = "|".join(parts)
            elif new_kind == "date":
                new_val = normalize_date(old_val)
            elif new_kind == "number":
                nv = old_val.replace(',', '.').strip()
                if nv.lower() in {'not rated', 'n/a', 'none', '-', 'yok'}:
                    new_val = ""
                else:
                    try:
                        float(nv)
                        new_val = nv
                    except ValueError:
                        new_val = ""
            elif new_kind in ("select", "status"):
                new_val = old_val.strip()
            else:
                new_val = old_val

            if new_val != old_val:
                conn.execute(
                    "UPDATE page_property_values SET value=? WHERE page_id=? AND property_id=?",
                    (new_val, page_id, pid),
                )
                val_updated_count += 1

    print(f"Updated {db_updated_count} database_properties and {val_updated_count} page_property_values rows.")

    # Process custom_page_properties
    custom_rows = conn.execute("""
        SELECT cpp.id, cpp.page_id, cpp.name, cpp.kind, cpp.options, p.parent_id, COALESCE(cppv.value, '')
        FROM custom_page_properties cpp
        JOIN pages p ON p.id = cpp.page_id
        LEFT JOIN custom_page_property_values cppv ON cppv.property_id = cpp.id
    """).fetchall()

    custom_props_data = defaultdict(lambda: {'name': '', 'kind': '', 'options': '', 'page_id': 0, 'parent_id': None, 'values': []})
    for pid, page_id, name, kind, options, parent_id, val in custom_rows:
        custom_props_data[pid]['name'] = name
        custom_props_data[pid]['kind'] = kind
        custom_props_data[pid]['options'] = options
        custom_props_data[pid]['page_id'] = page_id
        custom_props_data[pid]['parent_id'] = parent_id
        custom_props_data[pid]['values'].append(val)

    custom_updated_count = 0
    custom_val_updated_count = 0

    for pid, pdata in custom_props_data.items():
        name = pdata['name']
        kind = pdata['kind']
        options = pdata['options']
        parent_id = pdata['parent_id']
        vals = pdata['values']
        page_id = pdata['page_id']
        non_empty = [v for v in vals if v != '']

        if parent_id and (parent_id, name) in db_prop_by_parent_name:
            new_kind, new_options, val_map = db_prop_by_parent_name[(parent_id, name)]
        else:
            new_kind, new_options, val_map = classify_property(name, non_empty, kind, options)

        # Build options for custom prop if needed
        all_new_vals = []
        for old_val in vals:
            if new_kind == "checkbox":
                lv = old_val.strip().lower()
                new_val = "true" if lv in {"yes", "true", "evet", "1"} else "false"
            elif new_kind == "multi-select":
                parts = [t.strip() for t in re.split(r'[,|]', old_val) if t.strip()]
                new_val = "|".join(parts)
                opts = [o.strip() for o in new_options.splitlines() if o.strip()]
                for p in parts:
                    if p not in opts:
                        opts.append(p)
                new_options = "\n".join(opts)
            elif new_kind == "date":
                new_val = normalize_date(old_val)
            elif new_kind == "number":
                nv = old_val.replace(',', '.').strip()
                if nv.lower() in {'not rated', 'n/a', 'none', '-', 'yok'}:
                    new_val = ""
                else:
                    try:
                        float(nv)
                        new_val = nv
                    except ValueError:
                        new_val = ""
            elif new_kind in ("select", "status"):
                new_val = old_val.strip()
                opts = [o.strip() for o in new_options.splitlines() if o.strip()]
                if new_val and new_val not in opts:
                    opts.append(new_val)
                    new_options = "\n".join(opts)
            else:
                new_val = old_val
            all_new_vals.append((old_val, new_val))

        if new_kind != kind or new_options != options:
            conn.execute(
                "UPDATE custom_page_properties SET kind=?, options=? WHERE id=?",
                (new_kind, new_options, pid),
            )
            custom_updated_count += 1

        for old_val, new_val in all_new_vals:
            # Check if row exists in custom_page_property_values
            row_exists = conn.execute(
                "SELECT 1 FROM custom_page_property_values WHERE property_id=? AND page_id=?",
                (pid, page_id),
            ).fetchone()

            if not row_exists:
                if new_kind == "checkbox" or new_val:
                    conn.execute(
                        "INSERT INTO custom_page_property_values(page_id, property_id, value) VALUES(?, ?, ?)",
                        (page_id, pid, new_val if new_val else "false"),
                    )
                    custom_val_updated_count += 1
            elif new_val != old_val:
                conn.execute(
                    "UPDATE custom_page_property_values SET value=? WHERE page_id=? AND property_id=?",
                    (new_val, page_id, pid),
                )
                custom_val_updated_count += 1

    print(f"Updated {custom_updated_count} custom_page_properties and {custom_val_updated_count} custom_page_property_values rows.")

    # 3. Post-validation: verify EVERY row in database
    print("\nValidating all rows against Noteon rules...")
    val_errors = 0
    total_validated = 0

    # Validate database property values
    db_check = conn.execute("""
        SELECT dp.id, dp.name, dp.kind, dp.options, ppv.page_id, ppv.value
        FROM database_properties dp
        JOIN page_property_values ppv ON ppv.property_id = dp.id
    """).fetchall()

    for pid, name, kind, opts, pg_id, val in db_check:
        total_validated += 1
        if not validate_property_value(kind, opts, val):
            val_errors += 1
            if val_errors <= 10:
                print(f"[ERROR DB] Prop {pid} '{name}' ({kind}): val='{val}'")

    # Validate custom page property values
    custom_check = conn.execute("""
        SELECT cpp.id, cpp.name, cpp.kind, cpp.options, cppv.page_id, cppv.value
        FROM custom_page_properties cpp
        JOIN custom_page_property_values cppv ON cppv.property_id = cpp.id
    """).fetchall()

    for pid, name, kind, opts, pg_id, val in custom_check:
        total_validated += 1
        if not validate_property_value(kind, opts, val):
            val_errors += 1
            if val_errors <= 10:
                print(f"[ERROR CUSTOM] Prop {pid} '{name}' ({kind}): val='{val}'")

    print(f"Validated {total_validated:,} values with {val_errors} errors.")

    if val_errors > 0:
        print("Aborting transaction due to validation errors!")
        conn.rollback()
        sys.exit(1)

    # Commit
    conn.commit()
    print("\nTransaction committed successfully!")

    final_db_kinds = Counter(r[0] for r in conn.execute("SELECT kind FROM database_properties").fetchall())
    final_custom_kinds = Counter(r[0] for r in conn.execute("SELECT kind FROM custom_page_properties").fetchall())

    print("\n=== FINAL RESULTS ===")
    print("database_properties kinds before vs after:")
    for k in sorted(set(initial_db_kinds.keys()) | set(final_db_kinds.keys())):
        print(f"  {k:15}: {initial_db_kinds.get(k, 0):5} -> {final_db_kinds.get(k, 0):5}")

    print("\ncustom_page_properties kinds before vs after:")
    for k in sorted(set(initial_custom_kinds.keys()) | set(final_custom_kinds.keys())):
        print(f"  {k:15}: {initial_custom_kinds.get(k, 0):5} -> {final_custom_kinds.get(k, 0):5}")


if __name__ == "__main__":
    main()
