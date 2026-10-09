# Changelog

Notable changes to `mdr-process`, newest first. `mdr-process` is never
released on GitHub and has no tags: a host gets it by building `main`
(`just mdr-process`), and `mdr-process --version` names the build, e.g.
`mdr-process 0.2.0 (048d03d)`. See RELEASE.md, "Versioning `mdr-process`".

Bump the version in `mdr-process/Cargo.toml` in the same PR as the change:

- **minor** (0.2.0 -> 0.3.0) for any change in what it writes to the
  database or iRODS, or in its command line;
- **patch** (0.2.0 -> 0.2.1) for everything else.

## 0.7.0 (2026-10-09)

- **`mdr-process backfill-chains`**: md_chain, md_polymer and each polymer's
  UniProt reference for simulations imported before mdr-process wrote
  them. It reads a TSV of simulation ID, declared PDB ID and a local copy
  of the processed structure; splits each with the import's splitter;
  looks every polymer up in one batch (the PDB index read once, one BLAST,
  each UniProt entry fetched once); and writes each simulation in one
  transaction. A simulation that already has chains is left alone; a
  polymer shared by several simulations is decided by the lowest
  simulation ID. `--dry-run` touches no database. The report has one row
  per chain, with how many sharing simulations would have disagreed.
- `reference::lookup_polymers` is that batch; the import's per-simulation
  lookup now goes through it too. UniProt entries are fetched 8 at a time
  with two retries.
- The backfill caches UniProt entries in `<work_dir>/uniprot/` (fetch
  time and UniProt's JSON), so the real run over its dry run's work folder
  reuses them, and its BLAST files, and writes what the dry run reported.
  Imports do not cache.

## 0.6.0 (2026-10-09)

- **The PDB step reads the simulation's declared PDB ID** (Ken,
  2026-10-09), replacing 0.5.0's search of the whole PDB and its "every
  entry must agree" rule. The declared entry's protein chains are compared
  with ours: every residue of ours must equal one of theirs, in order,
  with theirs allowed extra residues at either end, and inside only where
  our backbone is broken (a missing loop). No mismatches. SIFTS's segment
  covering most of our residues gives the UniProt entry (`pdb`). No PDB
  ID, or no match: BLAST, as before.
- **The reference is still md_polymer's and still set once**, so when
  simulations with the same residues declare entries that map to
  different UniProt entries, the first to import decides. A later one that
  disagrees now gets an upload warning; the stored reference is not
  changed. This relaxes 0.5.0's "decided from the residues alone".
- `sequence.rs` records where a chain's backbone is broken
  (`breaks`), which the match uses and `import.json` carries.

## 0.5.1 (2026-10-09)

- **Dependencies updated within their existing version ranges**
  (`cargo update`; `Cargo.lock` only). Among them diesel 2.3.14, clap
  4.6.7, reqwest 0.13.5, tokio 1.53.2, rustls 0.23.45, uuid 1.27.0. No
  change in behaviour expected. The major-version bumps from dependabot's
  mdrepo-rs#23 are left for one PR each.

## 0.5.0 (2026-10-09)

- **Each polymer gets a UniProt reference, decided from its residues
  alone** (md-repo-app 0287-0290; Travis's option 1 and Ken's
  "an exact match in the PDB first, then BLAST", 2026-10-09). Per distinct
  polymer, in `reference.rs`:
  - DNA, RNA, or under 20 residues: `none`.
  - `pdb`: a PDB chain with exactly these residues (parent letters), from
    a local `pdb_seqres.txt`; SIFTS's UniProt segment for it, a fusion's
    largest. Every matching chain SIFTS maps must name the same accession,
    or BLAST decides (the proposed tie rule, to be confirmed by Travis).
  - `aligned`: BLAST, Swiss-Prot then TrEMBL; identity >= 95%, at most 3
    mismatches, >= 90% of the chain covered; top bit score, then canonical
    over isoform, then the lowest accession.
  - Otherwise `none`.
  The import writes it to md_polymer only if the polymer was never looked
  up, so a reference is set once. The hit's UniProt entry is stored but not
  linked to the simulation: md_simulation_uniprot still holds the TOML's
  accessions, as before, and a declared accession that is no chain's
  reference is now an upload warning.
- **Needs `just pdb-index` on each processing host** (into
  `$MDREPO_WORK_DIR/blast/pdb`). Without it, polymers are left unlooked-up
  and the upload gets a warning; nothing fails.
- **md_uniprot keeps UniProt's JSON** (`response`, `entry_version`,
  `fetched_at`), and **md_pdb keeps PDBe's molecules and SIFTS mappings**
  for the declared PDB ID (`response`, `entities_response`, `fetched_at`).
- Needs md-repo-app 0290 in the database.

## 0.4.0 (2026-10-01)

- **A .psf topology's elements come from its masses and bonds**, not
  cpptraj's guess. cpptraj gives each atom the element nearest its mass, so
  a hydrogen repartitioned to 3.024 amu was written as helium, and ions lost
  their charges. Columns 77-80 of `full.pdb` and `minimal.pdb` are rewritten
  before anything reads them; nothing else in those files changes. Ligands
  inferred from them, and the files in iRODS, change for affected
  simulations. A hydrogen is told from a heavy atom by its bonds as well as
  its mass, so a methyl carbon at 4x repartitioning (2.939 amu) stays carbon.
  A `.psf` that cannot be read, or an atom whose mass matches no element or
  more than one, leaves the files as cpptraj wrote them and is never fatal;
  the warning goes to the upload's messages, where the submitter sees it.

## 0.3.0 (2026-10-01)

- **What `mol_id.py` says for the submitter becomes an upload warning.**
  It prints each note as an `[mdrepo] note=` line on stderr: a residue
  passed over as part of a polymer chain, a group covalently bound to one
  and not recorded, a ligand recorded as released from one (needs
  simulation-processing with MD-Repo/simulation-processing#15). The notes
  are kept in `processed/ligand_notes.json`, so a rerun reports them
  again. An older `mol_id.py` writes none, and nothing changes.
- **A declared ligand with nothing inferred to check it against** is now
  a warning, "Unable to verify ligand [n] (...): no ligand was found in
  the structure". It used to pass in silence.

## 0.2.0 (2026-10-01)

The first numbered version; every build before this said 0.1.0.

- **Chains for `md_chain` and `md_polymer`** (MDR-94, mdrepo-rs#22). A
  residue is in a chain when its backbone is bonded to the one before it,
  whatever its name or record type, so HETATM and unknown residues stay in
  place. DNA and RNA chains are found; caps are recorded; 4-letter residue
  names are read whole. The FASTA comes from the same chains (protein only,
  for BLAST) and is unchanged for standard proteins.
- **Ligands declared by sequence** are matched to a chain, or by heavy-atom
  composition to a molecule written as one residue (DDD's `LIG`), and stored
  as an `md_ligand` row pointing at that chain (md-repo-app 0286).
- **A DNA- or RNA-only structure** is processed: no BLAST, and
  `fasta_sequence` is null.
- **`mdr-process sequence <pdb>...`** prints a structure's chains, read only.
- **`--version` and the first log line** name the commit this was built
  from.
