//! The registry that lets the **on-target** runner execute this crate's unit
//! tests. Off unless `device-tests` is on.
//!
//! # Why this exists at all
//!
//! Every test in this crate used to be type-checked by `just lint-esp32` and
//! executed by **nothing**. `cargo test` cannot reach this crate: it names
//! `esp_idf_hal`, so it does not compile for a host target, and the host test
//! recipe (`just test`) lists only the portable crates. Two real device bugs
//! shipped through that gap — a provisioning password window that lasted zero
//! milliseconds, and console output lost across `esp_restart()` — each with a
//! test that had never been run.
//!
//! `#[test]` is a compiler builtin that **deletes** the function unless the
//! crate is being built with `--test`, so a non-test target cannot call these
//! functions. The two changes that make them callable are:
//!
//! * each test module is `#[cfg(any(test, feature = "device-tests"))] pub mod`
//!   instead of `#[cfg(test)] mod`, and
//! * each test is `#[cfg_attr(test, test)] pub fn` instead of `#[test] fn`.
//!
//! Both are inert unless the feature is on, so the shipped `firmware` binary is
//! byte-for-byte unaffected — `device-tests` is never enabled for it.
//!
//! # The list is checked, not trusted
//!
//! A hand-written list is exactly the kind of thing that goes stale silently,
//! so `scripts/device-test-audit.py` (`just test-audit`, which `just lint-esp32`
//! runs) fails the build if this list and the `#[cfg_attr(test, test)]` markers
//! in `src/*.rs` disagree in either direction. Adding a test and forgetting to
//! register it is a build failure, not a quietly skipped test.

use crate::{heap, mqtt, provisioning, task, telnet, time, web, wifi};

/// One registered unit test: the name the console shows, and the function to
/// call.
pub struct Case {
    /// `module::function`, stable across runs. The host runner keys its
    /// pass/fail accounting on this string, so it must not be pretty-printed.
    pub name: &'static str,
    /// The test body. It must not touch an actuator: this binary runs on the
    /// real machine.
    pub run: fn(),
}

