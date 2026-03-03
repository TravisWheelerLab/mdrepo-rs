# Project

MDRepo Rust tools

## Installation

* Install Rust (https://rust-lang.org/tools/install/)
* `cargo build`

## Usage

```
$ cargo run --quiet --bin mdr -- -h
Usage: mdr [OPTIONS] [COMMAND]

Commands:
  validate    Validate simulation directory
  meta-check  Check metadata
  process     Process simulation directory
  reprocess   Reprocess an existing simulation
  ticket      Use ticket ID to download and process
  help        Print this message or the help of the given subcommand(s)

Options:
  -l, --log <LOG>            Log level [possible values: info, debug]
      --log-file <LOG_FILE>  Log file
  -h, --help                 Print help
  -V, --version              Print version

$ cargo run --quiet --bin mdr-toml -- -h
Usage: mdr-toml <FILENAME>

Arguments:
  <FILENAME>

Options:
  -h, --help  Print help
```

## Contributing

We welcome external contributions, although it is a good idea to contact the
maintainers before embarking on any significant development work to make sure
the proposed changes are a good fit.

Contributors agree to license their code under the license in use by this
project (see `LICENSE`).

To contribute:

  1. Fork the repo
  2. Make changes on a branch
  3. Create a pull request

## License

See `LICENSE` for details.

## Authors

* Ken Youens-Clark <kyclark@arizona.edu>

See `AUTHORS` the full list of authors.
