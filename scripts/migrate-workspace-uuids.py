#!/usr/bin/env python3
import sys
import os
import shutil
import sqlite3
import uuid
import datetime

def main():
    if len(sys.argv) < 2:
        print("Usage: python3 migrate-workspace-uuids.py <path_to_sqlite_db> [--execute]")
        sys.exit(1)

    db_path = sys.argv[1]
    execute = "--execute" in sys.argv

    if not os.path.exists(db_path):
        print(f"Error: Database file not found: {db_path}")
        sys.exit(1)

    print(f"=== OpenDocuments Workspace UUID Migration ===")
    print(f"Target DB: {db_path}")
    print(f"Mode: {'EXECUTE' if execute else 'DRY-RUN'}")

    conn = sqlite3.connect(db_path)
    cur = conn.cursor()

    cur.execute("SELECT id, name FROM workspaces")
    rows = cur.fetchall()
    print(f"\nExisting Workspaces ({len(rows)}):")
    mapping = {}
    for ws_id, name in rows:
        print(f"  - id: '{ws_id}', name: '{name}'")
        if ws_id == name:
            mapping[ws_id] = str(uuid.uuid4())

    if not mapping:
        print("\nNo legacy workspaces (where id == name) found. Nothing to migrate.")
        conn.close()
        return

    print(f"\nMigration Map (Legacy id == name -> New UUID):")
    for old_id, new_id in mapping.items():
        print(f"  - '{old_id}' -> '{new_id}'")

    child_tables = [
        ("workspace_members", "workspace_id"),
        ("connectors", "workspace_id"),
        ("documents", "workspace_id"),
        ("tags", "workspace_id"),
        ("conversations", "workspace_id"),
        ("query_logs", "workspace_id"),
        ("collections", "workspace_id"),
        ("api_keys", "workspace_id"),
        ("dictionary", "workspace_id")
    ]

    print("\nChild Tables Count Summary before migration:")
    for old_id in mapping.keys():
        print(f"Workspace '{old_id}':")
        for table, col in child_tables:
            cur.execute(f"SELECT count(*) FROM sqlite_master WHERE type='table' AND name='{table}'")
            if cur.fetchone()[0] > 0:
                cur.execute(f"SELECT count(*) FROM {table} WHERE {col} = ?", (old_id,))
                cnt = cur.fetchone()[0]
                if cnt > 0:
                    print(f"  - {table}.{col}: {cnt} rows")

    if not execute:
        print("\n[DRY-RUN COMPLETE] Pass --execute to apply changes to database.")
        conn.close()
        return

    timestamp = datetime.datetime.now().strftime("%Y%m%d_%H%M%S")
    backup_path = f"{db_path}.backup_{timestamp}"
    print(f"\nCreating safety backup: {backup_path}")
    shutil.copy2(db_path, backup_path)

    try:
        cur.execute("PRAGMA foreign_keys = OFF;")
        cur.execute("BEGIN TRANSACTION;")

        for old_id, new_id in mapping.items():
            for table, col in child_tables:
                cur.execute(f"SELECT count(*) FROM sqlite_master WHERE type='table' AND name='{table}'")
                if cur.fetchone()[0] > 0:
                    cur.execute(f"UPDATE {table} SET {col} = ? WHERE {col} = ?", (new_id, old_id))
            cur.execute("UPDATE workspaces SET id = ? WHERE id = ?", (new_id, old_id))

        cur.execute("COMMIT;")
        cur.execute("PRAGMA foreign_keys = ON;")

        cur.execute("PRAGMA integrity_check;")
        integrity = cur.fetchone()[0]
        cur.execute("PRAGMA foreign_key_check;")
        fk_errors = cur.fetchall()

        print("\nPost-Migration Diagnostics:")
        print(f"  - Integrity check: {integrity}")
        print(f"  - FK check errors: {len(fk_errors)}")

        if integrity != "ok" or len(fk_errors) > 0:
            print("CRITICAL: Diagnostics failed! Restoring backup...")
            conn.close()
            shutil.copy2(backup_path, db_path)
            sys.exit(1)

        cur.execute("SELECT id, name FROM workspaces")
        print(f"\nUpdated Workspaces ({len(cur.fetchall())}):")
        cur.execute("SELECT id, name FROM workspaces")
        for ws_id, name in cur.fetchall():
            print(f"  - id: '{ws_id}', name: '{name}'")

        print("\n[MIGRATION SUCCESSFUL]")

    except Exception as e:
        print(f"CRITICAL ERROR during migration: {e}")
        conn.rollback()
        conn.close()
        print("Restoring backup...")
        shutil.copy2(backup_path, db_path)
        sys.exit(1)

    conn.close()

if __name__ == "__main__":
    main()
