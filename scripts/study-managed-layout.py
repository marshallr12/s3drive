#!/usr/bin/env python3
"""Offline arithmetic illustration, not a provider benchmark or format contract."""
import json


def study():
    size = 1024**3
    chunk = 1024**2
    saves = 100
    return {
        "research_date": "2026-10-18",
        "kind": "hypothetical arithmetic, not measured performance or pricing",
        "assumptions": {
            "file_bytes": size,
            "chunk_bytes": chunk,
            "patch_bytes": 4096,
            "saves_after_initial": saves,
            "all_generations_retained": True,
            "no_compression_deduplication_or_server_side_copy": True,
            "manifest_bytes_excluded": True,
            "aligned_patch_changes_one_chunk": True,
            "cross_boundary_patch_changes_two_chunks": True,
        },
        "whole_object": {
            "new_payload_bytes_per_save": size,
            "retained_payload_bytes": size * (saves + 1),
        },
        "chunked": {
            "initial_data_objects": size // chunk,
            "aligned_patch": {
                "new_data_objects_per_save": 1,
                "new_payload_bytes_per_save": chunk,
                "retained_payload_bytes": size + saves * chunk,
            },
            "cross_boundary_patch": {
                "new_data_objects_per_save": 2,
                "new_payload_bytes_per_save": 2 * chunk,
                "retained_payload_bytes": size + saves * 2 * chunk,
            },
            "flat_sha256_table_bytes_before_references_or_encoding": size // chunk * 32,
            "proposed_tree_fanout": 256,
            "leaf_pages": (size // chunk + 255) // 256,
            "root_pages": 1,
            "changed_manifest_pages_per_single_chunk_edit": 2,
        },
        "limitations": [
            "Not an implemented storage format, native file limit or upload permission",
            "No timing, request overhead, read amplification or account billing modeled",
            "Alternative multipart copy can reduce client upload but needs separate semantics and acceptance",
            "Worst-case edits rewrite all chunks; no universal saving is promised",
            "Current code does not collect retained generations",
        ],
    }


if __name__ == "__main__":
    print(json.dumps(study(), indent=2))
