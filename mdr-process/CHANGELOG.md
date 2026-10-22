# Changelog

Notable changes to `mdr-process`, newest first. `mdr-process` is never
released on GitHub and has no tags: a host gets it by building `main`
(`just mdr-process`), and `mdr-process --version` names the build, e.g.
`mdr-process 0.2.0 (048d03d)`. See RELEASE.md, "Versioning `mdr-process`".

Bump the version in `mdr-process/Cargo.toml` in the same PR as the change:

- **minor** (0.2.0 -> 0.3.0) for any change in what it writes to the
  database or iRODS, or in its command line;
- **patch** (0.2.0 -> 0.2.1) for everything else.

## 0.18.0 (unreleased)

- **A fused chain gets a UniProt reference per part** (Ken, 2026-10-21).
  CXCR4 with T4 lysozyme in a loop (3odu) is three md_polymer_uniprot rows,
  CXCR4, T4L, CXCR4; a BRIL fusion two. Parts come only from SIFTS: every
  accession `pdb_chain_uniprot.tsv` lists for any chain of the declared
  entry, compared with the chain by BLAST as before. A row is a part at 70%
  identity over 20 or more residues; rows are taken by bit score, one that
  overlaps a part of another entry already kept by more than half the
  shorter of the two (`JUNCTION_SHARE`, Ken 2026-10-22: real junctions
  overlap by up to ~30 residues, so no fixed count works) is dropped (an
  ortholog, an isoform), one that overlaps by that or less is trimmed, the
  shared residues going to the higher score; for two rows of the same entry
  the limit stays 5 residues (`JUNCTION_OVERLAP`). Two rows of one entry
  with no part between them, in order in our chain and the entry and fewer
  than 20 residues between in each, are one row (BLAST broke an alignment
  at a gap); 20 or more of the entry's between (a deleted domain, iGluR
  S1S2; Ken 2026-10-22) keep them two, as do a tethered dimer's two copies.
  The 50%
  coverage floor applies to the parts together, so a fusion that no single
  protein covered half of, `none` before, is found. When the parts cover
  too little, the single best candidate is taken as in 0.16.0. Declared
  UniProt IDs and the search still give one reference.
  `md_polymer.match_method` is the first part's. The import writes all the
  parts in `ops::set_polymer_reference`'s one transaction, which now takes
  a list of hits (empty for `none`); `PolymerLookup.hit` is `hits` (and
  `import.json`'s key with it), and the reference-disagreement warning
  compares every part's entry.
- **BLAST's tabular output adds `qseq sseq`**, walked to trim a part's
  ranges exactly and count its identity again over what is left. A search
  result file from an older work dir (15 columns) still reads, without the
  alignment; any other column count is an error naming the file.
- **New `backfill-polymer-uniprot`** rewrites existing polymers' references
  by the rule above: input from
  `mdr-process/sql/backfill-polymer-uniprot-candidates.sql`; `--dry-run`,
  `--out` (one row per polymer: current and proposed rows, coverage before
  and after, the accessions not yet in md_uniprot, and flags such as
  `none_to_found`, `t4l_only_gains_parts`, `first_accession_changes`,
  `coverage_shrinks`, `unchanged`, `skipped_not_pdb_or_none`,
  `several_pdb_ids`), `--overlaps-out` (every pair of overlapping SIFTS
  rows, its overlap, the share of the shorter row that is, and whether it
  was trimmed or dropped), `-w` (the UniProt cache the real run reuses). Both
  runs read the database; the real run writes only polymers that are `pdb`
  or `none`, unchanged since read and with different rows, through the new
  `ops::replace_polymer_uniprots` (delete, insert and set `match_method` in
  one transaction: the one override of write-once), upserting their UniProt
  entries first.

## 0.17.0 (unreleased)

**Needs no migration.** `md_simulation.unique_file_hash_string` becomes a
different string for the same files; a simulation imported before is still
recognised by its old one.

- **The file hash string is a digest** (Nick, 2026-10-21).
  `unique_file_hash_string` was the sorted per-file MD5s joined with commas,
  33 bytes a file, and the column's unique index refuses a row over 2704
  bytes, so a simulation of more than about 80 trajectory files could not
  be imported: zenodo:14013285's Folding@home RUNs hold 85-100 each, and
  every push rolled back. It is now the SHA-256 hex digest of that joined
  string, 64 characters whatever the count. import.json carries the joined
  form as `legacy_unique_file_hash_string`; the import matches by the
  digest first and the joined form second, so a simulation imported before
  still deduplicates, and a reprocess rewrites its hash to the digest.

## 0.16.0 (unreleased)

**Needs md-repo-app migrations 0293-0295, and the other way round:**
0.15.x and earlier fail against a database with 0294 or 0295 applied, as
they write md_polymer's dropped hit columns (0294) and
md_simulation.fasta_sequence (0295). Deploy the two together.

- **A polymer's UniProt hit is an `md_polymer_uniprot` row** (Travis and
  Ken, 2026-10-21). md-repo-app 0293 creates the table, one row per part
  of a polymer, so a fused receptor + T4 lysozyme can name both proteins
  later, and copies each polymer's hit into it; 0294 drops md_polymer's
  `uniprot_id`, `query_start/end`, `reference_start/end` and `identity`.
  `md_polymer.match_method` stays, as the outcome of the search for the
  whole polymer. `ops::set_polymer_reference` keeps its signature and
  write-once rule: in one transaction it claims a polymer still untried
  and inserts the hit's row, so a hit the table refuses (past the
  polymer's end, now a trigger) leaves the polymer untried instead of
  claimed with no row. Still one row per polymer: nothing here splits a
  fused chain into parts yet. `ops::list_polymer_uniprots` reads them;
  the import's reference-disagreement warning compares with the first.
