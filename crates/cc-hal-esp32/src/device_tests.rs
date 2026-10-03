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

use crate::{
    actuators, display, heap, mqtt, provisioning, scale, switches, task, telnet, time, web, wifi,
};

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
        name: "actuators::a_latched_machine_may_energise_nothing",
        run: actuators::tests::a_latched_machine_may_energise_nothing,
    },
    Case {
        name: "actuators::an_empty_tank_stops_the_pump_and_the_water_valve_but_not_the_heater",
        run: actuators::tests::an_empty_tank_stops_the_pump_and_the_water_valve_but_not_the_heater,
    },
    Case {
        name: "actuators::the_water_valve_is_gated_on_an_empty_tank_which_the_cpp_does_not_do",
        run: actuators::tests::the_water_valve_is_gated_on_an_empty_tank_which_the_cpp_does_not_do,
    },
    Case {
        name: "actuators::the_steam_valve_is_whitelist_gated_to_steam_running",
        run: actuators::tests::the_steam_valve_is_whitelist_gated_to_steam_running,
    },
    Case {
        name: "actuators::an_inhibit_holds_its_own_actuator_and_nothing_else",
        run: actuators::tests::an_inhibit_holds_its_own_actuator_and_nothing_else,
    },
    Case {
        name: "actuators::a_healthy_interlock_permits_the_pump_the_valves_and_the_heater",
        run: actuators::tests::a_healthy_interlock_permits_the_pump_the_valves_and_the_heater,
    },
    Case {
        name: "actuators::the_valve_relay_is_off_only_when_both_valves_are_closed",
        run: actuators::tests::the_valve_relay_is_off_only_when_both_valves_are_closed,
    },
    Case {
        name: "actuators::an_empty_tank_at_boot_does_not_block_the_pump",
        run: actuators::tests::an_empty_tank_at_boot_does_not_block_the_pump,
    },
    Case {
        name: "display::the_geometry_is_a_128_by_64_page_buffer",
        run: display::tests::the_geometry_is_a_128_by_64_page_buffer,
    },
    Case {
        name: "display::the_addresses_are_the_datasheet_pair",
        run: display::tests::the_addresses_are_the_datasheet_pair,
    },
    Case {
        name: "display::the_refresh_interval_is_the_csqs_hundred_milliseconds",
        run: display::tests::the_refresh_interval_is_the_csqs_hundred_milliseconds,
    },
    Case {
        name: "display::a_frame_is_eight_writes_not_sixty_four",
        run: display::tests::a_frame_is_eight_writes_not_sixty_four,
    },
    Case {
        name: "display::the_init_sequence_is_u8g2s_ssd1306_noname_sequence",
        run: display::tests::the_init_sequence_is_u8g2s_ssd1306_noname_sequence,
    },
    Case {
        name: "display::the_init_sequence_remaps_segments_and_reverses_com",
        run: display::tests::the_init_sequence_remaps_segments_and_reverses_com,
    },
    Case {
        name: "display::the_init_sequence_sets_the_contrast_the_cpp_set",
        run: display::tests::the_init_sequence_sets_the_contrast_the_cpp_set,
    },
    Case {
        name: "display::the_panel_comes_up_in_horizontal_addressing_mode",
        run: display::tests::the_panel_comes_up_in_horizontal_addressing_mode,
    },
    Case {
        name: "display::bring_up_sends_u8g2s_sequence_then_the_display_on_it_appends",
        run: display::tests::bring_up_sends_u8g2s_sequence_then_the_display_on_it_appends,
    },
    Case {
        name: "display::a_flush_puts_the_whole_frame_on_the_wire_in_page_order",
        run: display::tests::a_flush_puts_the_whole_frame_on_the_wire_in_page_order,
    },
    Case {
        name: "display::power_save_blanks_the_panel_and_waking_restores_the_frame",
        run: display::tests::power_save_blanks_the_panel_and_waking_restores_the_frame,
    },
    Case {
        name: "display::the_refresh_gate_fires_every_hundred_milliseconds",
        run: display::tests::the_refresh_gate_fires_every_hundred_milliseconds,
    },
    Case {
        name: "display::the_refresh_gate_survives_the_49_day_millisecond_wrap",
        run: display::tests::the_refresh_gate_survives_the_49_day_millisecond_wrap,
    },
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
        name: "mqtt::an_inbound_topic_is_parsed_exactly_as_the_csqs_matcher_does",
        run: mqtt::tests::an_inbound_topic_is_parsed_exactly_as_the_csqs_matcher_does,
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
        name: "mqtt::the_interval_follows_the_machine_state",
        run: mqtt::tests::the_interval_follows_the_machine_state,
    },
    Case {
        name: "mqtt::the_registry_is_the_csqs_registration",
        run: mqtt::tests::the_registry_is_the_csqs_registration,
    },
    Case {
        name: "mqtt::an_inbound_reading_resolves_only_if_it_is_registered",
        run: mqtt::tests::an_inbound_reading_resolves_only_if_it_is_registered,
    },
    Case {
        name: "mqtt::the_four_specials_are_not_configuration_keys",
        run: mqtt::tests::the_four_specials_are_not_configuration_keys,
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
        name: "mqtt::the_weight_topics_are_exactly_the_ones_discovery_advertises",
        run: mqtt::tests::the_weight_topics_are_exactly_the_ones_discovery_advertises,
    },
    Case {
        name: "mqtt::the_plan_slices_partition_the_view",
        run: mqtt::tests::the_plan_slices_partition_the_view,
    },
    Case {
        name: "mqtt::every_registered_value_fits_the_payload_buffer",
        run: mqtt::tests::every_registered_value_fits_the_payload_buffer,
    },
    Case {
        name: "mqtt::the_shared_plan_helper_still_sees_the_whole_registry",
        run: mqtt::tests::the_shared_plan_helper_still_sees_the_whole_registry,
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
        name: "provisioning::the_argument_form_stages_the_credential_with_no_next_line",
        run: provisioning::tests::the_argument_form_stages_the_credential_with_no_next_line,
    },
    Case {
        name: "provisioning::the_next_line_form_still_works",
        run: provisioning::tests::the_next_line_form_still_works,
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
        name: "scale::the_pins_are_the_cpp_pin_map",
        run: scale::tests::the_pins_are_the_cpp_pin_map,
    },
    Case {
        name: "scale::the_sampler_stack_fits_the_datasets_it_carries",
        run: scale::tests::the_sampler_stack_fits_the_datasets_it_carries,
    },
    Case {
        name: "scale::a_fresh_telemetry_block_reports_no_weight",
        run: scale::tests::a_fresh_telemetry_block_reports_no_weight,
    },
    Case {
        name: "scale::a_weight_round_trips_through_the_shared_milligram_store",
        run: scale::tests::a_weight_round_trips_through_the_shared_milligram_store,
    },
    Case {
        name: "scale::a_non_finite_weight_is_never_published",
        run: scale::tests::a_non_finite_weight_is_never_published,
    },
    Case {
        name: "scale::an_unconnected_data_line_reports_not_ready_and_never_clocks",
        run: scale::tests::an_unconnected_data_line_reports_not_ready_and_never_clocks,
    },
    Case {
        name: "display::a_frame_does_not_re_send_the_init_sequence",
        run: display::tests::a_frame_does_not_re_send_the_init_sequence,
    },
    Case {
        name: "display::blanking_is_one_byte_and_does_not_reinitialise",
        run: display::tests::blanking_is_one_byte_and_does_not_reinitialise,
    },
    Case {
        name: "switches::an_absent_float_reports_the_tank_full_rather_than_empty",
        run: switches::tests::an_absent_float_reports_the_tank_full_rather_than_empty,
    },
    Case {
        name: "switches::a_fitted_float_reports_its_own_reading",
        run: switches::tests::a_fitted_float_reports_its_own_reading,
    },
    Case {
        name: "scale::the_signal_watchdog_reports_an_absent_cell_within_its_deadline",
        run: scale::tests::the_signal_watchdog_reports_an_absent_cell_within_its_deadline,
    },
    Case {
        name: "scale::a_command_queue_drops_rather_than_blocking_when_full",
        run: scale::tests::a_command_queue_drops_rather_than_blocking_when_full,
    },
    Case {
        name: "scale::the_configured_sample_count_rounds_down_to_a_power_of_two",
        run: scale::tests::the_configured_sample_count_rounds_down_to_a_power_of_two,
    },
    Case {
        name: "task::a_command_carries_no_pointer",
        run: task::tests::a_command_carries_no_pointer,
    },
    Case {
        name: "task::a_staged_parameter_request_reaches_the_control_task",
        run: task::tests::a_staged_parameter_request_reaches_the_control_task,
    },
    Case {
        name: "task::staged_requests_are_drained_in_order_and_all_at_once",
        run: task::tests::staged_requests_are_drained_in_order_and_all_at_once,
    },
    Case {
        name: "task::a_full_parameter_mailbox_refuses_rather_than_dropping",
        run: task::tests::a_full_parameter_mailbox_refuses_rather_than_dropping,
    },
    Case {
        name: "task::the_parameter_mailbox_prints_its_depth_and_never_its_contents",
        run: task::tests::the_parameter_mailbox_prints_its_depth_and_never_its_contents,
    },
    Case {
        name: "actuators::high_trigger_energises_high",
        run: actuators::tests::high_trigger_energises_high,
    },
    Case {
        name: "actuators::low_trigger_energises_low",
        run: actuators::tests::low_trigger_energises_low,
    },
    Case {
        name: "actuators::the_polarity_follows_the_configured_trigger",
        run: actuators::tests::the_polarity_follows_the_configured_trigger,
    },
    Case {
        name: "actuators::the_default_bundle_is_high_trigger_throughout",
        run: actuators::tests::the_default_bundle_is_high_trigger_throughout,
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
        name: "web::every_route_the_frontend_calls_is_registered",
        run: web::tests::every_route_the_frontend_calls_is_registered,
    },
    Case {
        name: "web::a_flag_reads_the_csqs_spellings_and_treats_anything_else_as_off",
        run: web::tests::a_flag_reads_the_csqs_spellings_and_treats_anything_else_as_off,
    },
    Case {
        name: "web::an_explicit_toggle_command_carries_the_value_it_was_given",
        run: web::tests::an_explicit_toggle_command_carries_the_value_it_was_given,
    },
    Case {
        name: "web::a_toggle_route_inverts_the_published_value",
        run: web::tests::a_toggle_route_inverts_the_published_value,
    },
    Case {
        name: "web::the_ota_status_document_satisfies_the_uis_schema",
        run: web::tests::the_ota_status_document_satisfies_the_uis_schema,
    },
    Case {
        name: "web::the_ota_status_document_is_not_an_update_error",
        run: web::tests::the_ota_status_document_is_not_an_update_error,
    },
    Case {
        name: "web::an_unavailable_ota_route_says_which_build_and_which_task",
        run: web::tests::an_unavailable_ota_route_says_which_build_and_which_task,
    },
    Case {
        name: "web::a_write_that_needs_a_reboot_is_named_rather_than_claimed_applied",
        run: web::tests::a_write_that_needs_a_reboot_is_named_rather_than_claimed_applied,
    },
    Case {
        name: "web::an_ordinary_parameter_is_not_reported_as_needing_a_reboot",
        run: web::tests::an_ordinary_parameter_is_not_reported_as_needing_a_reboot,
    },
    Case {
        name: "web::a_rejected_write_still_names_the_reboot_keys_it_did_accept",
        run: web::tests::a_rejected_write_still_names_the_reboot_keys_it_did_accept,
    },
    Case {
        name: "web::a_write_that_changed_nothing_needs_no_reboot",
        run: web::tests::a_write_that_changed_nothing_needs_no_reboot,
    },
    Case {
        name: "web::the_route_table_fits_the_servers_handler_budget",
        run: web::tests::the_route_table_fits_the_servers_handler_budget,
    },
    Case {
        name: "web::the_parameter_route_is_registered_for_both_methods",
        run: web::tests::the_parameter_route_is_registered_for_both_methods,
    },
    Case {
        name: "web::a_query_string_is_reachable_from_the_uri",
        run: web::tests::a_query_string_is_reachable_from_the_uri,
    },
    Case {
        name: "web::a_parameter_post_of_the_four_kinds_is_accepted",
        run: web::tests::a_parameter_post_of_the_four_kinds_is_accepted,
    },
    Case {
        name: "web::an_unknown_key_is_a_400_and_names_nothing_in_the_body",
        run: web::tests::an_unknown_key_is_a_400_and_names_nothing_in_the_body,
    },
    Case {
        name: "web::a_value_out_of_range_is_the_same_400_as_an_unknown_key",
        run: web::tests::a_value_out_of_range_is_the_same_400_as_an_unknown_key,
    },
    Case {
        name: "web::one_rejected_parameter_does_not_lose_the_accepted_ones",
        run: web::tests::one_rejected_parameter_does_not_lose_the_accepted_ones,
    },
    Case {
        name: "web::a_request_that_names_no_parameter_is_the_third_response",
        run: web::tests::a_request_that_names_no_parameter_is_the_third_response,
    },
    Case {
        name: "web::the_updated_response_is_the_cpp_body_verbatim",
        run: web::tests::the_updated_response_is_the_cpp_body_verbatim,
    },
    Case {
        name: "web::a_query_string_and_a_body_carry_the_same_parameter",
        run: web::tests::a_query_string_and_a_body_carry_the_same_parameter,
    },
    Case {
        name: "web::a_command_field_is_read_from_the_query_string_as_well_as_the_body",
        run: web::tests::a_command_field_is_read_from_the_query_string_as_well_as_the_body,
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
        name: "web::the_ui_shell_and_its_assets_are_embedded_and_gzipped",
        run: web::tests::the_ui_shell_and_its_assets_are_embedded_and_gzipped,
    },
    Case {
        name: "web::a_javascript_bundle_is_served_as_javascript",
        run: web::tests::a_javascript_bundle_is_served_as_javascript,
    },
    Case {
        name: "web::a_client_side_route_serves_the_shell_but_a_missing_asset_does_not",
        run: web::tests::a_client_side_route_serves_the_shell_but_a_missing_asset_does_not,
    },
    Case {
        name: "web::wildcard_matching_leaves_the_api_routes_exact",
        run: web::tests::wildcard_matching_leaves_the_api_routes_exact,
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
