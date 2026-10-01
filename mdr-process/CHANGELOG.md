# Changelog

Notable changes to `mdr-process`, newest first. `mdr-process` is never
released on GitHub and has no tags: a host gets it by building `main`
(`just mdr-process`), and `mdr-process --version` names the build, e.g.
`mdr-process 0.2.0 (048d03d)`. See RELEASE.md, "Versioning `mdr-process`".

Bump the version in `mdr-process/Cargo.toml` in the same PR as the change:

- **minor** (0.2.0 -> 0.3.0) for any change in what it writes to the
  database or iRODS, or in its command line;
- **patch** (0.2.0 -> 0.2.1) for everything else.

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
