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
