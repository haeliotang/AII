# Process models (M7)

`water.py` is a deliberately small, **synthetic and uncalibrated** steady-state
model of a raw-water pump plus one filtration unit. It exists to exercise the
Rust ↔ Python bridge: protocol versioning, units, deadlines, validity ranges and
classified failures. It is not a plant model and must never be presented as one.

## Model

| Field | Value |
| --- | --- |
| ID / version | `water-treatment` / `0.1.0` |
| Inputs | inlet flow (m³/h), turbidity (NTU), filter loading (0..1) |
| Validity | flow 5–15 m³/h, turbidity 5–50 NTU, loading 0–0.9 |
| Outputs | service level (0..1), potable flow (m³/h), reject flow (m³/h), head loss (kPa), energy (kWh) |
| Mass | potable + reject = inlet (within 1%) |
| Calibration | none; all parameters are synthetic assumptions (`synthetic = true`) |

Equations: filtrate fraction `0.9 − 0.5·loading`; service level is the weaker of
`potable/12 m³/h` and `1 − turbidity/100`; head loss `20 + 60·loading` kPa.

## Protocol

One JSON object per line on stdin/stdout. Request: `schema`, `model {id,version}`,
`parameters`, `expected_units`, `deadline_ms`, optional `faults`. Response:
`schema`, `model`, `status` (`ok`/`error`), values, `units`, `validity`,
`synthetic`. Error classes: `out_of_validity`, `non_convergence`,
`numerical_failure`, `model_mismatch`, `protocol_error`. Timeouts and crashes are
detected by the Rust client (deadline, process exit) and are never model
results.

`bridge_server.py` is the subprocess entry point; the Rust client is
`crates/poie-process`.

## Calibration plan

See [DATA.zh-CN.md](DATA.zh-CN.md). The model stays `synthetic` in reports until
calibration data is available; IDAES is the candidate for a higher-fidelity
model behind the same protocol (ADR-0005).