- **md_simulation.fasta_sequence is no longer written** (Ken, 2026-10-14
  and 10-21). md-repo-app 0295 drops it: it was a second copy of the
  chains' sequences, and the two drifted. The API builds a simulation's
  `fasta_sequence` from its md_chain rows, by `sequence::fasta_from_chains`'s
  rule. The import no longer sets it, `ops::set_fasta_sequence` is gone, and
  `backfill-chains --replace-chains` compares the chains alone, so a
  simulation whose chains match and whose stored FASTA did not is now left
  as it is. `import.json` loses its `fasta_sequence` key (nothing read it;
  `push_sim_files.py` reads the files). The processed `sequence.fa`, which
  BLAST reads, is written as before.

## 0.15.1 (unreleased)

- **TLS dependency update** (2026-10-21). No change in behavior. Cargo.lock
  moves rustls 0.23.45 -> 0.23.46, rustls-webpki 0.103.15 -> 0.103.16 and
  webpki-roots/webpki-root-certs 1.0.9 -> 1.0.10 for RUSTSEC-2026-0381/0382/
  0383 (certificate paths through distrusted emSign intermediates, whose
  private keys are public) and RUSTSEC-2026-0346..0349, all in the TLS
  under its HTTPS requests (reqwest). The member's own
  `mdr-process/Cargo.lock`, stale since the 03-18 move into the workspace
  and never read by cargo, is removed.

## 0.14.0 (unreleased)

- **Fix: a SIFTS accession's coverage is never less than its longest
  BLAST row** (2026-10-15). 0.12.0 added up an accession's rows taken by
  bit score, skipping a row out of order with those taken. In a tethered
  HIV protease dimer (3nxn, 3fsm, 4eqj, 4eq0) both copies align to the
  same residues of P03369; the higher-scoring row was the shorter (99 of
  202) and the longer (115) was skipped as out of order, so P03369 fell
  under half the chain: 3nxn and 3fsm lost their reference, 4eqj and
  4eq0 took Q9YV20 at 79% instead. The full A1 dry run found these 4 and
  no others; the 37 chains 0.12.0 gained are unchanged.

## 0.13.0 (unreleased)

