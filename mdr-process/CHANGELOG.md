# Changelog

Notable changes to `mdr-process`, newest first. `mdr-process` is never
released on GitHub and has no tags: a host gets it by building `main`
(`just mdr-process`), and `mdr-process --version` names the build, e.g.
`mdr-process 0.2.0 (048d03d)`. See RELEASE.md, "Versioning `mdr-process`".

Bump the version in `mdr-process/Cargo.toml` in the same PR as the change:

- **minor** (0.2.0 -> 0.3.0) for any change in what it writes to the
  database or iRODS, or in its command line;
- **patch** (0.2.0 -> 0.2.1) for everything else.

## 0.39.0 (unreleased)

- **A fusion's junction is cut where the two alignments match best**, rather
  than given to the row with the higher bit score over its whole length.
  BLAST's local alignments run past a junction, so both of a fusion's rows
  claim the residues around it, and `sifts_parts` gave every shared residue
  to the higher-scoring row. In a 4eiy-like A2A-BRIL-A2A construct the
  second receptor row (206 bits) outscores BRIL's (204) while matching the
  17 residues at the junction 5 times against BRIL's 16: those 17 were
  recorded as receptor, BRIL's part lost 22 residues, and the receptor's
  part claimed 203-218 of the entry, a stretch the construct deletes. The
  cut now taken maximises the identical residues of the left part up to it
  plus those of the right part after it, counted by walking the two
  alignments (`junction_cut`), so a part kept earlier can be cut back by a
  row taken after it. Ties, as when both rows match the shared residues,
  and junctions no cut resolves still go to the row ranked first. The
  2026-10-21 dry run reported receptor-BRIL junction overlaps of 15-23
  residues, so this moves real reference ranges.
- **Two rows of one entry are the same residues claimed again by where they
  put those residues in the entry**, not by how many of ours they share
  (`same_entry_residues`). The old rule dropped a second row of an entry
  that overlapped the first by more than `JUNCTION_OVERLAP` (5) of our
  residues. Real BLAST rows for a tethered dimer (3nxn, two copies of the
  HIV protease in one chain) share 6 at the junction, the first row putting
  them on the entry's 608-613 and the second on its 491-496, so the second
  copy was dropped: the polymer got one part over 54% of the chain, claiming
  residues of the entry the chain does not have. Rows that do put the shared
  residues on the same residues of the entry are still a repeat, and still
  dropped. `JUNCTION_OVERLAP` now only bounds what two rows joined into one
  part may share of the entry.
- Both are H1 and H2 of the 2026-11 review of the reference track, each
  confirmed there against real `blastp` output.
- **References already in md_polymer_uniprot do not change by themselves.**
  The import writes a polymer's reference once, so run
  `backfill-polymer-uniprot --dry-run` against this build and check
  `--overlaps-out` before the real run: a fused chain's parts move, and a
  chain whose repeat was dropped gains one.

## 0.37.0 (unreleased)

- **`mdr-process zero-xtc-box IN [-o|--output FILE]`**: set every frame's box to zero
  in an XTC, or in each `.xtc` inside a tar (`full.tar`, `minimal.tar`),
  for a simulation with no box whose files carry junk boxes. cpptraj wrote
  an uninitialized box for box-less input (a DCD with the unit-cell flag
  off) until simulation-processing #44: inf, -inf, 1e10 nm, or tiny
  leftovers like -4.3e-29, in every frame of some replicates and none of
  others. The coordinates are fine, so only the 36 box bytes of each frame
  change; file and tar sizes, tar headers and every other byte stay the
  same.
- Without `--output` it only reports (per XTC: frames, boxes to zero, box
  kinds; `--json`). With it, IN is never written: the copy is written as
  `FILE.partial`, read back (same frames, every box zero, no byte changed
  outside a box field) and only then renamed. A file with a real-looking
  cell in any frame is refused, since that simulation has a box. An
  existing output file, a damaged frame or tar header, or a sparse tar member is
  an error.