/// Every unit test in `cc-hal-esp32`, in source order.
pub const CASES: &[Case] = &[
    Case {
        name: "heap::the_shed_floor_is_the_adrs_thirty_kilobytes",
        run: heap::tests::the_shed_floor_is_the_adrs_thirty_kilobytes,
    },
    Case {
        name: "mqtt::the_default_configuration_is_not_a_broker",
        run: mqtt::tests::the_default_configuration_is_not_a_broker,
    },
    Case {
        name: "mqtt::the_topic_layout_has_no_separator_the_cpp_does_not_have",
        run: mqtt::tests::the_topic_layout_has_no_separator_the_cpp_does_not_have,
    },
    Case {
        name: "mqtt::a_prefix_without_a_trailing_slash_reproduces_the_cpp_result",
        run: mqtt::tests::a_prefix_without_a_trailing_slash_reproduces_the_cpp_result,
    },
    Case {
        name: "mqtt::the_buffer_is_the_csqs_1024",
        run: mqtt::tests::the_buffer_is_the_csqs_1024,
    },
    Case {
        name: "mqtt::the_budget_is_the_csqs_ten_milliseconds",
        run: mqtt::tests::the_budget_is_the_csqs_ten_milliseconds,
    },
    Case {
        name: "mqtt::the_intervals_are_the_csqs",
        run: mqtt::tests::the_intervals_are_the_csqs,
    },
    Case {
        name: "mqtt::the_registry_puts_each_kind_of_topic_in_the_cpp_phase",
        run: mqtt::tests::the_registry_puts_each_kind_of_topic_in_the_cpp_phase,
    },
    Case {
        name: "mqtt::a_pressure_sensor_appears_only_when_it_is_fitted",
        run: mqtt::tests::a_pressure_sensor_appears_only_when_it_is_fitted,
    },
    Case {
        name: "mqtt::the_plan_slices_partition_the_view",
        run: mqtt::tests::the_plan_slices_partition_the_view,
    },
    Case {
        name: "mqtt::the_brew_guard_defaults_to_off_and_can_be_set",
        run: mqtt::tests::the_brew_guard_defaults_to_off_and_can_be_set,
    },
    Case {
        name: "mqtt::a_registry_never_names_a_credential",
        run: mqtt::tests::a_registry_never_names_a_credential,
    },
    Case {
        name: "mqtt::the_topics_string_names_the_base_so_a_bring_up_log_is_useful",
        run: mqtt::tests::the_topics_string_names_the_base_so_a_bring_up_log_is_useful,
    },
    Case {
        name: "provisioning::a_log_line_produces_no_reply_at_all",
        run: provisioning::tests::a_log_line_produces_no_reply_at_all,
    },
    Case {
        name: "provisioning::every_reply_is_prefixed_so_a_script_can_match_it",
        run: provisioning::tests::every_reply_is_prefixed_so_a_script_can_match_it,
    },
    Case {
        name: "provisioning::the_password_window_opens_and_closes",
        run: provisioning::tests::the_password_window_opens_and_closes,
    },
    Case {
        name: "provisioning::the_password_never_appears_in_a_reply",
        run: provisioning::tests::the_password_never_appears_in_a_reply,
    },
    Case {
        name: "provisioning::the_ssid_is_never_echoed_even_in_the_set_reply",
        run: provisioning::tests::the_ssid_is_never_echoed_even_in_the_set_reply,
    },
    Case {
        name: "provisioning::a_pending_credential_is_handed_over_not_printed",
        run: provisioning::tests::a_pending_credential_is_handed_over_not_printed,
    },
    Case {
        name: "provisioning::apply_asks_for_a_set_when_there_is_a_credential",
        run: provisioning::tests::apply_asks_for_a_set_when_there_is_a_credential,
    },
    Case {
        name: "provisioning::clear_then_apply_asks_for_the_clear_and_not_the_set",
        run: provisioning::tests::clear_then_apply_asks_for_the_clear_and_not_the_set,
    },
    Case {
        name: "provisioning::a_new_credential_supersedes_an_armed_clear",
        run: provisioning::tests::a_new_credential_supersedes_an_armed_clear,
    },
    Case {
        name: "provisioning::status_names_no_credential",
        run: provisioning::tests::status_names_no_credential,
    },
    Case {
        name: "provisioning::a_reader_assembles_lines_from_arbitrary_chunk_boundaries",
        run: provisioning::tests::a_reader_assembles_lines_from_arbitrary_chunk_boundaries,
    },
    Case {
        name: "provisioning::a_reader_discards_an_over_long_line_whole",
        run: provisioning::tests::a_reader_discards_an_over_long_line_whole,
    },
    Case {
        name: "provisioning::a_reader_strips_carriage_returns",
        run: provisioning::tests::a_reader_strips_carriage_returns,
    },
    Case {
        name: "provisioning::a_reader_keeps_a_passwords_own_spaces",
        run: provisioning::tests::a_reader_keeps_a_passwords_own_spaces,
    },
    Case {
        name: "provisioning::a_reader_drops_a_line_that_is_not_utf8",
        run: provisioning::tests::a_reader_drops_a_line_that_is_not_utf8,
    },
    Case {
        name: "task::a_command_carries_no_pointer",
        run: task::tests::a_command_carries_no_pointer,
    },
    Case {
        name: "telnet::the_port_and_banner_are_the_csqs",
        run: telnet::tests::the_port_and_banner_are_the_csqs,
    },
    Case {
        name: "telnet::the_line_buffer_is_the_adrs_256",
        run: telnet::tests::the_line_buffer_is_the_adrs_256,
    },
    Case {
        name: "telnet::the_heartbeat_is_the_csqs_thirty_seconds",
        run: telnet::tests::the_heartbeat_is_the_csqs_thirty_seconds,
    },
    Case {
        name: "telnet::a_line_buffer_splits_lines",
        run: telnet::tests::a_line_buffer_splits_lines,
    },
    Case {
        name: "telnet::a_crlf_pair_is_one_terminator_not_two",
        run: telnet::tests::a_crlf_pair_is_one_terminator_not_two,
    },
    Case {
        name: "telnet::an_over_long_line_is_flagged_rather_than_silently_split",
        run: telnet::tests::an_over_long_line_is_flagged_rather_than_silently_split,
    },
    Case {
        name: "telnet::a_partial_line_is_kept_for_the_next_chunk",
        run: telnet::tests::a_partial_line_is_kept_for_the_next_chunk,
    },
    Case {
        name: "telnet::a_bare_newline_is_not_a_line",
        run: telnet::tests::a_bare_newline_is_not_a_line,
    },
    Case {
        name: "telnet::a_shed_engages_below_the_floor_and_recovers_above_it",
        run: telnet::tests::a_shed_engages_below_the_floor_and_recovers_above_it,
    },
    Case {
        name: "telnet::a_heartbeat_is_due_once_per_interval",
        run: telnet::tests::a_heartbeat_is_due_once_per_interval,
    },
    Case {
        name: "telnet::pump_passes_lines_through_and_stops_at_end_of_stream",
        run: telnet::tests::pump_passes_lines_through_and_stops_at_end_of_stream,
    },
    Case {
        name: "telnet::the_stats_summary_names_the_floor",
        run: telnet::tests::the_stats_summary_names_the_floor,
    },
    Case {
        name: "time::the_clock_advances_and_never_goes_backwards",
        run: time::tests::the_clock_advances_and_never_goes_backwards,
    },
    Case {
        name: "time::the_microsecond_clock_is_finer_than_the_millisecond_one",
        run: time::tests::the_microsecond_clock_is_finer_than_the_millisecond_one,
    },
    Case {
        name: "web::the_status_payload_has_the_csqs_keys",
        run: web::tests::the_status_payload_has_the_csqs_keys,
    },
    Case {
        name: "web::an_absent_reading_is_null_and_never_a_fabricated_zero",
        run: web::tests::an_absent_reading_is_null_and_never_a_fabricated_zero,
    },
    Case {
        name: "web::a_present_reading_is_formatted_to_two_decimals",
        run: web::tests::a_present_reading_is_formatted_to_two_decimals,
    },
    Case {
        name: "web::the_status_payload_is_valid_json",
        run: web::tests::the_status_payload_is_valid_json,
    },
    Case {
        name: "web::the_temperatures_payload_is_the_csqs_three_keys",
        run: web::tests::the_temperatures_payload_is_the_csqs_three_keys,
    },
    Case {
        name: "web::the_health_payload_distinguishes_alive_from_published",
        run: web::tests::the_health_payload_distinguishes_alive_from_published,
    },
    Case {
        name: "web::nvs_debug_reports_the_blob_and_the_heap_and_no_parameters",
        run: web::tests::nvs_debug_reports_the_blob_and_the_heap_and_no_parameters,
    },
    Case {
        name: "web::an_empty_blob_describes_as_zeroes_rather_than_panicking",
        run: web::tests::an_empty_blob_describes_as_zeroes_rather_than_panicking,
    },
    Case {
        name: "web::every_csqs_api_route_is_registered",
        run: web::tests::every_csqs_api_route_is_registered,
    },
    Case {
        name: "web::the_route_table_fits_the_servers_handler_budget",
        run: web::tests::the_route_table_fits_the_servers_handler_budget,
    },
    Case {
        name: "web::the_redirect_and_the_ui_are_routes",
        run: web::tests::the_redirect_and_the_ui_are_routes,
    },
    Case {
        name: "web::the_sse_stream_is_a_route",
        run: web::tests::the_sse_stream_is_a_route,
    },
    Case {
        name: "web::an_sse_frame_ends_with_a_blank_line",
        run: web::tests::an_sse_frame_ends_with_a_blank_line,
    },
    Case {
        name: "web::a_keepalive_is_a_comment_and_carries_no_event",
        run: web::tests::a_keepalive_is_a_comment_and_carries_no_event,
    },
    Case {
        name: "web::the_default_sse_mode_is_the_chunked_one",
        run: web::tests::the_default_sse_mode_is_the_chunked_one,
    },
    Case {
        name: "web::the_client_cap_leaves_sockets_for_the_api",
        run: web::tests::the_client_cap_leaves_sockets_for_the_api,
    },
    Case {
        name: "web::a_fresh_stream_has_no_clients_and_no_counters",
        run: web::tests::a_fresh_stream_has_no_clients_and_no_counters,
    },
    Case {
        name: "web::a_push_counts_and_a_full_mailbox_counts_the_drop",
        run: web::tests::a_push_counts_and_a_full_mailbox_counts_the_drop,
    },
    Case {
        name: "web::the_broadcaster_cadence_fits_under_the_poll_interval",
        run: web::tests::the_broadcaster_cadence_fits_under_the_poll_interval,
    },
    Case {
        name: "web::the_keepalive_interval_is_under_the_nat_floor",
        run: web::tests::the_keepalive_interval_is_under_the_nat_floor,
    },
    Case {
        name: "web::the_large_response_floor_is_the_adrs_floor",
        run: web::tests::the_large_response_floor_is_the_adrs_floor,
    },
    Case {
        name: "web::the_large_response_limit_is_above_the_csqs_19kb",
        run: web::tests::the_large_response_limit_is_above_the_csqs_19kb,
    },
    Case {
        name: "web::the_shared_telemetry_round_trips",
        run: web::tests::the_shared_telemetry_round_trips,
    },
    Case {
        name: "web::the_parameters_body_carries_a_value_for_every_parameter",
        run: web::tests::the_parameters_body_carries_a_value_for_every_parameter,
    },
    Case {
        name: "web::a_parameter_value_is_typed_like_its_default",
        run: web::tests::a_parameter_value_is_typed_like_its_default,
    },
    Case {
        name: "web::a_set_parameter_reports_the_stored_value_not_the_default",
        run: web::tests::a_set_parameter_reports_the_stored_value_not_the_default,
    },
    Case {
        name: "web::the_radio_fields_survive_a_machine_publish",
        run: web::tests::the_radio_fields_survive_a_machine_publish,
    },
    Case {
        name: "web::a_default_snapshot_reports_no_radio_rather_than_a_fabricated_one",
        run: web::tests::a_default_snapshot_reports_no_radio_rather_than_a_fabricated_one,
    },
    Case {
        name: "web::the_status_body_reports_an_associated_radio",
        run: web::tests::the_status_body_reports_an_associated_radio,
    },
    Case {
        name: "web::a_reboot_request_is_one_shot",
        run: web::tests::a_reboot_request_is_one_shot,
    },
    Case {
        name: "web::the_ui_placeholder_says_why_it_is_empty",
        run: web::tests::the_ui_placeholder_says_why_it_is_empty,
    },
    Case {
        name: "web::an_unavailable_endpoint_names_the_task_that_owns_it",
        run: web::tests::an_unavailable_endpoint_names_the_task_that_owns_it,
    },
    Case {
        name: "wifi::the_connect_timeout_is_the_cpp_ten_seconds",
        run: wifi::tests::the_connect_timeout_is_the_cpp_ten_seconds,
    },
    Case {
        name: "wifi::the_monitor_poll_is_slower_than_the_control_tick",
        run: wifi::tests::the_monitor_poll_is_slower_than_the_control_tick,
    },
    Case {
        name: "wifi::an_over_long_ssid_is_refused_rather_than_truncated",
        run: wifi::tests::an_over_long_ssid_is_refused_rather_than_truncated,
    },
];
