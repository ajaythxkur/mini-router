# Mini Router

An API gateway that lets AI agents discover, inspect, and pay for tool calls through one API key, written in Rust.

> **Status:** v0.1 (Engine) complete. Building v0.2 (HTTP API).

## What it does

Tools are defined as **data, not code**: each one is a TOML file describing an HTTP endpoint, its input schema, and its price. One generic engine validates input against the schema, builds the request, calls the vendor, and returns JSON. Adding a tool means adding a file, with no new code.

## Quickstart

Requires [Rust](https://rustup.rs).

```bash
git clone https://github.com/ajaythxkur/mini-router.git
cd mini-router

cargo run -- list
cargo run -- run open-meteo#forecast '{"latitude":28.6,"longitude":77.2,"current":"temperature_2m"}'
```

Invalid input is rejected before any network call:

```
$ cargo run -- run open-meteo#forecast '{"latitude":500,"longitude":77.2}'
error: invalid input: /latitude: 500 is greater than the maximum of 90
```

### HTTP API

```bash
cargo run -- serve        # listens on http://0.0.0.0:8080
```

| Method | Path | What it does |
|---|---|---|
| GET | `/health` | Liveness check |
| GET | `/discover?q=weather` | Ranked search; omit `q` to list all |
| GET | `/connectors/{provider}/{endpoint}` | Full connector details and input schema |
| POST | `/connectors/{provider}/{endpoint}/run` | Run with a JSON body as input |

```bash
curl -s -X POST localhost:8080/connectors/open-meteo/forecast/run \
  -H 'content-type: application/json' \
  -d '{"latitude":52.52,"longitude":13.41,"current":"temperature_2m"}'
```

Errors are JSON: `{"error":{"code":"invalid_input","message":"..."}}`; vendor failures also include `vendor_status`.

## Connectors

12 free, keyless APIs:

| Connector | What it does |
|---|---|
| `open-meteo#forecast` | Weather forecast for a location |
| `nominatim#search` | Place name → coordinates |
| `wikipedia#summary` | Wikipedia article summary |
| `hackernews#search` | Hacker News search |
| `github#search-repos` | GitHub repository search |
| `coingecko#price` | Crypto prices |
| `frankfurter#latest` | Currency exchange rates |
| `openlibrary#search` | Book search |
| `crossref#works` | Research paper metadata |
| `wiktionary#define` | Any language definitions |
| `npm#package` | npm package metadata |
| `pypi#package` | PyPI package metadata |

## Adding a connector

Create `connectors/<provider>/<endpoint>.toml`:

```toml
id = "wikipedia#summary"
display_name = "Wikipedia Summary"
description = "Short summary of an English Wikipedia article by exact title."
categories = ["knowledge"]

[request]
method = "GET"
base_url = "https://en.wikipedia.org"
path = "/api/rest_v1/page/summary/{title}"   # {title} is filled from input, URL-encoded

[pricing]
kind = "per_call"
amount = "0.0005"

[input]   # JSON Schema
type = "object"
required = ["title"]
additionalProperties = false
properties.title = { type = "string", minLength = 1 }
```

`cargo test` checks that every connector file loads and has a valid schema.

## Project layout

```
crates/
├── core/     mini-router-core: connector format, loader, run engine (no HTTP server)
└── server/   mini-router: CLI and HTTP API (axum)
connectors/   connector definitions (TOML)
```

## Design decisions

- **Connectors as data.** No per-vendor code; the engine is the same for every tool.
- **Validate before calling.** JSON Schema validation runs locally, so bad input costs nothing.
- **Exact money.** Prices are `Decimal`, never floats.
- **Offline tests.** HTTP behaviour is tested against a local mock server (`wiremock`); live API tests are opt-in with `cargo test -- --ignored`.

## Roadmap

- [x] v0.1: Connector engine with 12 connectors
- [ ] v0.2: HTTP API (`/discover`, `/inspect`, `/run`) + Postgres + API keys
- [ ] v0.3: Prepaid wallet with a ledger + Stripe top-ups (test mode)
- [ ] v0.4: MCP server for AI agents + deployment
- [ ] Detect vendor errors returned with HTTP 200

## License

MIT
