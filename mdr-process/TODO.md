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

## Task A — `ticket` command feedback  (ACTIVE / not shelved)

Goal: on `mdr-process ticket`, record per-landing upload instances + messages,
and set `md_ticket.processing_complete = true` on full success.

Confirmed decisions:
- Use `mdr-db` directly from Rust (not a new Python script).
- All logic lives in `ticket.rs` only — `process::process` stays unchanged
  (reprocess / standalone `process` runs have no ticket / upload instance).

Plan (all in `mdr-process/src/ticket.rs`):
1. Cargo.toml: add `mdr-db = { path = "../mdr-db" }`,
   `diesel = { version = "2.2", features = ["postgres"] }` (to name
   `PgConnection`), `chrono = { version = "0.4", features = ["clock"] }`.
2. Helper `connect(server) -> Result<PgConnection>` mapping `Server` →
   `PRODUCTION_DSN` / `STAGING_DSN` (mirror `mdr-export`).
3. Before the parallel subdir loop: one connection + `ops::get_ticket(conn,
   ticket_id)` to get `created_by_id` (→ `user_id`) and `orcid`
   (fallback `"NA"`, matching the Django `... or "NA"`).
4. In the `into_par_iter()` loop (currently `ticket.rs:118`, change `for_each`
   → `map` returning a success bool), per subdir:
   - `landing_id = subdir.file_name()`.
   - open its own `PgConnection` (Diesel conn isn't shareable across rayon
     tasks).
   - gather `filenames` from the subdir (exclude names starting with
     `mdrepo-submission.` and the `processed/` dir), join with `, `.
   - upsert the instance: `list_upload_instances(.., tkt_id)` filtered to
     matching `landing_id`, else `insert_upload_instance(NewUploadInstance{
     created_on: Utc::now(), ticket_id, landing_id, user_id, orcid, filenames,
     .. })`.
   - call `process::process(..)` (unchanged).
   - best-effort read `<subdir>/processed/imported.json`
     (`types::ImportResult`) for `simulation_id`.
   - write messages via `ops::insert_upload_message`:
     - `Err(e)` → `is_error=true` message, `successful=false`.
     - `Ok(errors)` non-empty → one `is_warning=true` message per line,
       `successful=true`.
     - `Ok([])` → "Processing completed successfully", `successful=true`.
   - `update_upload_instance` with `successful` + `simulation_id`.
5. After the loop: if every subdir succeeded, `ops::update_ticket(conn,
   ticket_id, TicketUpdate { processing_complete: Some(true), ..Default })`.
6. Make all DB writes best-effort (log on failure via `log`) so a DB hiccup
   never aborts processing — but log loudly if the `processing_complete`
   update fails.

Notes / caveats:
- `ticket_id` is `u64` in `TicketArgs`; DB ids are `i64` — cast.
- Reading `imported.json` is a soft coupling to `process.rs` internals; if it's
  missing, leave `simulation_id` NULL rather than failing.

---

## Task C — populate the new `md_simulation` columns  (SHELVED — later)

Five columns were added to `md_simulation` and mirrored into `mdr-db`
(`schema.rs` + `Simulation` / `NewSimulation` / `SimulationUpdate` in
`models.rs`): `is_embargoed`, `is_coarse_grained`, `num_replicates`,
`irods_ticket`, `superseding_simulation_id`. The Rust structs can now read/write
them, but **nothing on the write path populates them yet.**

Simulation rows are written by the Python import path, not by Rust
(`simulation-processing/python/import_preprocessed.py`, fed by the
`ExportSimulation` JSON that `process.rs::make_import_json` emits to
`import.json`). So wiring these up is a **Python-side** change, separate from
Task A (`ticket` feedback) and from `process::process` (unchanged).

Items:
- `irods_ticket`: `process.rs` already mints an iRODS ticket for the simulation
  dir via `create_irods_ticket.py` (`process.rs:335`), but the ticket string is
  not stored on the simulation. Decide whether to persist it into
  `md_simulation.irods_ticket` (would need the Python side to write it, and/or
  `create_irods_ticket.py` to return the ticket for storage).
- `num_replicates`, `is_embargoed`, `is_coarse_grained`: already flow through
  `ExportSimulation` → `import.json` (`process.rs:1128`+). Verify whether
  `import_preprocessed.py` already persists them to the new columns or still
  needs wiring.
- `superseding_simulation_id`: origin/trigger unclear (supersession workflow?).
  Determine who sets it before wiring.

Note: none of this affects Task A — `ticket.rs` only touches `md_ticket`,
`md_upload_instance`, `md_upload_instance_message`.
