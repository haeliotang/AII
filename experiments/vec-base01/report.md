Viability report (viability-expansion/0.2, method enumerated, horizon 86400s)
scenarios: development 20, holdout 0 (holdout viewed in development: false)
solver: poie-viability 0.1.0 + poie-sim timed 0.1.0 (finite fixed-trajectory set (20 pre-registered cases); no random sampling)
model water-treatment v0.1.0 (synthetic: true; validity: filter_loading 0-0.9; inlet_flow_m3_h 5-15; turbidity_ntu 5-50)
verdicts: counterexample: 7, held_on_tested: 8, infeasible_in_model: 4, unknown_input: 1
- T0: held on 1 tested scenario(s) with the enumerated method; only the fixed trajectory of case T0; untested disturbances are not covered, and this is not a claim of safety or robustness
- T1: counterexample in scenario T1: constraint "deadline" first violated at 14400s (state: completion 21600s after deadline 14400s)
- T2: counterexample in scenario T2: constraint "electric-energy" first violated at 0s (state: electric-energy needs 2 kWh, only 1 kWh available at 0s)
- T3: held on 1 tested scenario(s) with the enumerated method; only the fixed trajectory of case T3; untested disturbances are not covered, and this is not a claim of safety or robustness
- T4: counterexample in scenario T4: constraint "deadline" first violated at 43200s (state: completion 45000s after deadline 43200s)
- T5: held on 1 tested scenario(s) with the enumerated method; only the fixed trajectory of case T5; untested disturbances are not covered, and this is not a claim of safety or robustness
- T6: infeasible in the model; certificate "capability-graph blocked: below_verification turn-lathe-nut", checked by "cargo run -p poie-ontology --bin poie-graph -- scenarios/base-01/cases.toml"
- T7: cannot decide: unknown inputs T7: unknown_state pv-1
- T8: held on 1 tested scenario(s) with the enumerated method; only the fixed trajectory of case T8; untested disturbances are not covered, and this is not a claim of safety or robustness
- T9: counterexample in scenario T9: constraint "deadline" first violated at 18000s (state: completion 21600s after deadline 18000s)
- T10: counterexample in scenario T10: constraint "electric-energy" first violated at 14400s (state: electric-energy needs 2 kWh, only 1 kWh available at 14400s)
- T11: infeasible in the model; certificate "capability-graph blocked: cycle bus-1", checked by "cargo run -p poie-ontology --bin poie-graph -- scenarios/base-01/cases.toml"
- T12: held on 1 tested scenario(s) with the enumerated method; only the fixed trajectory of case T12; untested disturbances are not covered, and this is not a claim of safety or robustness
- T13: infeasible in the model; certificate "capability-graph blocked: missing_resource bronze-bar", checked by "cargo run -p poie-ontology --bin poie-graph -- scenarios/base-01/cases.toml"
- T14: held on 1 tested scenario(s) with the enumerated method; only the fixed trajectory of case T14; untested disturbances are not covered, and this is not a claim of safety or robustness
- T15: counterexample in scenario T15: constraint "bronze-bar" first violated at 39600s (state: bronze-bar needs 1.2 kg, only 0 kg available at 39600s)
- T16: counterexample in scenario T16: constraint "deadline" first violated at 43200s (state: completion 46800s after deadline 43200s)
- T17: held on 1 tested scenario(s) with the enumerated method; only the fixed trajectory of case T17; untested disturbances are not covered, and this is not a claim of safety or robustness
- T18: infeasible in the model; certificate "capability-graph blocked: cycle bus-1", checked by "cargo run -p poie-ontology --bin poie-graph -- scenarios/base-01/cases.toml"
- T19: held on 1 tested scenario(s) with the enumerated method; only the fixed trajectory of case T19; untested disturbances are not covered, and this is not a claim of safety or robustness
note: censored survival times are marked censored; no mean is reported from them
metric groups: independence, process, resilience, viability
no confidence interval computed; finite samples are not a robustness guarantee
