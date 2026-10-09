# Deploys pin to Cargo.lock. `--locked` makes cargo assert the lock will not
# change, which is both a reproducibility guarantee -- the binary you install
# is built from the versions the repo records -- and the reason this file no
# longer runs `cargo update`.
#
# It used to be `cargo update && cargo install --path .`, so every deploy
# silently upgraded every transitive dependency and left Cargo.lock dirty in
# the working tree. On 2026-08-18 that was a 143-line diff nobody had asked
# for, sitting uncommitted across a day's work and obscuring real changes.
# `cargo metadata --locked` confirmed the committed lock had been consistent
# the whole time, so none of those upgrades were needed to build.
INSTALL := "cargo install --path . --locked"

all: mdr-meta mdr-process mdr-export

mdr-meta:
    cd mdr-meta && {{INSTALL}}

mdr-process:
    cd mdr-process && {{INSTALL}}

mdr-export:
    cd mdr-export && {{INSTALL}}

# Upgrading dependencies is now a thing you ask for, not a side effect of
# deploying. Run it, look at the diff, build, test, and commit Cargo.lock as
# its own change -- so a dependency bump can be bisected and reverted like
# anything else.

# Bump dependencies deliberately, then review and commit Cargo.lock
update:
    cargo update
    @echo
    @echo "Cargo.lock updated. Review 'git diff Cargo.lock', rebuild, and"
    @echo "commit it deliberately -- deploys use --locked and will not do it."

# The PDB index mdr-process's polymer UniProt lookup reads (reference.rs):
# every PDB chain's sequence (wwPDB) and SIFTS's chain-to-UniProt segments
# (EBI), uncompressed, in <blast_dir>/pdb. Refresh it with the BLAST
# databases; each file is replaced whole, so an import never reads half of
# one. Without it, imports leave polymers unlooked-up (with a warning).
pdb-index blast_dir="/opt/mdrepo/blast":
    mkdir -p {{blast_dir}}/pdb
    curl -sSfL https://files.wwpdb.org/pub/pdb/derived_data/pdb_seqres.txt.gz | gunzip > {{blast_dir}}/pdb/pdb_seqres.txt.tmp
    curl -sSfL https://ftp.ebi.ac.uk/pub/databases/msd/sifts/flatfiles/tsv/pdb_chain_uniprot.tsv.gz | gunzip > {{blast_dir}}/pdb/pdb_chain_uniprot.tsv.tmp
    mv {{blast_dir}}/pdb/pdb_seqres.txt.tmp {{blast_dir}}/pdb/pdb_seqres.txt
    mv {{blast_dir}}/pdb/pdb_chain_uniprot.tsv.tmp {{blast_dir}}/pdb/pdb_chain_uniprot.tsv
