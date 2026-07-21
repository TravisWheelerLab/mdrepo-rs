# mdr-process — planned work

## Background: upload-instance / ticket feedback

MDRepo records per-upload feedback in two Postgres tables (Django models in
`md-repo-app/md_repo/md_repo_app/models/simulation_upload_instance/`):

- `md_upload_instance` (`SimulationUploadInstance`) — one row per upload landing.
  Key fields: `ticket_id`, `landing_id`, `user_id`, `lead_contributor_orcid`,
  `filenames`, `simulation_id`, `successful`, `created_on`.
- `md_upload_instance_message` (`SimulationUploadStatusMessage`) — timestamped
  log/error/warning lines: `simulation_upload_id`, `timestamp`, `message`,
  `is_error`, `is_warning`.

`mdr-db` already models all three tables (`md_upload_instance`,
`md_upload_instance_message`, `md_ticket`) with insert/update ops
(`mdr-db/src/ops.rs`). `mdr-export/src/main.rs` is the precedent for connecting:
read `PRODUCTION_DSN` / `STAGING_DSN` from env → `mdr_db::connect(&url)`.

### Key fact: `landing_id` == subdirectory basename

`irods_tickets` (built in `md-repo-app/.../irods_utils.py::upload_tickets`) is a
`;`-separated list of `"{ticket.string}:{path}"`, where for uploads
`ticket.string = MDRSubmit_<b32token>_<i>` and `path = .../<b32token>_<i>`.
`check_new_simulations.py`'s `TICKET_RE = ^MDRSubmit_([^:]+):(.+)$` captures
`landing_id = <b32token>_<i>`, which equals `basename(path)` == the iRODS
collection name == the local subdir `fetch_uploads.py` creates
(`ticket-<id>/<coll.name>`). So in Rust, **the subdirectory basename is the
canonical `landing_id`**; upsert on `(ticket_id, landing_id)`.

---

## Task C — populate the new `md_simulation` columns  (SHELVED — later)

Five columns were added to `md_simulation` and mirrored into `mdr-db`
(`schema.rs` + `Simulation` / `NewSimulation` / `SimulationUpdate` in
`models.rs`): `is_embargoed`, `is_coarse_grained`, `num_replicates`,
`irods_ticket`, `superseding_simulation_id`. Four of the five are now settled:

- `is_embargoed`, `is_coarse_grained`, `num_replicates` — **done.** The Rust
  importer writes all three (`import.rs::upsert_simulation`), on both the insert
  and the update branch. Verified on staging simulation 80.
- `irods_ticket` — **not ours.** Django creates it on the first request if it is
  missing, so `import.rs` deliberately inserts `None`.
- `superseding_simulation_id` — still open: origin/trigger unclear (supersession
  workflow?). Determine who sets it before wiring.

Note the write path moved: simulation rows come from Rust now
(`import.rs`, via `mdr-db`), not from `import_preprocessed.py`, so what is left
here is a Rust change rather than a Python one.

---

## Task D — where do warnings go on a non-ticket `process` run?  (OPEN QUESTION)

`md_import_warning` was dropped (Django migration `0253`) because it duplicated
what already reaches `md_upload_instance_message`. But that table is
**ticket-scoped**: `ticket.rs` logs `result.warnings` against an upload instance,
and a direct `mdr-process process` run (a reprocess, or any standalone
invocation) has no ticket and therefore no upload instance to hang them on.

So for non-ticket runs, warnings now survive only in `import.json` on disk and in
the log output — nothing in the database. The Rust importer does not carry them
at all (see the divergence note in `import.rs`'s header).

Decide whether that is acceptable. If it is not, the options are roughly:
- give `md_upload_instance` a nullable ticket so standalone runs can own one,
- add a simulation-scoped warnings table (i.e. reinstate something like
  `md_import_warning`, which is the thing we just removed), or
- accept the log/`import.json` as the record and close this out.

---

## Task E — tests for `import.rs`  (ACTIVE)

`mdr-process/src/import.rs` has **no tests of its own.** `mdr-db`'s natural-key
finders have 11 (`mdr-db/tests/finders.rs`), but nothing exercises
`import_simulation`, which orchestrates all ten `upsert_*` helpers, both
delete-cascades and the transaction. Today its only coverage is the manual
staging runs described below, which nothing re-runs.

Harness to reuse (already built, see `mdr-db/tests/`): an ephemeral Dockerized
Postgres seeded from a schema-only dump of staging
(`mdr-db/tests/fixtures/schema.sql`), one test per `begin_test_transaction()` so
everything rolls back, skipped when `TEST_DATABASE_URL` is unset. Run it with
`mdr-db/tests/with-testdb.sh`. Regenerate the fixture when Django migrations
change the schema (command in that file's header).

Cases worth covering — these are exactly what was verified by hand against
staging simulation 80 on 2026-07-21, so they are the known-good behavior:
1. **Fresh import** writes the simulation plus every child table, with absent
   text stored as NULL rather than `""`.
2. **Re-import of the same payload does not duplicate child rows** — every
   `find_*` natural-key probe must hit. This is the big one; a regression here
   is silent and only shows up as doubled contributors/files in the UI.
3. **Reprocess** (`reprocess_simulation_id` set) deletes and reinserts the
   processed files, leaves uploaded files alone, and leaves the `md_simulation`
   row otherwise unchanged (`creation_date` included).
4. **`replace_original_files`** additionally clears and reinserts the uploaded
   files, re-deriving `is_primary` and `local_file_path`.
5. **Shared entities are reused, not duplicated** — an existing pub/uniprot/pdb
   /software is found and linked, and no second row appears.
6. **The transaction rolls back as a unit**: force a failure partway through and
   assert no half-imported simulation survives.

---

## Task F — retire `import_preprocessed.py`

The Rust importer replaced it in production on 2026-07-21 (`process.rs` no longer
shells out; the deployed `mdr-process` has no reference to the script). Parity was
confirmed against staging simulation 80 across a fresh import, a reprocess, and a
reprocess with `--replace-original-files`.

What is left is deleting `simulation-processing/python/import_preprocessed.py` and
whatever invokes it. Before doing so:
- Confirm nothing else still calls it — it is also still being run by hand on a
  separate machine for imports, which is why it is still on disk.
- The other copy, `utils/python/misato/import_preprocessed.py`, has no solute
  rename table and looks unrelated; check before touching it.
- Two fixes landed in the script after the port, and both die with it:
  `c802dac` (the `is_coarse_graing` key that reset the flag on every reprocess)
  and the still-unfixed `{"Cl": "Cl+"}` rename, which the Rust importer corrects
  to `Cl-`. Nothing to carry over — noted so neither looks like a regression.
