# Core selection-count verification

Command (project working directory):
```
CARGO_HOME="$PWD/.build-tools/cargo" RUSTUP_HOME="$PWD/.build-tools/rustup" RUSTUP_TOOLCHAIN=stable "$PWD/.build-tools/cargo/bin/cargo" test -p macro-core --test selection_count --offline --locked
```

Final source execution: exit 0; 15 tests passed. Output follows.

```

running 15 tests
test cancelled_undelivered_seat_is_not_counted_and_old_ack_is_inert ... ok
test automatic_refresh_needs_fresh_zero_and_never_discards_preexisting_or_pending_selection ... ok
test duplicate_observation_cannot_reauthorize_after_delay ... ok
test delayed_next_input_requires_another_fresh_observation_not_tick ... ok
test legacy_navigation_is_preserved_but_initial_selection_is_read_before_it ... ok
test initial_target_completes_without_seat_input_and_excess_count_holds ... ok
test known_failure_reconciles_count_and_keeps_other_delivered_seat ... ok
test each_delivery_requires_exact_increment_and_final_fresh_target_before_completion ... ok
test late_known_failure_during_next_pending_effect_requires_adjusted_total ... ok
test decrease_jump_and_unreadable_do_not_restore_reclick_or_release_pending_effect ... ok
test normal_entry_and_captcha_order_are_unchanged_by_count_option ... ok
test request_rotation_blocks_old_context_refresh_and_run_results ... ok
test unreadable_observation_invalidates_verified_status_without_inventing_zero_or_erasing_pending_effect ... ok
test unknown_baseline_never_clicks_and_initial_total_is_not_an_additional_target ... ok
test options_default_off_and_existing_profile_roundtrips ... ok

test result: ok. 15 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s


```

Earlier affected baseline: `cargo test -p macro-core --test engine --offline --locked`, exit 0; 102 tests passed. Subsequent changes only add SelectionCountUnavailable and constrain the count-pending UI flag to active count phases; parent workspace verification covers their integration. No app/release build, OS input, live site, or browser was executed.

