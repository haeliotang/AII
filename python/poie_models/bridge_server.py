#!/usr/bin/env python3
"""JSON-line bridge server for the simplified water-treatment model (M7.1).

Reads one request object per line from stdin, writes one response object per
line to stdout and exits at EOF. Fault injection is used by the Rust tests and
demonstrations; the normal path has no faults.
"""

import json
import sys
import time

from water import MODEL_ID, MODEL_VERSION, evaluate

SCHEMA = "poie.process-bridge/0.1"


def respond(payload):
    print(json.dumps(payload), flush=True)


def error(request, code, message, validity=None):
    payload = {
        "schema": SCHEMA,
        "model": (request or {}).get("model") or {"id": MODEL_ID, "version": MODEL_VERSION},
        "status": "error",
        "error": code,
        "message": message,
    }
    if validity is not None:
        payload["validity"] = validity
    respond(payload)


def main():
    for line in sys.stdin:
        line = line.strip()
        if not line:
            continue
        try:
            request = json.loads(line)
        except json.JSONDecodeError as exc:
            error(None, "protocol_error", f"invalid JSON: {exc}")
            continue

        faults = request.get("faults") or {}
        if faults.get("sleep_ms"):
            time.sleep(float(faults["sleep_ms"]) / 1000.0)
        if faults.get("crash"):
            sys.exit(3)

        model = request.get("model") or {}
        if model.get("id") != MODEL_ID or model.get("version") != MODEL_VERSION:
            error(request, "model_mismatch", f"only {MODEL_ID} v{MODEL_VERSION} is served")
            continue

        parameters = request.get("parameters") or {}
        result = evaluate(parameters)
        if result["status"] != "ok":
            error(request, result.get("error", "protocol_error"), result.get("message", ""), result.get("validity"))
            continue

        if faults.get("non_convergence"):
            error(request, "non_convergence", "solver did not converge (injected)")
            continue
        if faults.get("numerical_failure"):
            error(request, "numerical_failure", "non-finite residual (injected)")
            continue

        units = {
            "service_level": "1",
            "potable_flow_m3_h": "m3/h",
            "reject_flow_m3_h": "m3/h",
            "head_loss_kpa": "kPa",
            "energy_kwh": "kWh",
        }
        payload = {
            "schema": SCHEMA,
            "model": {"id": MODEL_ID, "version": MODEL_VERSION},
            "status": "ok",
            "service_level": result["service_level"],
            "potable_flow_m3_h": result["potable_flow_m3_h"],
            "reject_flow_m3_h": result["reject_flow_m3_h"],
            "head_loss_kpa": result["head_loss_kpa"],
            "energy_kwh": result["energy_kwh"],
            "units": units,
            "validity": result["validity"],
            "synthetic": result["synthetic"],
        }
        if faults.get("violate_conservation"):
            payload["reject_flow_m3_h"] += 0.5
        if faults.get("wrong_unit"):
            payload["units"]["potable_flow_m3_h"] = "L/s"
        respond(payload)


if __name__ == "__main__":
    main()