- **A warning when the declared PDB ID matches no chain** (Ken,
  2026-10-15). 21 PLAS-20K simulations declare the PDB entry of another
  protein (84136: 4tq8, transthyretin, on an epoxide hydrolase 2 chain),
  and nothing said so: the SIFTS candidates failed and the chain went to
  the search. Now step 2 also notes which polymers a SIFTS accession of
  their declared entry aligns to at 70% identity or more over 20 or more
  residues, at any coverage (`related_to_pdb`), and when SIFTS maps the
  entry, every protein polymer was looked up, and none is related, the
  import warns (`declared_pdb_not_matched`, from `lookup_chains`). Looser
  than the SIFTS rule on purpose, so a Fab whose entry SIFTS maps only
  over its constant half, or a chain whose declared UniProt ID outscores
  the SIFTS accession, is not flagged. The backfill judges each
  simulation by its own declared IDs, not by a reference another
  simulation decided, logs the warning, counts it, and adds a column
  `pdb_id_check` (`no match`) to its report. A warning, not an error: the
  structure is what was simulated. It does not see a system with one
  right protein and one foreign (2wva, 5hi4, 5km5, 5v03). On 3,023
  simulations (the 21 and 3,002 random) it flagged 24: 17 of the 21, and
  7 whose PDB ID is right but only a part SIFTS does not map was
  simulated (a Fab, a peptide; about 0.2%), which the warning says.
  `declared_matches` and `lookup_polymers` return the related set too.

## 0.12.0 (unreleased)

- **A SIFTS accession split by an insert is taken** (Ken, 2026-10-15). A
  SIFTS accession must cover half the chain, and it was measured on one
  BLAST row. A protein with another inserted in a loop, such as CXCR4
  with T4 lysozyme in 3odu (sim 10488), comes back as one row each side
  of the insert, each under half, so the chain got `none`. Its rows at
  70% identity or more now count together (`sifts_coverage`), when they
  run in the same order in the chain and the entry, each residue once.
  The stored range and identity are still those of the row with the top
  bit score. Declared UniProt IDs and the search are unchanged.

## 0.11.0 (unreleased)

- **`backfill-chains --replace-chains`** (Ken, 2026-10-14). The DDD
  peptides' chains were written on 10-02 from files that gave every chain
  the same ID; the files were relabeled on 10-08 but the chains were not,
  so all are labelled `A`, `fasta_sequence` has `>A`, `>A_2`, ..., and four
  sims (105114, 105117, 105131, 105135) have two chains stored as one. With
  the flag, a simulation that already has chains and whose stored structure
  chains (label, residues, numbering, caps) or `fasta_sequence` differ from
  its structure file's has its structure chains and `fasta_sequence`
  replaced in one transaction, as an import of that file writes them
  (`import::replace_structure_chains`). Declared chains keep their rows,
  so polymer ligands stay linked, and move to follow the new structure
  chains; a declared chain between structure chains is refused. New
  polymers are looked up; existing references are kept. Report status
  `chains replaced`. mdr-db: `delete_structure_chains_for_simulation`,
  `set_chain_order`, `set_fasta_sequence`.

## 0.10.0 (unreleased)

- **`backfill-chains` fills untried polymers of a simulation that already
  has chains** (Ken, 2026-10-14). It used to leave such a simulation alone,
  so the DDD peptides (MDR00105041-105140, imported on 0.4.0 with chains and
  no references) kept 192 polymers never looked up. Now their chains stay
  as they are, and only polymers with no `match_method` are written: those
  in the structure from the lookup, matched by residues (a declared chain
  shifts the chain order), and short or non-protein ones the structure
  lacks (a declared peptide) `none`, by the length rule an import applies.
  A polymer that already has a reference keeps it, as before.
  `import::backfill_chains` returns `Backfilled` instead of a bool; the
  report's status is `has chains; <n> references set`.

## 0.9.0 (unreleased)

- **SIFTS accessions skip the pass rule** (Ken, 2026-10-13, after the DDD
  dry run). In step 2, an entry SIFTS lists for the declared PDB ID is
  PDBe's assignment and is taken whenever `blastp -subject` aligns it over
  at least half the chain (`MIN_SIFTS_COVERAGE`, a guard against a wrong
  declared PDB ID; Ken: "50% until things go sideways"), with no identity
  or mismatch limit; BLAST still picks among the entry's accessions (top bit score, then
  the tie-break) and gives the ranges and identity. 0.8.0 held them to
  95/3/90 against the UniProt sequence, which 0.6.0 never did (it checked
  our chain against the PDB entry's own sequence, then took SIFTS's
  accession), and so lost 1,820 of DDD's 26,495 `pdb` chains: mutants and
  engineered constructs (3gcp, 4 mismatches, 98.9%) and fusions. A fusion
  now takes the partner with the top bit score, as 0.6.0 took SIFTS's
  largest segment. Declared UniProt IDs and the search keep 95/3/90.
