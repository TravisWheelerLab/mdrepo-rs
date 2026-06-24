# mdr-webcheck

A CLI tool that checks MDRepo web endpoints against expected HTTP status codes and content. Browser-rendered pages (Elm frontend) are tested with a headless Chromium instance; JSON API endpoints are tested directly with `reqwest`.

## Building

```
cargo build --release
```

## Usage

```
mdr-webcheck [OPTIONS]
```

### Options

| Flag | Default | Description |
|------|---------|-------------|
| `-u, --url <HOST>` | `localhost` | Base host to check (`localhost`, `staging.mdrepo.org`, `mdrepo.org`) |
| `-p, --port <PORT>` | — | Port to append to the URL (e.g. `80`) |
| `--http` | — | Force HTTP instead of HTTPS (automatic for localhost/127.0.0.1) |
| `-c, --config <FILE>` | `endpoints.toml` | Path to the TOML config file |
| `-t, --timeout <SECS>` | `30` | Request timeout in seconds |
| `-v, --verbose` | — | Print details for passing checks as well as failures |
| `--chrome <PATH>` | auto-detected | Path to a Chrome/Chromium binary |
| `-f, --filter <SUBSTRING>` | — | Only run checks whose path contains this string (case-insensitive) |
| `--admin-token <TOKEN>` | — | Secret token for authenticated checks (see below) |

### Examples

```bash
# Check all endpoints against the local dev server on port 80
mdr-webcheck --url localhost --port 80

# Check staging
mdr-webcheck --url staging.mdrepo.org

# Check production, only paths containing "uniprot"
mdr-webcheck --url mdrepo.org --filter uniprot

# Check with admin authentication (required for protected pages)
mdr-webcheck --url mdrepo.org --admin-token "$ADMIN_SECRET_KEY"
```

## Config file (`endpoints.toml`)

Each `[[endpoints]]` entry describes one URL to check.

```toml
[[endpoints]]
path = "/explore"
contains = ["Found"]

[[endpoints]]
path = "/api/v1/simulation_list"
browser = false

  [[endpoints.json_checks]]
  path = "count"
  gt = 0
```

### Endpoint fields

| Field | Default | Description |
|-------|---------|-------------|
| `path` | required | URL path, e.g. `/explore` |
| `host` | — | If set, only run this check when `--url` matches exactly |
| `expected_status` | `200` | Expected HTTP status code |
| `browser` | `true` | Use a headless browser (needed for Elm-rendered pages). Set `false` for JSON API endpoints. |
| `contains` | `[]` | Strings that must appear in the response body / rendered page |
| `not_contains` | `[]` | Strings that must NOT appear |
| `json_checks` | `[]` | Structured checks against JSON fields (only used when `browser = false`) |

### JSON checks

JSON field paths use dot-notation with optional bracket indexing (e.g. `results[0].title`).

```toml
[[endpoints.json_checks]]
path = "count"
gt = 0           # numeric: must be greater than this value

[[endpoints.json_checks]]
path = "slug"
eq = "MDR00016143"   # must equal this value (alias: equals)

[[endpoints.json_checks]]
path = "short_description"
contains = "luciferase"   # string: must contain this substring

[[endpoints.json_checks]]
path = "optional_field"
exists = false   # field must be absent (default: true = must exist)
```

## Authentication

Some pages require a logged-in user. The `--admin-token` flag handles this transparently for both check types:

- **Browser checks** (`browser = true`): before running any page checks, the tool navigates to `/api/v1/admin_login?admin_token=<token>`, which sets a session cookie in the shared browser instance. All subsequent browser page loads are authenticated.
- **API checks** (`browser = false`): `?admin_token=<token>` is appended to each URL automatically.

The token must match the `ADMIN_SECRET_KEY` environment variable set on the Django backend. When matched, the backend authenticates the request as `mdrepo_admin` (user pk=1).

```bash
# Recommended: read from environment, never hard-code the token
mdr-webcheck --url mdrepo.org --admin-token "$ADMIN_SECRET_KEY"
```

## Chrome / headless browser

The tool auto-detects a Chrome or Chromium binary from standard locations. On Apple Silicon Macs it uses `lipo` to verify the binary includes an `arm64` slice and skips any x64-only binary to avoid running under Rosetta.

Detection order:
1. `--chrome <path>` if specified
2. `/Applications/Google Chrome.app/Contents/MacOS/Google Chrome`
3. `/Applications/Chromium.app/Contents/MacOS/Chromium`
4. `/usr/bin/google-chrome`, `/usr/bin/chromium`, `/usr/bin/chromium-browser`

If no compatible binary is found and any endpoint has `browser = true`, the tool exits with an error. Install [Google Chrome](https://www.google.com/chrome/) or point `--chrome` at an ARM-native Chromium.

The browser is only launched if at least one endpoint has `browser = true`. Runs with all endpoints set to `browser = false` have no Chromium dependency.

## Exit codes

| Code | Meaning |
|------|---------|
| `0` | All checks passed |
| `1` | One or more checks failed, or a fatal error occurred |
