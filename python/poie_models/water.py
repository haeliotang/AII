"""Simplified water-treatment process model (M7.2; synthetic, uncalibrated).

A deliberately small steady-state model of a raw-water pump plus one filtration
unit. It exists to exercise the bridge protocol, validity checks and failure
handling; it is NOT a calibrated plant model. See DATA.zh-CN.md for the
calibration plan and README.md for the declared validity range.
"""

import math

MODEL_ID = "water-treatment"
MODEL_VERSION = "0.1.0"

# All ranges are synthetic assumptions (M7.5).
INPUT_RANGES = {
    "inlet_flow_m3_h": (5.0, 15.0),
    "turbidity_ntu": (5.0, 50.0),
    "filter_loading": (0.0, 0.9),
}

NOMINAL_FLOW_M3_H = 12.0


def validity_of(params):
    """Returns (within, ranges) for the supplied parameters."""
    within = True
    for name, (low, high) in INPUT_RANGES.items():
        value = params.get(name)
        if value is None or not isinstance(value, (int, float)) or not math.isfinite(value):
            within = False
            continue
        if value < low or value > high:
            within = False
    return within, {name: list(bounds) for name, bounds in INPUT_RANGES.items()}


def evaluate(params):
    """Evaluates the model; returns a dict with status/values/validity."""
    within, ranges = validity_of(params)
    if not within:
        return {
            "status": "error",
            "error": "out_of_validity",
            "message": "inputs outside the declared validity range",
            "validity": {"ranges": ranges, "within": False},
        }

    inlet = float(params["inlet_flow_m3_h"])
    turbidity = float(params["turbidity_ntu"])
    loading = float(params["filter_loading"])

    # Filtrate fraction falls with loading; the rest is reject flow.
    filtrate_fraction = 1.0 - 0.1 - 0.5 * loading
    potable = inlet * filtrate_fraction
    reject = inlet - potable

    # Service level: the weaker of "fraction of nominal flow" and "quality".
    flow_fraction = potable / NOMINAL_FLOW_M3_H
    quality_fraction = 1.0 - turbidity / 100.0
    service_level = max(0.0, min(1.0, min(flow_fraction, quality_fraction)))

    head_loss_kpa = 20.0 + 60.0 * loading
    energy_kwh = 0.05 * inlet + 0.1

    return {
        "status": "ok",
        "service_level": service_level,
        "potable_flow_m3_h": potable,
        "reject_flow_m3_h": reject,
        "head_loss_kpa": head_loss_kpa,
        "energy_kwh": energy_kwh,
        "validity": {"ranges": ranges, "within": True},
        "synthetic": True,
    }
