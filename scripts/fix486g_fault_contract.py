"""Shared fail-closed contracts for FIX486G rejected Graph targets."""

FAULT_VALIDATION_CONTRACTS = {
    "wrong-parent": {
        "fault_setup": "graph_wrong_parent_overlay",
        "survivor_mode": "DIRECT",
        "forbidden_scope": "ANY",
        "expected_rejection_reason": "BINDING_INVALID",
        "provenance_behavior": "FORBIDDEN_TARGET_NO_GRAPH_CREDIT",
        "hard_gate_names": ("binding_invalid_graph_final_contexts",),
    },
    "binding-status": {
        "fault_setup": None,
        "survivor_mode": "DIRECT",
        "forbidden_scope": "ANY",
        "expected_rejection_reason": "VISIBILITY_REJECTED",
        "provenance_behavior": "FORBIDDEN_TARGET_NO_GRAPH_CREDIT",
        "hard_gate_names": ("inactive_deleted_expired_graph_final_contexts",),
    },
    "inactive-target": {
        "fault_setup": "graph_inactive_deleted_expired_overlay",
        "survivor_mode": "DIRECT",
        "forbidden_scope": "ANY",
        "expected_rejection_reason": "VISIBILITY_REJECTED",
        "provenance_behavior": "FORBIDDEN_TARGET_NO_GRAPH_CREDIT",
        "hard_gate_names": ("inactive_deleted_expired_graph_final_contexts",),
    },
    "deleted-target": {
        "fault_setup": None,
        "survivor_mode": "DIRECT",
        "forbidden_scope": "ANY",
        "expected_rejection_reason": "VISIBILITY_REJECTED",
        "provenance_behavior": "FORBIDDEN_TARGET_NO_GRAPH_CREDIT",
        "hard_gate_names": ("inactive_deleted_expired_graph_final_contexts",),
    },
    "expired-target": {
        "fault_setup": None,
        "survivor_mode": "DIRECT",
        "forbidden_scope": "ANY",
        "expected_rejection_reason": "VISIBILITY_REJECTED",
        "provenance_behavior": "FORBIDDEN_TARGET_NO_GRAPH_CREDIT",
        "hard_gate_names": ("inactive_deleted_expired_graph_final_contexts",),
    },
    "missing-parent": {
        "fault_setup": None,
        "survivor_mode": "DIRECT",
        "forbidden_scope": "ANY",
        "expected_rejection_reason": "BINDING_INVALID",
        "provenance_behavior": "FORBIDDEN_TARGET_NO_GRAPH_CREDIT",
        "hard_gate_names": ("binding_invalid_graph_final_contexts",),
    },
    "cross-zone": {
        "fault_setup": "graph_cross_zone_overlay",
        "survivor_mode": "DIRECT",
        "forbidden_scope": "ANY",
        "expected_rejection_reason": "GRAPH_ENDPOINT_ZONE_MISMATCH",
        "provenance_behavior": "FORBIDDEN_TARGET_NO_GRAPH_CREDIT",
        "hard_gate_names": ("cross_zone_graph_final_contexts", "forbidden_anchor_leaks"),
    },
    "hop-limit": {
        "fault_setup": "graph_second_hop_overlay",
        "survivor_mode": "GRAPH",
        "forbidden_scope": "GRAPH",
        "expected_rejection_reason": "HOP_LIMIT_REJECTED",
        "provenance_behavior": "FORBIDDEN_TARGET_NO_GRAPH_CREDIT",
        "hard_gate_names": ("hop_limit_violation_final_contexts", "second_hop_final_contexts"),
    },
    "cycle": {
        "fault_setup": "graph_cycle_overlay",
        "survivor_mode": "GRAPH",
        "forbidden_scope": "GRAPH",
        "expected_rejection_reason": "GRAPH_CYCLE_REJECTED",
        "provenance_behavior": "FORBIDDEN_TARGET_NO_GRAPH_CREDIT",
        "hard_gate_names": ("cycle_credit_inflation_events", "duplicate_graph_credit"),
    },
    "candidate-non-interference": {
        "fault_setup": None,
        "survivor_mode": "EITHER",
        "forbidden_scope": "ANY",
        "expected_rejection_reason": "BINDING_INVALID",
        "provenance_behavior": "FORBIDDEN_TARGET_NO_GRAPH_CREDIT",
        "hard_gate_names": ("valid_survivor_lost",),
    },
}

FAULT_CONTRACT_BY_SETUP = {
    contract["fault_setup"]: dict(contract, scenario=scenario)
    for scenario, contract in FAULT_VALIDATION_CONTRACTS.items()
    if contract["fault_setup"]
}
