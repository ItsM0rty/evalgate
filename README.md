# Evalgate

Evalgate runs model evaluation suites and exits with status 1 when a score gate fails or a comparison finds a regression. It supports live OpenAI-compatible and Anthropic targets, deterministic echo and replay targets, weighted checks, repeats, snapshots, cache-backed judge and embedding checks, and CI report formats.

## Run it

Rust 1.85 or newer is required.

```bash
cargo build --release -p evalgate-cli
./target/release/evalgate run examples/suite.yaml --format table
./target/release/evalgate run examples/suite.yaml --repeat 5 --format json --output current.json
./target/release/evalgate compare baseline.json current.json --format markdown --tolerance 0.02
```

`run` accepts `--no-cache`, `--update-snapshots`, and `--format table|json|junit`. Cache entries and snapshots are stored under `.evalgate/` beside the suite.

## Suite schema

```yaml
suite:
  name: Support replies
  pass_threshold: 0.9
  flaky_min_rate: 0.01
  flaky_max_rate: 0.99
targets:
  - name: recorded
    type: replay
    file: replay.jsonl
    concurrency: 4
    requests_per_second: 10
    retries: 2
    timeout_ms: 30000
cases:
  - name: shipping reply
    input: { order: ORD-12345 }
    prompt: Where is {{order}}?
    pass_threshold: 1
    checks:
      - { kind: json_valid }
      - { kind: includes, value: ORD-12345, weight: 2 }
```

Top-level keys are `suite`, `targets`, and `cases`. Suite and case thresholds range from 0 to 1. Target names and case names must be unique.

Target types:

- `openai`: POSTs chat completions and reads `choices[0].message.content`.
- `anthropic`: POSTs the Messages API and reads `content[0].text`.
- `echo`: returns the rendered prompt.
- `replay`: reads JSONL records with `case`, `output`, and optional `latency_ms` and `cost_usd`.

Live targets accept `endpoint`, `model`, `api_key_env`, `params`, `concurrency`, `requests_per_second`, `retries`, and `timeout_ms`. Requests retry HTTP 429 and 5xx responses with exponential backoff and jitter.

Check kinds:

- Text: `exact`, `includes`, `starts_with`, `ends_with`.
- Pattern: `matches`, `not_matches`.
- JSON: `json_valid`, `json_shape`. Shape rules support `type`, `required`, `properties`, `items`, `enum`, `min`, and `max`.
- Similarity: `edit_distance`, `ngram_overlap`, with a `threshold`.
- Operational: `max_latency_ms`, `max_cost_usd`.
- Stateful: `snapshot`.
- External: `judge`, `semantic`.

Every check accepts a positive `weight`. A judge check requires an OpenAI-compatible `endpoint` and parses only `{"pass":bool,"score":number,"reason":string}`. A semantic check sends the output and expected string to an embeddings `endpoint`, then compares their cosine score with `threshold`.

## CI

The checked-in workflow runs the replay suite on pull requests, writes JSON and JUnit reports, compares against a baseline when present, and appends Markdown to the GitHub job summary. Replay keeps CI independent of API keys.

## Development

```bash
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
wasm-pack build crates/evalgate-wasm --target web --release
```

## License

MIT. See `LICENSE` and `THIRD_PARTY_NOTICES.md`.
