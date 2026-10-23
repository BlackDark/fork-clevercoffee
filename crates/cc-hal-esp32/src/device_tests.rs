//! On-target test registry, enabled only by `device-tests`.
//!
//! `#[test]` functions disappear outside a test harness. Public functions marked
//! `#[cfg_attr(test, test)]` let the device runner call them without `cargo test`.
//! `just test-audit` checks registration in both directions; shipped firmware never
//! enables the feature.

use crate::{actuators, display, scale, switches, task, time};

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
        name: "actuators::the_water_valve_is_gated_on_an_empty_tank",
        run: actuators::tests::the_water_valve_is_gated_on_an_empty_tank,
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
        name: "actuators::the_water_valve_is_whitelist_gated_to_the_water_flow_states",
        run: actuators::tests::the_water_valve_is_whitelist_gated_to_the_water_flow_states,
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
        name: "display::the_refresh_interval_is_a_hundred_milliseconds",
        run: display::tests::the_refresh_interval_is_a_hundred_milliseconds,
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
        name: "scale::the_pins_match_the_board_map",
        run: scale::tests::the_pins_match_the_board_map,
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
        name: "task::only_one_staged_request_is_taken_per_tick",
        run: task::tests::only_one_staged_request_is_taken_per_tick,
    },
    Case {
        name: "task::note_refused_then_note_applied_acks_the_refusal",
        run: task::tests::note_refused_then_note_applied_acks_the_refusal,
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
        name: "time::the_clock_advances_and_never_goes_backwards",
        run: time::tests::the_clock_advances_and_never_goes_backwards,
    },
    Case {
        name: "time::the_microsecond_clock_is_finer_than_the_millisecond_one",
        run: time::tests::the_microsecond_clock_is_finer_than_the_millisecond_one,
    },
];