- **A SIFTS accession must be at least 70% identical** (`MIN_SIFTS_IDENTITY`;
  the DDD report's question 2, taken as suggested, Ken 2026-10-14). SIFTS
  lists entries per PDB entry, not per chain, so without it 25 DDD chains
  in 20 entries, mostly antibodies, took another chain's entry at 23-77%
  (1etz's heavy chain B took the light chain's G0YP42 at 26.7%).

## 0.8.0 (unreleased)

- **BLAST for every comparison** (Ken, 2026-10-13; plan
  `PLAN-2026-10-13-polymer-reference-blast-every-comparison.md`). Curated
  sources pick the candidate entries: the UniProt entries SIFTS lists for
  the declared PDB ID (`pdb`) and the declared UniProt IDs (`declared`, new;
  md-repo-app 0292 allows it). Each is compared with the chain by
  `blastp -subject` (no database) under the search's pass rule. Failing
  those, the Swiss-Prot/TrEMBL search, as before (`aligned`). 0.6.0's
  exact-sequence PDB matcher (`place()`, SIFTS coordinate mapping,
  `pdb_seqres.txt`) is gone; the ranges and identity are BLAST's for every
  method.
- **Identity is over our aligned residues** (`nident` / our residues in the
  alignment), not BLAST's `pident`: residues our chain lacks (an unresolved
  loop) no longer count against it. The pass rule is otherwise unchanged:
  >= 95%, <= 3 mismatches, >= 90% of the chain covered, in both steps. So
  a mutant with up to 3 substitutions now matches its declared entry (0.6.0
  allowed none), and a fusion matches neither partner (D3, deferred to the
  DDD dry run).
- **Ties go to the UniRef100 representative** when it is one of the tied
  entries, then canonical over isoform, then the lowest accession. SOL3's
  identical strain entries now give S288c's P38858, not YJM789's A6ZT71
  (MDR00105204). One UniProt call per tie, cached with the entries by the
  backfill; a failed call falls back to the old order.
- A declared UniProt ID whose entry cannot be fetched is now an upload
  warning (it was only logged, and the ID was silently not linked).
- The backfill's input takes an optional fourth column, the declared
  UniProt IDs (comma-separated), and its `disagreeing_sims` compares each
  sharing simulation's own declared candidates.
- **`mdr-process backfill-chains`**: md_chain, md_polymer and each polymer's
  UniProt reference for simulations imported before mdr-process wrote
  them. It reads a TSV of simulation ID, declared PDB ID and a local copy
  of the processed structure; splits each with the import's splitter;
  looks every polymer up in one batch (SIFTS read once, one search,
  each UniProt entry fetched once); and writes each simulation in one
  transaction. A simulation that already has chains is left alone; a
  polymer shared by several simulations is decided by the lowest
  simulation ID. `--dry-run` touches no database. The report has one row
  per chain, with how many sharing simulations would have disagreed.
- `reference::lookup_polymers` is that batch; the import's per-simulation
  lookup now goes through it too. UniProt entries are fetched 8 at a time
  with two retries.
- The backfill caches UniProt entries in `<work_dir>/uniprot/` (fetch
  time and UniProt's JSON; and each UniRef100 answer), so the real run over its dry run's work folder
  reuses them, and its BLAST files, and writes what the dry run reported.
  Imports do not cache.

## 0.7.0 (2026-10-12)

- **A blank chain ID is stored as an empty `md_chain.chain_label`, not a
  space.** `sequence.rs` read PDB column 22 untrimmed, so every chain in
  a file with no chain IDs got `' '` (72 prod chains, MDR00105141-105212,
  and 1 on staging; those rows were set to `''` by hand the same day).
  FASTA headers were already right (`_`, `_2`): `header()` trims. The
  UI derives the same `_`/`_2`/`A_2` name from `chain_label` and
  `chain_order` (Ken, 2026-10-12); the column stays the file's label.
- Minor, not patch: it changes what is written to the database. The
  unmerged `mdr-backfill-chains` branch also claimed 0.7.0; it becomes
  0.8.0 when rebased onto this.

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
