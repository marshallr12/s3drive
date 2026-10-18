use super::*;

fn limits() -> Limits {
    Limits {
        max_object_bytes: MAX_OBJECT_BYTES,
        min_nonfinal_part_bytes: 5 * 1024 * 1024,
        max_part_bytes: 5 * 1024 * 1024 * 1024,
        max_parts: MAX_PARTS,
    }
}

fn assert_coverage(plan: Plan) {
    let limits = plan.limits();
    assert!((1..=limits.max_parts).contains(&plan.part_count()));
    assert_eq!(plan.parts().len(), usize::from(plan.part_count()));
    assert_eq!(plan.part(0), None);
    assert_eq!(plan.part(plan.part_count() + 1), None);
    let mut offset = 0;
    for (index, part) in plan.parts().enumerate() {
        assert_eq!(part.number, u16::try_from(index + 1).unwrap());
        assert_eq!(plan.part(part.number), Some(part));
        assert_eq!(part.offset, offset);
        assert!((1..=limits.max_part_bytes).contains(&part.length));
        if part.number < plan.part_count() {
            assert!(part.length >= limits.min_nonfinal_part_bytes);
            assert_eq!(part.length, plan.part_bytes());
        }
        offset = offset.checked_add(part.length).unwrap();
    }
    assert_eq!(offset, plan.total_bytes());
}

#[test]
fn short_final_exact_multiple_and_tiny_nonempty_objects_cover_exact_bytes() {
    let min = limits().min_nonfinal_part_bytes;
    for size in [1, min - 1, min, min + 1, min * 2, min * 2 + 1] {
        let plan = Plan::new(size, min, limits()).unwrap();
        assert_eq!(plan.part_bytes(), min);
        assert_eq!(u64::from(plan.part_count()), size.div_ceil(min));
        assert_coverage(plan);
    }
    let plan = Plan::new(min + 1, 1, limits()).unwrap();
    assert_eq!(plan.part_count(), 2);
    assert_eq!(plan.part(2).unwrap().length, 1);
    assert_coverage(plan);
}

#[test]
fn count_pressure_raises_stride_without_relaxing_any_limit() {
    let limits = Limits {
        max_parts: 2,
        ..limits()
    };
    let min = limits.min_nonfinal_part_bytes;
    for (size, expected_stride) in [(min * 2, min), (min * 2 + 1, min + 1)] {
        let plan = Plan::new(size, min, limits).unwrap();
        assert_eq!(plan.part_bytes(), expected_stride);
        assert_eq!(plan.limits(), limits);
        assert_coverage(plan);
    }
    let impossible = Limits {
        max_part_bytes: min,
        ..limits
    };
    assert_eq!(
        Plan::new(min * 2 + 1, min, impossible),
        Err(Error::CannotFit)
    );
}

#[test]
fn largest_signed_length_and_unsigned_inputs_never_overflow_or_allocate_payloads() {
    let s3_shaped_limits = limits();
    let limits = Limits {
        max_object_bytes: MAX_OBJECT_BYTES,
        min_nonfinal_part_bytes: 1,
        max_part_bytes: MAX_OBJECT_BYTES,
        max_parts: MAX_PARTS,
    };
    for size in [MAX_OBJECT_BYTES - 1, MAX_OBJECT_BYTES] {
        let plan = Plan::new(size, 1, limits).unwrap();
        assert_eq!(plan.part_count(), MAX_PARTS);
        assert_coverage(plan);
        let one = Plan::new(size, MAX_OBJECT_BYTES, limits).unwrap();
        assert_eq!(one.part_count(), 1);
        assert_coverage(one);
    }
    assert_eq!(Plan::new(u64::MAX, 1, limits), Err(Error::ObjectTooLarge));
    assert_eq!(
        Plan::new(MAX_OBJECT_BYTES + 1, 1, limits),
        Err(Error::ObjectTooLarge)
    );
    assert_eq!(
        Plan::new(MAX_OBJECT_BYTES, 1, s3_shaped_limits),
        Err(Error::CannotFit)
    );
}

#[test]
fn exact_client_part_capacity_fits_but_one_more_byte_is_refused() {
    let limits = limits();
    let capacity = limits.max_part_bytes * u64::from(limits.max_parts);
    let plan = Plan::new(capacity, limits.min_nonfinal_part_bytes, limits).unwrap();
    assert_eq!(plan.part_count(), MAX_PARTS);
    assert_eq!(plan.part_bytes(), limits.max_part_bytes);
    assert_eq!(plan.part(MAX_PARTS).unwrap().length, limits.max_part_bytes);
    assert_coverage(plan);
    assert_eq!(
        Plan::new(capacity + 1, limits.min_nonfinal_part_bytes, limits),
        Err(Error::CannotFit)
    );
}

#[test]
fn malformed_limits_empty_objects_and_preferences_are_explicit_errors() {
    let good = limits();
    for bad in [
        Limits {
            max_object_bytes: 0,
            ..good
        },
        Limits {
            max_object_bytes: u64::MAX,
            ..good
        },
        Limits {
            min_nonfinal_part_bytes: 0,
            ..good
        },
        Limits {
            min_nonfinal_part_bytes: good.max_part_bytes + 1,
            ..good
        },
        Limits {
            max_part_bytes: u64::MAX,
            ..good
        },
        Limits {
            max_parts: 0,
            ..good
        },
        Limits {
            max_parts: MAX_PARTS + 1,
            ..good
        },
    ] {
        assert_eq!(Plan::new(1, 1, bad), Err(Error::InvalidLimits));
    }
    assert_eq!(Plan::new(0, 1, good), Err(Error::EmptyObject));
    for preferred in [0, good.max_part_bytes + 1, u64::MAX] {
        assert_eq!(
            Plan::new(1, preferred, good),
            Err(Error::InvalidPreferredSize)
        );
    }
    let small = Limits {
        max_object_bytes: 10,
        ..good
    };
    assert_eq!(Plan::new(11, 1, small), Err(Error::ObjectTooLarge));
    assert_coverage(Plan::new(10, 1, small).unwrap());
}

#[test]
fn exhaustive_small_geometries_match_an_independent_feasibility_oracle() {
    // Tiny synthetic limits are arithmetic controls, not provider profiles.
    for min in 1..=5 {
        for max in min..=9 {
            for count in 1..=5 {
                let limits = Limits {
                    max_object_bytes: 60,
                    min_nonfinal_part_bytes: min,
                    max_part_bytes: max,
                    max_parts: count,
                };
                for size in 1..=60 {
                    for preferred in 1..=max {
                        let result = Plan::new(size, preferred, limits);
                        // Enumerate all allowed strides, without using the
                        // planner's ceil/count calculation as the oracle.
                        let feasible = (min.max(preferred)..=max).any(|stride| {
                            (1..=count).any(|n| {
                                let preceding = u64::from(n - 1) * stride;
                                size > preceding && size - preceding <= stride
                            })
                        });
                        assert_eq!(result.is_ok(), feasible, "{size} {preferred} {limits:?}");
                        if let Ok(plan) = result {
                            assert_coverage(plan);
                        } else {
                            assert_eq!(result, Err(Error::CannotFit));
                        }
                    }
                }
            }
        }
    }
}
