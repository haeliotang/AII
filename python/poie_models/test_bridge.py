"""Stdlib-only tests for the water model and the JSON-line bridge (M7).

These tests run without third-party packages so CI can execute them with the
system Python. They cover the model's validity/mass contract and the bridge's
protocol and injected-fault behaviour; deadlines and process exits are also
covered from the Rust side (crates/poie-process).
"""

import json
import subprocess
import sys
import unittest
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))

import water  # noqa: E402  (path is set up above)

BRIDGE = HERE / "bridge_server.py"
SCHEMA = "poie.process-bridge/0.1"


def run_bridge(requests, expect_exit=None):
    payload = "\n".join(json.dumps(request) for request in requests) + "\n"
    completed = subprocess.run(
        [sys.executable, str(BRIDGE)],
        input=payload,
        capture_output=True,
        text=True,
        timeout=30,
    )
    if expect_exit is not None:
        if completed.returncode != expect_exit:
            raise AssertionError(f"expected exit {expect_exit}, got {completed.returncode}: {completed.stderr}")
    responses = [json.loads(line) for line in completed.stdout.splitlines() if line.strip()]
    return completed, responses


def request(parameters=None, faults=None):
    return {
        "schema": SCHEMA,
        "model": {"id": water.MODEL_ID, "version": water.MODEL_VERSION},
        "parameters": parameters
        or {"inlet_flow_m3_h": 10.0, "turbidity_ntu": 20.0, "filter_loading": 0.5},
        "expected_units": {
            "service_level": "1",
            "potable_flow_m3_h": "m3/h",
            "reject_flow_m3_h": "m3/h",
            "head_loss_kpa": "kPa",
            "energy_kwh": "kWh",
        },
        "deadline_ms": 2_000,
        **({"faults": faults} if faults else {}),
    }


class WaterModelTests(unittest.TestCase):
    def test_nominal_evaluation_respects_mass_balance_and_bounds(self):
        result = water.evaluate(
            {"inlet_flow_m3_h": 10.0, "turbidity_ntu": 20.0, "filter_loading": 0.5}
        )
        self.assertEqual(result["status"], "ok")
        self.assertTrue(result["synthetic"])
        self.assertGreaterEqual(result["service_level"], 0.0)
        self.assertLessEqual(result["service_level"], 1.0)
        mass_in = 10.0
        mass_out = result["potable_flow_m3_h"] + result["reject_flow_m3_h"]
        self.assertAlmostEqual(mass_in, mass_out, delta=0.01 * mass_in)
        self.assertTrue(result["validity"]["within"])

    def test_out_of_validity_is_classified_not_clamped(self):
        result = water.evaluate(
            {"inlet_flow_m3_h": 20.0, "turbidity_ntu": 20.0, "filter_loading": 0.5}
        )
        self.assertEqual(result["status"], "error")
        self.assertEqual(result["error"], "out_of_validity")
        self.assertFalse(result["validity"]["within"])
        # An out-of-range value must never be silently clamped to a result.
        self.assertNotIn("service_level", result)


class BridgeProtocolTests(unittest.TestCase):
    def test_ok_response_carries_units_and_synthetic_flag(self):
        _, responses = run_bridge([request()])
        self.assertEqual(len(responses), 1)
        response = responses[0]
        self.assertEqual(response["schema"], SCHEMA)
        self.assertEqual(response["status"], "ok")
        self.assertEqual(response["model"], {"id": water.MODEL_ID, "version": water.MODEL_VERSION})
        self.assertEqual(response["units"]["potable_flow_m3_h"], "m3/h")
        self.assertEqual(response["units"]["service_level"], "1")
        self.assertTrue(response["synthetic"])

    def test_several_requests_in_one_process(self):
        _, responses = run_bridge(
            [
                request({"inlet_flow_m3_h": 8.0, "turbidity_ntu": 10.0, "filter_loading": 0.1}),
                request({"inlet_flow_m3_h": 12.0, "turbidity_ntu": 30.0, "filter_loading": 0.8}),
            ]
        )
        self.assertEqual(len(responses), 2)
        self.assertTrue(all(response["status"] == "ok" for response in responses))

    def test_model_mismatch_is_an_error_response(self):
        mismatched = request()
        mismatched["model"] = {"id": "water-treatment", "version": "9.9.9"}
        _, responses = run_bridge([mismatched])
        self.assertEqual(responses[0]["status"], "error")
        self.assertEqual(responses[0]["error"], "model_mismatch")

    def test_invalid_json_is_a_protocol_error_and_the_process_continues(self):
        payload = "not json\n" + json.dumps(request()) + "\n"
        completed = subprocess.run(
            [sys.executable, str(BRIDGE)], input=payload, capture_output=True, text=True, timeout=30
        )
        responses = [json.loads(line) for line in completed.stdout.splitlines() if line.strip()]
        self.assertEqual(len(responses), 2)
        self.assertEqual(responses[0]["status"], "error")
        self.assertEqual(responses[0]["error"], "protocol_error")
        self.assertEqual(responses[1]["status"], "ok")

    def test_injected_failures_are_classified(self):
        _, responses = run_bridge([request(faults={"non_convergence": True})])
        self.assertEqual(responses[0]["error"], "non_convergence")
        _, responses = run_bridge([request(faults={"numerical_failure": True})])
        self.assertEqual(responses[0]["error"], "numerical_failure")

    def test_injected_unit_and_conservation_corruption_are_visible(self):
        _, responses = run_bridge([request(faults={"wrong_unit": True})])
        self.assertEqual(responses[0]["units"]["potable_flow_m3_h"], "L/s")
        _, responses = run_bridge([request(faults={"violate_conservation": True})])
        response = responses[0]
        self.assertNotAlmostEqual(
            response["potable_flow_m3_h"] + response["reject_flow_m3_h"], 10.0, places=1
        )

    def test_crash_fault_exits_nonzero_without_a_response(self):
        completed, responses = run_bridge([request(faults={"crash": True})], expect_exit=3)
        self.assertEqual(responses, [])
        self.assertEqual(completed.returncode, 3)


if __name__ == "__main__":
    unittest.main()
