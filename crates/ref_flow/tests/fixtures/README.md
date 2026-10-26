# ref_flow test fixtures

- `budget_golden.json` — expected outputs of the request-budget policy,
  produced by `generators/gen_budget_golden.py` from
  `generators/budget_reference.py` (a frozen copy of the Python collector's
  `budget.py`, the specification the Rust port follows). Deterministic
  (seeded); regenerate only when the reference changes on purpose:

  ```
  cd generators && python3 -I gen_budget_golden.py > ../budget_golden.json
  ```

  `cargo test` never runs Python.
