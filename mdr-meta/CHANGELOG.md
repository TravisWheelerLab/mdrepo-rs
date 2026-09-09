# Changelog

Notable user-facing changes to `mdr-meta`. Each entry here becomes the
GitHub release note for that version (see `.github/workflows/release.yml`)
-- write it before tagging, not after.

## 0.3.24

### New: `ligands.inchi`, and `ligands.smiles` becomes optional

A ligand may now be declared by a standard InChI instead of, or as well
as, a SMILES:

```toml
[[ligands]]
name = "acetaldehyde"
inchi = "InChI=1S/C2H4O/c1-2-3/h2H,1H3"
```

- **At least one of `smiles` and `inchi` is required**, and either alone
  is enough. Declaring neither is an error naming the ligand.
- The InChI must be a **standard** InChI, beginning `InChI=1S/`. The
  non-standard `InChI=1/` form is refused: two non-standard InChIs are
  not comparable, and comparability is the reason for accepting the
  notation at all.
- Declaring both is allowed. They are checked against each other on the
  ingest path, where a pair describing two different molecules is
  refused rather than reconciled.
- Neither value is ever rewritten in your file.
- Nothing changes for metadata that declares `smiles` and no `inchi`,
  which is every released bundle today.

### Behaviour change: an unknown key under `[[ligands]]` is now an error

A misspelled or stray key inside a `[[ligands]]` table used to be
accepted and silently discarded, so `inchii = "..."` validated clean and
the value simply vanished. It is now a parse failure naming the line.

```
Failed to parse input: TOML parse error at line 66, column 1
   |
66 | inchii = "InChI=1S/C2H4O/c1-2-3/h2H,1H3"
   | ^^^^^^
```

Every other table already behaved this way; `[[ligands]]` was the
exception, because the attribute that enforces it sits on the top-level
document and serde does not cascade it into nested structs.

**Checked before release** against all 97,809 released metadata files:
the same 30 that failed under 0.3.23 fail under 0.3.24, for byte-identical
reasons, and nothing new fails. No published bundle has been carrying a
stray ligand key.

### Behaviour change: `sampling_frequency_ps` has a floor of 1 ps

The accepted range was `0.001-100000` and is now `1-100000`. A
declaration below 1 ps is refused outright, from any source.

This lands in the same release as the InChI work and is otherwise
unrelated to it. It is the validator half of a rule the processing
pipeline already enforces: a trajectory carrying no usable timing is
stamped by the conversion with a default of 1 ps per frame, which is
indistinguishable from a genuine 1 ps, so a spacing that small is not
recorded on the trajectory's word alone.

Below 10 ps the processing pipeline additionally *requires* the
declaration and checks it against the trajectory, agreeing to within 1%.
`check` does not measure trajectories and so cannot apply that half; it
enforces the floor only.

**Checked before release**: only 73 released simulations declare the
field at all, every one of them at 100 ps, so no published metadata
changes verdict.

### Unchanged

- Every other field, every subcommand, and `check`'s exit codes
  (`0`/`1`/`2`, see the 0.3.17 notes below) are untouched.

## 0.3.23

### Behaviour change: a wildcard in a filename is now an error

`check` now rejects `*`, `?` or `[` in any filename the metadata refers
to: `structure_file_name`, `topology_file_name`, every
`trajectory_file_names` entry, and every `additional_files.file_name`.

```toml
trajectory_file_names = ["Pro_lig*.mdc"]
```

```
trajectory_file_names[1]: filename "Pro_lig*.mdc" contains the wildcard
'*'; globs are not expanded, so every file must be listed explicitly
```

**Metadata that passed `check` before may now fail it.** A file like the
one above used to exit `0`; it now exits `1`. If you have been relying on
a pattern to stand for a set of files, list the files instead.

Nothing has ever expanded these patterns. The name was always used
literally, so a pattern simply failed later, during import, as

```
Missing file "Pro_lig*.mdc" referenced by metadata: No such file or
directory (os error 2)
```

which reads like a missing file rather than an unsupported spelling. This
moves the report to `check`, where it can be acted on before any work
starts, and says what is actually wrong.

In practice the pattern has also been a reliable sign that the data is
absent: in every bundle where we have seen it, the generator wrote it
precisely because it had no trajectories to list, and the archive
contained none.

### Unchanged

- Every other field and check behaves as before.
- `check`'s exit codes (`0`/`1`/`2`, see the 0.3.17 notes below) are
  unchanged; a wildcard is reported as an ordinary validation error.
- A filename with no wildcard is unaffected, including names that contain
  digits, dots or dashes.

## 0.3.22

### New: `collections`

Metadata can now declare one or more named collections a simulation
belongs to, via a new optional TOML key:

```toml
collections = ["ATLAS"]
```

- Optional -- omitting it changes nothing about existing metadata.
- Each entry must be a non-empty, non-whitespace string; `check`, `gen`,
  and `eg` validate and print it the same way they already do for
  `uniprot_ids`.
- `mdr-meta eg`'s full example output now includes a `collections` line.

### Unchanged

- Every other field, and `gen`'s directory-classification behavior, is
  untouched.
- `check`'s exit codes (`0`/`1`/`2`, see the 0.3.17 notes below) are
  unchanged.

## 0.3.21

### Fixed

`gen` no longer lists its own `--outfile` as one of the directory's
`additional_files` when `--outfile` points inside the directory being
scanned -- exactly what this page's own worked example does:

```sh
mdr-meta gen -d MDR00016593 -s GROMACS -o MDR00016593/mdrepo-metadata.toml
```

Previously, the freshly-created (empty) output file was picked up by the
directory scan and listed as an additional file of its own metadata.

## 0.3.17

### Breaking change

`mdr-meta check` used to exit `0` whether or not your metadata was valid.
It reported problems only by printing them, so any script that tested the
exit status treated a broken TOML as a good one.

`check` now follows the same convention as `grep`:

| Code | Meaning |
|------|---------|
| `0`  | Metadata is valid |
| `1`  | Metadata is invalid; the findings are printed to stdout |
| `2`  | The check could not run (usage error) |

Checking several files at once exits `1` if **any** of them is invalid.
A file that cannot be parsed or read counts as invalid, and the remaining
files are still checked.

### What you may need to change

- Anything of the form `mdr-meta check meta.toml && upload` was always
  proceeding to the upload. It now stops when the metadata is bad, which
  is what it looked like it did all along.
- A validation step in CI that has been quietly passing may now go red.
  That is the point of the change, but expect it on submissions whose
  metadata was already invalid.
- If you need one script to work against both old and new builds, do not
  branch on the exit code alone: treat **stdout as the verdict when the
  code is `0` or `1`**, and only treat other codes as "the check never
  ran". An older binary returns `0` with the findings on stdout, so
  code-only logic will pass invalid metadata.

### Also fixed

- `mdr-meta check` with no filenames exited `0` -- a vacuous pass on
  nothing at all. It is now a usage error.
- A file that passed printed a stray blank line. Success is now silent,
  so empty output means valid.

### Other subcommands

Errors raised by `eg`, `gen`, `to-json`, `to-toml` and `upgrade` now exit
`2` instead of `1` -- for example `eg --outfile` pointed at a file that
already exists. Their success behaviour and output are unchanged.

### Unchanged

- The wording and format of the findings `check` prints.
- `--no-id` still suppresses the "Missing PDB and Uniprot IDs" note.
- Ligand SMILES errors are still reported and are not suppressible.
