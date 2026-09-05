#!/usr/bin/env python3
"""Validate a Sloar ownership/evidence closure record.

This helper is intentionally repository-agnostic. It does not infer ownership
from source code or declare a product correct. It validates a caller-provided
closure record so missing owners, unsupported acceptance claims, stale gates,
and incomplete production convergence stay explicit.
"""
from __future__ import annotations

import argparse
import json
from pathlib import Path
from typing import Any


DORMANT_LIFECYCLES = {"dormant", "retired"}
PASS_RESULTS = {"pass", "passed", "green", "success"}


def _norm(value: Any) -> str:
    return str(value or "").strip().lower()


def _as_list(value: Any) -> list[Any]:
    return value if isinstance(value, list) else []


def evaluate(record: dict[str, Any]) -> dict[str, Any]:
    findings: list[dict[str, Any]] = []
    owners = _as_list(record.get("ownership"))
    claims = _as_list(record.get("claims"))
    evidence = _as_list(record.get("evidence"))
    gates = _as_list(record.get("gates"))
    features = {
        str(row.get("id")): row
        for row in _as_list(record.get("features"))
        if isinstance(row, dict) and row.get("id")
    }

    evidence_by_id = {
        str(row.get("id")): row
        for row in evidence
        if isinstance(row, dict) and row.get("id")
    }

    for row in owners:
        if not isinstance(row, dict):
            continue
        decision = str(row.get("decision") or "unknown")
        authoritative = row.get("authoritative_owner")
        writers = [str(x) for x in _as_list(row.get("writers")) if x]
        deciders = [str(x) for x in _as_list(row.get("independent_deciders")) if x]
        if not authoritative:
            findings.append({
                "code": "OWNER_UNKNOWN",
                "severity": "P0",
                "subject": decision,
                "message": "Consequential semantic decision has no authoritative owner.",
            })
        if len(set(deciders)) > 1:
            findings.append({
                "code": "OWNERSHIP_SPLIT",
                "severity": "P0",
                "subject": decision,
                "message": f"Multiple independent decision owners: {', '.join(sorted(set(deciders)))}",
            })
        if authoritative and writers and str(authoritative) not in writers:
            findings.append({
                "code": "OWNER_NOT_WRITER",
                "severity": "P1",
                "subject": decision,
                "message": "Declared authoritative owner is not listed among observed writers.",
            })

    claim_results: list[dict[str, Any]] = []
    for claim in claims:
        if not isinstance(claim, dict):
            continue
        claim_id = str(claim.get("id") or "unknown")
        required = {str(x) for x in _as_list(claim.get("requires")) if x}
        evidence_ids = [str(x) for x in _as_list(claim.get("evidence")) if x]
        covered: set[str] = set()
        usable: list[str] = []
        stale: list[str] = []
        blocked: list[str] = []

        target = str(claim.get("target") or "")
        for evidence_id in evidence_ids:
            row = evidence_by_id.get(evidence_id)
            if not row:
                continue
            result = _norm(row.get("result"))
            evidence_target = str(row.get("target") or "")
            if target and evidence_target and target != evidence_target:
                stale.append(evidence_id)
                continue
            if result in PASS_RESULTS:
                covered.update(str(x) for x in _as_list(row.get("covers")) if x)
                usable.append(evidence_id)
            elif result in {"blocked", "pending", "running", "red", "fail", "failed"}:
                blocked.append(evidence_id)

        missing = sorted(required - covered)
        status = "closed" if not missing and bool(usable or not required) else "open"
        if status == "open":
            findings.append({
                "code": "EVIDENCE_GAP",
                "severity": "P0",
                "subject": claim_id,
                "message": "Acceptance claim lacks matching passing evidence.",
                "missing": missing,
                "stale_evidence": stale,
                "blocked_evidence": blocked,
            })
        claim_results.append({
            "id": claim_id,
            "status": status,
            "required": sorted(required),
            "covered": sorted(covered),
            "usable_evidence": usable,
            "stale_evidence": stale,
            "blocked_evidence": blocked,
        })

    stale_gates: list[str] = []
    for gate in gates:
        if not isinstance(gate, dict):
            continue
        result = _norm(gate.get("result"))
        if result not in {"red", "fail", "failed", "blocked"}:
            continue
        feature_id = str(gate.get("feature") or "")
        feature = features.get(feature_id, {})
        lifecycle = _norm(feature.get("status"))
        affects = bool(gate.get("task_affects_feature", feature.get("task_affects_feature", False)))
        if lifecycle in DORMANT_LIFECYCLES and not affects:
            gate_id = str(gate.get("id") or feature_id or "unknown")
            stale_gates.append(gate_id)
            findings.append({
                "code": "STALE_GATE_SUSPECTED",
                "severity": "P1",
                "subject": gate_id,
                "message": f"Failing gate belongs to {lifecycle} feature outside the current change boundary.",
            })

    convergence = record.get("convergence")
    convergence_result = {"status": "not_required", "missing": []}
    if isinstance(convergence, dict):
        required = [str(x) for x in _as_list(convergence.get("required")) if x]
        observed_map = convergence.get("observed") if isinstance(convergence.get("observed"), dict) else {}
        observed = {str(key) for key, value in observed_map.items() if value}
        missing = [stage for stage in required if stage not in observed]
        convergence_result = {
            "status": "closed" if not missing else "open",
            "required": required,
            "observed": sorted(observed),
            "missing": missing,
        }
        if missing:
            findings.append({
                "code": "CONVERGENCE_GAP",
                "severity": "P0",
                "subject": "production",
                "message": "Publication/runtime convergence chain is incomplete.",
                "missing": missing,
            })

    p0 = sum(1 for item in findings if item["severity"] == "P0")
    p1 = sum(1 for item in findings if item["severity"] == "P1")
    status = "BLOCKED" if p0 else ("REVIEW" if p1 else "READY")

    return {
        "schema": 1,
        "status": status,
        "summary": {
            "p0": p0,
            "p1": p1,
            "claims_closed": sum(1 for row in claim_results if row["status"] == "closed"),
            "claims_total": len(claim_results),
            "stale_gates": len(stale_gates),
        },
        "claims": claim_results,
        "convergence": convergence_result,
        "findings": findings,
        "policy": {
            "blocked": "Do not claim completion or stack another symptom patch. Resolve P0 ownership/evidence/convergence gaps first.",
            "review": "Inspect lifecycle/gate relevance before changing product source.",
            "ready": "This record is structurally closed; repository-defined verification is still authoritative.",
        },
    }


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("record", type=Path)
    parser.add_argument("--json", action="store_true")
    args = parser.parse_args()

    data = json.loads(args.record.read_text(encoding="utf-8"))
    result = evaluate(data)

    if args.json:
        print(json.dumps(result, indent=2, ensure_ascii=False))
    else:
        print(f"Sloar engineering closure: {result['status']}")
        print(
            f"P0={result['summary']['p0']} "
            f"P1={result['summary']['p1']} "
            f"claims={result['summary']['claims_closed']}/{result['summary']['claims_total']}"
        )
        for finding in result["findings"]:
            print(f"- {finding['severity']} {finding['code']}: {finding['subject']}: {finding['message']}")
    return 0 if result["status"] == "READY" else 2


if __name__ == "__main__":
    raise SystemExit(main())
