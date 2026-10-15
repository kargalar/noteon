"""Replace Noteon's Notepad hard links with independent files, then remove source files.

Run only after the migration has completed and while Notepad is closed. Each file is
copied and checked before its old source entry is removed, so the operation is
resumable if interrupted and does not need a second 45 GB of free space.
"""

from __future__ import annotations

import argparse
import hashlib
import os
import shutil
import sqlite3
import sys
import uuid
from pathlib import Path


def parse_args() -> argparse.Namespace:
    appdata = Path(os.environ["APPDATA"])
    local_appdata = Path(os.environ["LOCALAPPDATA"])
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--source-attachments",
        type=Path,
        default=appdata / "com.kargalar.notepad/workspaces/import-3dccca531225ffc610bf/attachments",
    )
    parser.add_argument(
        "--noteon-attachments",
        type=Path,
        default=local_appdata / "Kargalar/Noteon/data/imports/notepad-20261015T122857Z/attachments",
    )
    parser.add_argument(
        "--source-db",
        type=Path,
        default=appdata / "com.kargalar.notepad/workspaces/import-3dccca531225ffc610bf/workspace.sqlite",
    )
    parser.add_argument("--dry-run", action="store_true")
    return parser.parse_args()


def digest(path: Path) -> tuple[str, int]:
    value = hashlib.sha256()
    size = 0
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(4 * 1024 * 1024), b""):
            size += len(chunk)
            value.update(chunk)
    return value.hexdigest(), size


def ensure_expected_paths(source: Path, destination: Path) -> None:
    source_root = (Path(os.environ["APPDATA"]) / "com.kargalar.notepad/workspaces/import-3dccca531225ffc610bf/attachments").resolve()
    noteon_root = (Path(os.environ["LOCALAPPDATA"]) / "Kargalar/Noteon/data/imports/notepad-20261015T122857Z/attachments").resolve()
    if source.resolve() != source_root:
        raise RuntimeError(f"Unexpected Notepad source directory: {source}")
    if destination.resolve() != noteon_root:
        raise RuntimeError(f"Unexpected Noteon destination directory: {destination}")
    if source.drive.casefold() != destination.drive.casefold():
        raise RuntimeError("Both directories must be on the same drive for safe, resumable moves.")
    if not source.is_dir() or not destination.is_dir():
        raise FileNotFoundError("The source or destination attachments directory was not found.")


def expected_hashes(source_db: Path) -> dict[str, str | None]:
    connection = sqlite3.connect(f"file:{source_db.as_posix()}?mode=ro", uri=True)
    try:
        return {
            row[0].replace("\\", "/"): row[1]
            for row in connection.execute("SELECT relative_path,sha256 FROM media")
        }
    finally:
        connection.close()


def copy_one(source: Path, destination: Path, expected: str | None) -> tuple[int, bool]:
    temp = destination.with_name(f".{destination.name}.{uuid.uuid4().hex}.move-part")
    source_info = source.stat()
    source_hash = hashlib.sha256()
    copied_bytes = 0
    try:
        with source.open("rb") as reader, temp.open("xb") as writer:
            for chunk in iter(lambda: reader.read(4 * 1024 * 1024), b""):
                writer.write(chunk)
                source_hash.update(chunk)
                copied_bytes += len(chunk)
            writer.flush()
            os.fsync(writer.fileno())
        digest_value = source_hash.hexdigest()
        if copied_bytes != source_info.st_size:
            raise RuntimeError(f"File size changed during copy: {source}")
        if expected and digest_value.casefold() != expected.casefold():
            raise RuntimeError(f"The source file's SHA-256 does not match the database record: {source}")
        destination.parent.mkdir(parents=True, exist_ok=True)
        os.replace(temp, destination)
        if destination.stat().st_size != copied_bytes:
            raise RuntimeError(f"Could not verify the destination file size: {destination}")
        return copied_bytes, True
    except Exception:
        temp.unlink(missing_ok=True)
        raise


def detach(source_dir: Path, destination_dir: Path, source_db: Path, dry_run: bool) -> None:
    ensure_expected_paths(source_dir, destination_dir)
    hashes = expected_hashes(source_db)
    source_files = sorted(path for path in source_dir.rglob("*") if path.is_file())
    total_bytes = sum(path.stat().st_size for path in source_files)
    print(f"Kaynakta kalan dosya: {len(source_files):,} ({total_bytes:,} bayt)", flush=True)
    if dry_run:
        for path in source_files[:5]:
            print(f"Moving: {path.relative_to(source_dir)}", flush=True)
        return

    moved = already_detached = removed_source = 0
    bytes_copied = 0
    for index, source in enumerate(source_files, 1):
        relative = source.relative_to(source_dir)
        relative_key = f"attachments/{relative.as_posix()}"
        destination = destination_dir / relative
        expected = hashes.get(relative_key)
        if not destination.exists():
            size, copied = copy_one(source, destination, expected)
            bytes_copied += size
            moved += 1
        elif os.path.samefile(source, destination):
            size, copied = copy_one(source, destination, expected)
            bytes_copied += size
            moved += 1
        else:
            source_hash, source_size = digest(source)
            destination_hash, destination_size = digest(destination)
            if source_size != destination_size or source_hash != destination_hash:
                raise RuntimeError(f"The source and Noteon copies differ; source was not deleted: {relative}")
            if expected and destination_hash.casefold() != expected.casefold():
                raise RuntimeError(f"The Noteon copy failed SHA-256 verification: {relative}")
            already_detached += 1

        if os.path.samefile(source, destination):
            raise RuntimeError(f"Could not create an independent copy; source was not deleted: {relative}")
        source.unlink()
        removed_source += 1
        if index % 50 == 0 or index == len(source_files):
            print(f"Processed {index:,}/{len(source_files):,} files; moved {moved:,}, verified and detached {already_detached:,}.", flush=True)

    remaining = [path for path in source_dir.rglob("*") if path.is_file()]
    if remaining:
        raise RuntimeError(f"{len(remaining):,} files remain in the source directory.")
    print(
        f"Done: removed {removed_source:,} source files; "
        f"created independent copies of {moved:,} files ({bytes_copied:,} bytes); "
        f"{already_detached:,} files were already detached in a previous run.",
        flush=True,
    )


def main() -> int:
    if hasattr(sys.stdout, "reconfigure"):
        sys.stdout.reconfigure(encoding="utf-8")
    if hasattr(sys.stderr, "reconfigure"):
        sys.stderr.reconfigure(encoding="utf-8")
    args = parse_args()
    try:
        detach(args.source_attachments, args.noteon_attachments, args.source_db, args.dry_run)
    except Exception as error:
        print(f"Media move failed: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
