"""Reject user-directory paths in release files, including ZIP members and UTF-16."""
import argparse
import re
import sys
import zipfile
from pathlib import Path


USER_PATH = re.compile(
    r"(?:[a-z]:[\\/]+Users[\\/]+|/(?:home|Users)/|/mnt/[a-z]/Users/)",
    re.IGNORECASE,
)


def contains_user_path(data):
    # Scan both byte alignments: wide strings can start at odd file offsets.
    return any(
        USER_PATH.search(text)
        for text in (
            data.decode("latin1"),
            data.decode("utf-16le", errors="ignore"),
            data[1:].decode("utf-16le", errors="ignore"),
            data.decode("utf-16be", errors="ignore"),
            data[1:].decode("utf-16be", errors="ignore"),
        )
    )


def check_file(path):
    if path.suffix.lower() == ".zip":
        with zipfile.ZipFile(path) as archive:
            for index, entry in enumerate(archive.infolist(), 1):
                if entry.is_dir():
                    continue
                if (
                    entry.filename.lower().endswith(".pdb")
                    or contains_user_path(entry.filename.encode())
                    or contains_user_path(archive.read(entry))
                ):
                    # Avoid exposing the matching path in public CI logs.
                    raise ValueError(f"ZIP member {index} contains private paths or debug symbols")
    elif path.suffix.lower() == ".pdb" or contains_user_path(path.read_bytes()):
        raise ValueError("file contains private paths or debug symbols")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("files", nargs="+", type=Path)
    args = parser.parse_args()
    for index, path in enumerate(args.files, 1):
        try:
            check_file(path)
        except (OSError, ValueError, zipfile.BadZipFile):
            print(f"Release privacy check failed for input {index}.", file=sys.stderr)
            return 1
    print("Release privacy check passed.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
