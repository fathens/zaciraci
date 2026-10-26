# ref_flow — NearBlocks Ref Finance flow collector

Collects the `swap` / `ft_on_transfer` receipts of `v2.ref-finance.near`
from the NearBlocks v3 API, reconstructs the executed trades of every
successful transaction and stores them in `ref_flow_receipts` /
`ref_flow_txs` / `ref_flow_trades` (see `persistence::ref_flow`). It runs
inside the backend process as a cron-style task and replaces the Python
collector in `.tmp/ref_flow/collect.py`.

## Modules

| module | role |
|---|---|
| `budget` | request budget policy (Standard plan constants, daily allowance, forward reserve, walls, pacing); pure functions, golden-tested against the frozen Python reference |
| `accounting` | per-cycle tracker: pre-charges the day ledger in blocks and feeds the client |
| `credit` | `Credit` permit (one per HTTP attempt, fail-closed) and `Reservation` |
| `rate_limit` | per-minute window + burst interval limiter |
| `client` | hardened reqwest client and the `NearBlocksApi` trait |
| `parse` | list items → `RefReceipt`; receipt trees → `Trade` from the contract's `Swapped` logs |
| `state` | `CollectorState` (forward cursor, gaps, cooldown) stored behind a CAS version |
| `collector` | one cycle: forward → recent gaps → enrich → old gaps → cleanup |
| `runner` | the cron loop spawned by `backend` |
| `import` (feature `importer`) | one-off migration of the Python raw archive |

## Configuration

| key | where | default | meaning |
|---|---|---|---|
| `NEARBLOCKS_API_KEY` | fly secret / env (startup) | unset | Bearer key; the collector stays idle without it |
| `REF_FLOW_ENABLED` | config_store / env | `false` | kill switch, re-read every tick |
| `REF_FLOW_CRON_SCHEDULE` | config_store / env | `0 0,30 * * * *` | cycle schedule (a hard 10-minute floor applies regardless) |
| `REF_FLOW_RETENTION_DAYS` | config_store / env | `90` | retention of receipts / txs / trades (clamped to ≥ 31) |
| `REF_FLOW_DAY_ALLOW_CAP` | config_store / env | `113000` | operator cap on the daily request budget; can only lower the built-in limit |

Plan limits (`budget::STANDARD`) are compile-time constants on purpose.

## Operations

- **Enable**: set the secret, deploy, then `REF_FLOW_ENABLED=true` in
  config_store. First start initialises the state row with the forward
  cursor at "now"; earlier history is the importer's job.
- **Parallel run with the Python collector** (migration): lower the Python
  `day_max` and set `REF_FLOW_DAY_ALLOW_CAP` so both fit the plan; the
  importer can run while the collector is live because live rows win.
- **Watch**: `ref_flow_runs` (freshness of the latest `fwd` row, `note`
  with `stop=` reasons, `parse_stats`), `ref_flow_ledger` (daily
  requests vs. the plan), and coverage via
  `persistence::ref_flow::tx::coverage_by_bucket`.
- **Re-enrich a range** (parser fix): `persistence::ref_flow::tx::reset_enrichment`.

## Importer

```
cargo run -p ref_flow --features importer --bin ref_flow_import -- \
    --raw-dir .tmp/ref_flow/raw --since 2026-09-10 --until 2026-10-25 \
    --compare-csv .tmp/ref_flow/analysis/trades.csv          # dry run
# add --apply to write; DATABASE_URL selects the target (use `fly proxy`
# for mpg). State, ledger and run tables are never touched.
```

The comparison reports `matched` / `new_here` / `mismatched` / `csv_only`.
`new_here` is expected to be a few percent: the Python extractor dropped
ft routes that unwrap NEAR and swap+withdraw batches.