- Tar members found by ustar name + prefix, GNU long names and pax `path`
  / `size`.
- Real data: 21553 (contributor 250) and 105141 (contributor 355): before,
  6 of 10 and 30 of 50 replicates fail `check-outputs`; after, none; molly
  reads identical coordinates. 0.4-7 s per tar.

## 0.36.1 (unreleased)

- `process --help`: `--skip-output-checks` no longer says it skips
  simulation-processing's `check_outputs.py`; the checks are mdr-process's
  own (`check-outputs`) since 0.35.0, and the script is removed
  (simulation-processing #49).

## 0.36.0 (unreleased)

- **`check-outputs` takes exactly one folder.** It used to take several
  and treat them as one simulation's file set (the first file of a name
  won), so `check-outputs rep_*` checked only rep_1's files and passed a
  run whose rep_2 had an infinite box in all 20,000 frames (105222 before
  simulation-processing #44). A second folder is now a usage error; check
  each replicate with its own call. `process` already did.
- **A missing or empty folder no longer passes.** A path that is not a
  folder is an error; a folder holding none of full/minimal
  `.xtc`/`.pdb`/`.psf`/`.mmcif` or `sampled.xtc` (an empty folder, or one
  holding only `rep_N` subfolders) is a `files` failure. Both used
  to report "0 fail, 0 warn" and exit 0. Applies to `process`'s output
  checks too.

## 0.35.0 (unreleased)

- **Output checks before the import and before the push.** `process` checks
  every replicate's folder (each against its own source trajectory) after
  the import JSON is made and before anything is written to the database
  or IRODS, and the longest replicate's folder again once the mmCIF pair
  exists, before the push. A failure stops the run and lists every finding;
  nothing is imported, or, for the mmCIF check, the simulation stays a
  placeholder with nothing pushed, as after an incomplete push. Warnings
  join the run's warnings as "Output check: ...". Dry runs check too. One
  log line per run: `Output checks: N folder(s), F failure(s), W warning(s)`.
- The checks (new module `output_checks`): the **box** (finite; none or a
  real cell in every frame; `full.pdb` CRYST1 agrees with frame 0), **time**
  (never backwards; full and minimal agree; an XTC source's stored times
  match), **atoms** (same atoms, names and residues as the `.psf`;
  `.pdb`/`.mmcif` coordinates = the first frame). `.gro` is not checked.
  Why: box-less runs shipped XTCs with an uninitialized box for years
  (simulation-processing #44) and no check read inside a trajectory.
- `mdr-process check-outputs DIR... [--box|--no-box] [--source-traj FILE]
  [--json OUT] [-q]`: the same checks on any folder(s), exit 1 on failure.
  (One folder only from 0.36.0.)
- `--skip-output-checks` on `process`: for a simulation whose findings were
  looked at and accepted. Recorded as a warning. The ticket and reprocess
  flows always check.
- New dependency: the `molly` crate (XTC reading; mdr-process already ran the
  `molly` command for frame times).

## 0.34.0 (unreleased)

- **No more `simproc` conda env.** The trajectory conversion script
  (`cpptraj_gmx_traj_manipulation.py`) used to run under the hand-built
  `simproc` env (Python 3.7, AmberTools 22, pytraj, ParmEd 3.4.3), found
  through `micromamba env list`, with its `bin/` on PATH for cpptraj and
  AMBERHOME set. It now runs under SCRIPT_DIR's uv environment (ParmEd
  4.3.1), the same one as every other script, and cpptraj is a standalone
  build from https://github.com/Amber-MD/cpptraj: `MDREPO_CPPTRAJ` if set,
  else `cpptraj` on PATH.
- Both are checked once before any trajectory (`cpptraj --version` runs;
  the uv environment imports ParmEd), so a host that is not ready fails at
  once with the fix in the message.
- **Before installing on a host:** pull simulation-processing (the script
  without pytraj, and ParmEd in its `pyproject.toml`) and `uv sync`; build
  cpptraj and put it on PATH or set `MDREPO_CPPTRAJ`. micromamba is no
  longer needed by mdr-process.

## 0.33.0 (unreleased)

- **`validate --agent-imported`** checks a directory by the agent rules
  `process --agent-imported` and `meta-check --agent-imported` already use
  (0.31.0): a format set that does not fit the engine is a warning when
  processing can read the files, and the ORCID, temperature and software may
  be left out. mdrepo-agent asked for it (mdrepo-agent#105) so its pre-push
  gate no longer has to drop validate's errors that meta-check does not
  raise.

## 0.30.0 (unreleased)

- **A DNA or RNA ligand written as one residue is matched by composition**,
  as peptides are (DDD's `LIG`; the TOML's `sequence_type = "dna"|"rna"`).
  Until now `place_chains` refused it as "not supported yet". The
  candidates (`sequence::expected_nucleic_formulas`): each nucleotide's CCD
  heavy atoms (standard DA..DT, A, C, G, U; others from the residue-formula
  table), minus one O per phosphodiester bond; then the same without the 5'
  phosphate (P1 O3 fewer), which PDB entries often leave off. Checked on
  DDD group B's six: 3bbb, 4nku, 6kdi without it, 4i67, 5o58, 6p7q with it,
  as RCSB's own names for them say (`5'-D(*AP*G)` vs `5'-R(P*AP*G)`). When
  nothing matches, the message lists both formulas.

## 0.29.0 (unreleased)

- **Ligands declared by sequence can use non-standard residues** (Travis
  2026-10-02, decision 4(b)): the composition check takes each residue's
  heavy atoms from a table built from the PDB's Chemical Component Dictionary
  (`src/residue_formulas.rs`, 216 codes: TPO, SEP, PTR, ASJ, M3L, ...)
  when it is not one of the 22 standard amino acids. A code outside the table
  is still refused, now naming the file to add it to. Generated by
  simulation-processing `python/residue_formulas.py` from
  `data/residue-codes.txt`: the codes in the DDD peptide bundles (group B,
  2026-11-04) and the 10-02 batch.

## 0.28.0 (unreleased)

- **The .pdb pair's file types are now "Processed PDB structure" and
  "Minimal PDB structure"** (were "Processed structure" / "Minimal
  structure"), like the GRO and mmCIF types. Needs md-repo-app migration
  0301, which renames the types and retypes the existing rows: deploy the
  two together, since each side's names fail the other's foreign key.

## 0.27.0 (unreleased)

- **`standardize --server S --simulation-id N` also writes the mmCIF pair**
  (the MDR-108 backfill, MDR-110): the final pdbs, the simulation's title,
  UniProt references and ligands read from the database (read-only), each
  ligand's residue name from `mol_id.py smiles-from-structure` (no
  network), the links from the psf's bonds. report.json gains `mmcif`
  (entities, links, references, ligands unplaced).
- **A ligand is placed on a residue only by InChIKey**, exact or its first
  block (`mmcif_input_from_db`, imports too); otherwise it is written
  without its identity and named in a warning: a wrong SMILES in the file
  is worse than none. If mol_id.py finds no ligand at all (MDR00020024's
  minimal.pdb lacks it, MDR-109), the file is still written.

## 0.26.0 (unreleased)

- **The standard mmCIF pair, `full.mmcif` and `minimal.mmcif` (MDR-110,
  #75; plan PLAN-2026-11-03-mmcif-writer.md, Ken's answers 2026-11-03).**
  `mmcif_writer.rs`: the pdb's atoms, order and coordinates; entities (a
  polymer by type, caps and residues; a ligand by name; water; an ion by
  name); label_asym per chain/molecule and auth_asym = the pdb's chain ID
  (blank outside every chain: WAT/ION/HET, as the psf); sequences; the
  cell (not GROMACS's 1 A placeholder); UniProt references; ligand name,
  SMILES and InChIKey where mol_id.py named the residue; `_struct_conn`
  for links between residues other than a chain's backbone (from
  extract_bonds.py's bonds). Each file is read back against the pdb.
- **Written after the import**, which makes the accession the file names
  (Ken): `make_import_json` saves `mmcif-input.json` (the title, and each
  ligand's residue name from mol_id.py by InChIKey); after `run_import`,
  `add_import_mmcifs` reads the UniProt references and ligands from the
  database (`mmcif_input_from_db`: a polymer seen before is not looked up
  again, so only the database has every chain's references), writes the
  pair, adds them to import.json and then records them as "Processed/
  Minimal mmCIF structure" (md-repo-app 0300), so the push uploads and
  verifies them. Not written: a warning, as for the psf.
- `mdr-process write-mmcif` writes the pair from two pdbs without the
  database, for trying it.

## 0.25.0 (unreleased)

- **psf segments for atoms outside every chain: WAT, ION, HET** (Ken,
  2026-11-03). cpptraj writes a TER after every molecule, and a segment
  per TER block gave every water its own segment (MDR00087113: 131,294
  segments; since 0.22.0, new imports too). Now a blank-ID atom is in
  WAT (water), ION (one-atom residue) or HET (anything else: ligands,
  lipids), its residues numbered 1, 2, 3... in file order within each, so
  each stays distinct where the pdb repeats numbers (MDR00000778's waters
  are all residue 1). Chains keep their letter as the segment; a file with
  no chain ID at all keeps a segment per TER block for what is not water
  or an ion. A minimal.psf atom takes the segment and residue number of
  its full.pdb atom, so the pair agree. `psf_writer::blank_segments`.
- **`mdr-process standardize -i DIR -o DIR`** for the MDR-106 + psf
  backfill (PLAN-2026-11-03-chain-ids-psf-backfill.md): one existing
  simulation's full.pdb/minimal.pdb (chain-ID rule), psf pair, and a
  thumbnail where 2+ chains were relettered, by the same code as an import,
  into another directory with report.json; the inputs are not changed.
  `standard_psfs` now takes paths instead of the import's arguments.

## 0.24.0 (unreleased)

- **One rule for the chain IDs of full.pdb and minimal.pdb (MDR-106,
  #61), whatever wrote them** (GROMACS, cpptraj for Amber, cpptraj/parmed
  for NAMD/CHARMM; `sequence::assign_chain_ids`, `process::label_chains`).
  The chains are the splitter's (md_chain's). Their IDs are kept when each
  chain already has its own letter (A-Z or a-z, none repeated: DDD's and
  the PDB's own IDs). Otherwise every chain is relettered A, B, ... in file
  order (A-Z, a-z, 0-9): a blank column, mdCATH's "0", GPCRmd's "P" on
  every protein chain. When relettering, an atom outside every chain
  (ligand, ion, water, lipid) keeps its ID unless it is now a chain's
  letter, and is then blank (Ken, 2026-11-03). Only column 22 changes
  (ATOM/HETATM and TER), so the trajectories still match. Both files take
  the rule or neither, and their chains must read back the same. Left as
  written, with a warning for the submitter: more than 62 chains, more
  than one MODEL. It runs before the psf step and before md_chain is
  filled, so `md_chain.chain_label`, the psf segments and the files agree.
- **The standard .psf's segments:** where a file has chain letters, its
  blank atoms are segments S1, S2, ... by TER block, so solvent can no
  longer share segment "A" with chain A. A file blank throughout keeps
  A, B, ... by TER block.
- **`mdr-process label-chains PDB [MINIMAL] -o DIR`**: the same rule for
  one file, or a full/minimal pair, written to DIR; the inputs are not
  changed. For the MDR-106 backfill.

## 0.23.1 (unreleased)

- **sha2 0.10 -> 0.11, with the stored hashes unchanged.** sha2 0.11's
  digest no longer formats with `{:x}`; the three hashes the database keys
  on (`unique_file_hash` -> md_simulation.unique_file_hash_string,
  `import::residues_hash` -> md_polymer.residues_hash, `sha256_of_file`)
  now go through one helper, `process::sha256_hex`. A new test pins all
  three to `sha256sum`'s output, written and passing on sha2 0.10 before
  the bump; the old `unique_file_hash` test compared sha2 with itself.
  Split out of Dependabot's #62, which failed to build on this.

## 0.23.0 (unreleased)

- **A declared UniProt ID aligned over 80 or more of the chain's residues
  at 95% identity is confirmed, whatever the coverage**
  (`MIN_DECLARED_ALIGNED`, `confirms_declared`). The 50% floor turned down
  antibody constant domains just under half a Fab chain: in the prod
  backfill-declared-uniprot run (2026-11-02), 211 fails had >=95%
  identity under 50% coverage, ~165 of them over 80+ residues (P01834
  49, P01837 40, P0DOY2 14, P01857 13, ...). Applies to imports and to
  backfill-declared-uniprot, whose rerun adds the newly confirmed rows
  (Ken, 2026-11-02).

## 0.22.0 (unreleased)

- **The standard `.psf` is published, and the `.gro` files are no longer
  called topology.** `full.psf` and `minimal.psf` are registered as
  "Processed topology" and "Minimal topology"; `full.gro` and
  `minimal.gro` as "Processed GRO structure" and "Minimal GRO structure"
  (a .gro has coordinates and a box, no bonds; Travis, 2026-10-19). A
  replicate with no psf (coarse-grained, or the psf could not be
  written) has no topology file. **Needs md-repo-app migration 0298 on
  the server first**: md_processed_file.file_type is a foreign key, so
  without the two new types every import fails.

## 0.21.1 (unreleased)

- **The standard `.psf` is written for every replicate** (`full.psf` and
  `minimal.psf` beside the PDBs; Travis, 2026-10-22: pdb, xtc and psf are
  the standard files, and the psf is the graph only). New module
  `psf_writer`: atoms, elements, residues, segments (the chain ID, or one
  per TER-separated block where the column is blank) and bonds, in
  psfgen's PSF EXT layout; charges 0, standard masses, the element as the
  type; read back with `psf_elements::parse_psf` before it is kept. The
  bonds come from simulation-processing `extract_bonds.py`, which maps the
  upload's best topology onto `full.pdb` (.tpr, .psf, prmtop, a
  self-contained GROMACS .top) or guesses them from distances. Skipped for
  coarse-grained systems.
- **Not yet published.** The files are not registered as processed files,
  so nothing new reaches iRODS or the database: md-repo-app has no file
  type for them until the `.gro` files stop being "topology". A failure
  is a warning, never a failed import, and so is a missing
  `extract_bonds.py` (a host whose simulation-processing predates it).

## 0.21.0 (unreleased)

- **UniProt IDs are kept only on the chains, and a declared one only when
  it passes against a chain** (Travis, 2026-10-22, MDR-104; Ken,
  2026-10-30). The import no longer writes `md_simulation_uniprot`, which
  linked every declared ID to the simulation unchecked (or the top
  Swiss-Prot hit when none was declared); md-repo-app drops the table.
  A simulation's UniProt IDs are its chains' references
  (md_chain -> md_polymer_uniprot), which `mdr-export` now writes as
  `uniprot_ids`.
- **The old UniProt search is gone** (`get_uniprot_entries`,
  `blast_uniprot`): a Swiss-Prot BLAST on every import with a protein,
  plus isoform and TrEMBL searches when a declared ID was not hit, whose
  only result was that table and a few warnings. Declared IDs are
  compared with the chains pairwise (`blastp -subject`), as they already
  were for the chains' references, and the databases are searched only
  for a polymer no candidate passes.
- **Every declared UniProt ID that passes is recorded.** A polymer has one
  reference, so a second declared ID that also passed (two entries with
  one sequence) was dropped; it is now another `declared`
  md_polymer_uniprot row (`PolymerLookup::other_declared`). A declared ID
  that passes against a polymer an earlier simulation looked up is added
  to it the same way (`import::add_declared_references`), where before
  the polymer's first lookup was all that was kept. A polymer that was
  `none` becomes `declared`. Needs md-repo-app's migration replacing
  md_polymer_uniprot's unique (polymer_id, query_start) with
  (polymer_id, uniprot_id, query_start); before it, a declared entry at
  the start of an existing row is skipped, not an error.
- **A peptide too short for BLAST is found in its declared entries**
  (Ken, 2026-10-30). A protein chain of 5 to 19 residues was always
  `none`; now each declared UniProt entry that contains its sequence
  exactly is a `declared` reference at 100% identity
  (`reference::peptide_lookup`), with no BLAST and no SIFTS. On the
  laptop copy of prod this keeps 1,705 md_simulation_uniprot links, such
  as coactivator peptides bound to nuclear receptors, that would have
  failed.
- **A declared ID is confirmed more loosely than a reference is chosen**
  (`reference::confirms_declared`, Ken, 2026-10-30): besides the pass
  rule, over half the chain at 95% identity (an antibody's constant
  domain), or 90% of it at 90% identity whatever the mismatches (a
  drug-resistant HIV protease). A peptide of 8 or more residues may
  differ from its declared entry by one substitution (6qhl's p65 E49R).
  The reference is still chosen by the pass rule.
- `backfill-declared-uniprot` reports `rejected` for an entry that aligns
  to nothing when every protein chain has a reference of its own: a
  protein that was not simulated (BRIL, T4 lysozyme).
- A declared ID that passes against no chain is reported as not recorded
  (`declared_not_referenced`), now also when no chain has a reference.

## 0.20.0

- **A declared UniProt ID that UniProt will never serve no longer leaves
  the chains untried.** If a candidate's entry could not be fetched, the
  chain's UniProt lookup was skipped and left for a later run, which never
  helped when the ID was not an accession (`<uniprotid>`, `BIOTIN`, a
  truncated `A0A0R4`) or the entry had been deleted or demerged: 36
  simulations (53 polymers) in prod, 2026-10-28. `get_uniprot_entry` now
  reports such an ID as `UniprotGone` (UniProt answers 400 or 404, or the
  entry is inactive); it is not retried, it is left out of the lookup with
  a warning ("UniProt ID ... was left out of the chain's UniProt lookup"),
  and the chain goes on with its other candidates, or to the search. A
  fetch that fails for any other reason still leaves the chain untried.
- **A PDB ID that RCSB has removed is still linked.** RCSB's entry lookup
  (`core/entry`) answers 404 for a removed (obsolete) entry, so the import
  logged a warning and left `md_simulation.pdb_id` empty even though the
  metadata named the PDB ID. On a 404, `mdr-process` now asks RCSB's
  removed-entry lookup (`holdings/removed`) and, if the entry is listed,
  creates the `md_pdb` row with its title and the classification
  `OBSOLETE` (PDBe has nothing for a removed entry, so `response` and
  `entities_response` stay empty). The import still warns, e.g. "PDB 1JT1
  was removed by RCSB on 2025-01-15 (replaced by 5W90); linked from its
  removal record". Found on MDR00105470 (DDD bundle 1jt1).

## 0.19.0 (unreleased)

Fixes from the 2026-10 scientific-correctness review (`mdrepo_review_summary.md`).

- **GROMOS, OPLS and Amber phospho residue names fold to their parent.**
  `HISA`/`HISB` (GROMOS histidine tautomers), `ARGN`, `LYSN`, `CYS1`,
  `ASN1`, `PGLU` and the phosaa names `S1P`/`S2P`, `T1P`/`T2P`, `Y1P`/`Y2P`
  were `X` in `md_polymer.sequence` with the raw name in `residues`, so a
  GROMOS protein had a different polymer key from the same protein under
  another force field and, with more than three histidines, lost its declared
  and search UniProt references (review SEQ-8, H4). Two parent letters were
  wrong: `HRG` (homoarginine) is `R`, not `H`, and `OMT` (methionine sulfone)
  is `M`, not `T`; `HYP`, `NLE`, `ALY`, `CSD` and `SAR` now take their CCD
  parent's letter (SEQ-9).
- **The strands of a DNA/RNA duplex are separate chains.** A strand's 5'
  nucleotide has no P, so there was no O3'-P bond to measure and the
  numbering decided; tleap numbers both strands straight through and
  cpptraj writes no chain IDs, so every Amber duplex was one chain (1BNA a
  24-nt strand; review SEQ-2). A nucleotide without a P after one with an
  O3' now starts a chain, as does one the names or atoms call a terminus
  (`DC5`/`DG3`, `H5T`/`HO5'`, `H3T`/`HO3'`). `md_chain`/`md_polymer` rows
  of duplexes imported before this hold the merged strand.
- **A chain broken across the periodic box is one chain.** The C-N and
  O3'-P distances are measured under the minimum image of the CRYST1 cell
  (triclinic cells included), so a chain that mdrun's per-atom wrapping or
  cpptraj's autoimage left spanning the box is no longer cut where the bond
  crosses it (review SEQ-4). A CRYST1 with an edge under 10 A (GROMACS's
  `1 1 1` placeholder for no box) is not a cell.
- **A cached result is reused only for the inputs it was made from**
  (review H3, F12, TQ-07). BLAST results in a processed or work directory
  were reused on their existence alone and read by FASTA ID, so after any
  change to the chains (a reprocess without `--force` after a splitter fix,
  `backfill-chains`' dry run against a different input) another chain's
  hits were taken as this one's at 100% identity with no warning. The query
  FASTA is now kept beside each results file (`polymers.<db>.fa`,
  `blast.<db>.fa`) and the results are reused only when it is byte-identical
  to this run's; a row for another query, length or residues is dropped with
  a warning. `sequence.fa` is always rewritten. `inferred_ligands.json` is
  reused only for the same `mol_id.py` and `minimal.pdb`, recorded in
  `inferred_ligands.inputs.json` (a file `bulk_process_local.py` seeds
  records no structure, and is still invalidated by a change to
  `mol_id.py`). The converting run's time-axis verdict is kept in
  `rep_N/source_timing.json`, so a re-run that finds the outputs present
  gives the same duration verdict instead of refusing a time-less source it
  accepted the first time.
- **Massless sites are not heavy atoms.** simulation-processing now writes
  every cpptraj PDB with `include_ep` (review CONV-2), so an Amber extra
  point (element `XP`) or a CHARMM lone pair (blank element after
  `psf_elements`, or no element column) reaches the chain reader; neither
  counts in a residue's heavy-atom formula, which the ligand checks compare.

Also in 0.19.0:

- **A fractional integration timestep (#51).** `integration_timestep_fs` is
  a float (`f64`), so 2.5 fs (Zenodo 3975394) is accepted; the range stays
  1-20 fs and TOMLs that say `= 2` still parse. `md_simulation.integration_timestep_fs`
  is `Float8`, matching md-repo-app migration 0296. Deploy the two together:
  this build cannot read the old integer column, and 0.18.0 cannot read the
  new one (it fails with `Error deserializing field 'integration_timestep_fs'`).
- **The thumbnail and RMSD/RMSF helpers have a time limit (#52, #53).** A hung
  `create_preview.py` or `get_rmsd_rmsf.py` used to hang mdr-process with it
  (PLAS-20K 5nd5, 4 h). Both now stop after 5 minutes and fail with an error
  naming the step; set `MDR_HELPER_TIMEOUT_SECS` to change the limit.

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
