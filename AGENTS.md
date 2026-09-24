# mdrepo-rs

Cargo workspace holding the MDRepo processing engine and its supporting tools.
Runs on the processing VM, not in Kubernetes.

| Crate | Role |
|---|---|
| `mdr-process` | The processing engine. `ticket` is what the queue worker runs |
| `mdr-meta` | Metadata validator and generator. Also invoked by the Django API |
| `libmdrepo` | Shared metadata types, validation rules, and constants |
| `mdr-db` | Diesel mirror of the Django-owned schema |
| `mdr-export` | Exports simulations to JSON/TOML |
| `mdr-webcheck` | Site endpoint checker |

## Commands

```console
just all          # install mdr-meta, mdr-process, mdr-export
just mdr-process  # cargo install --path . --locked
just update       # bump dependencies deliberately, then review Cargo.lock
cargo test
```

Deploys pin to `Cargo.lock` via `--locked`. Do not run `cargo update` as part of
a deploy; dependency bumps are a separate reviewable change.

## Things to know

- **`mdr-process` shells out constantly** — to `uv`, `gocmd`, `gmx`, `blastp`,
  `cpptraj`, `mdcompress`, `molly`, and Python helpers in
  `simulation-processing/python/`. Most "not found" failures are a missing `PATH`
  entry under cron, not a code defect.
- **`libmdrepo::metadata::Meta::check` is the contract with contributors.** It is
  the single source of validation truth, reached three ways: `mdr-meta check` on
  the contributor's machine, the Django `verify_metadata` endpoint at submission,
  and `validate::validate_meta` during processing.
- **`#[serde(deny_unknown_fields)]` on `Meta`** — a typo'd TOML key is a parse
  failure, not a silent ignore.
- **Exit codes are load-bearing.** `mdr-meta` exits 0 for valid, 1 for invalid
  metadata (findings on stdout), 2 when the check could not run. Callers must be
  able to tell "your TOML is bad" from "the check never ran" — do not collapse them.
- **The manifest check must never parse the metadata TOML.** `ticket::check_manifest`
  is a transfer-integrity check only; keeping it independent is what keeps a bad
  download distinguishable from a bad submission. There is a test asserting this.
- **Processing rewrites the metadata TOML in place**, so verification against the
  manifest has to happen first or there is nothing left to verify against.
- **Mutation starts at `run_import`.** Fetch, validate, convert and BLAST write
  nothing durable outside the scratch directory. That boundary is what makes a
  pre-import failure safe to re-run and a later one not.
- **A landing is only "done" on two agreeing signals**: `successful = true` on the
  upload instance and `is_placeholder = false` on the simulation. `is_placeholder`
  is cleared only from a push verified by MD5 and presence.
- **Ligand SMILES are canonicalized in memory, never on disk.** The submitter's
  TOML is preserved as sent. Results come back by position, so the index tuple in
  `load_canonical_meta` is load-bearing — dropping it attaches one ligand's
  structure to another's record with no error anywhere.
- **`rustfmt.toml` is checked in.** Match it.
