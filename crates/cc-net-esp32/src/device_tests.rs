//! On-target test registry, enabled only by `device-tests`; the same scheme as
//! `cc_hal_esp32::device_tests`, whose `Case` it reuses.

use cc_hal_esp32::device_tests::Case;

use crate::{ota, provisioning, telnet, web, wifi};

/// Every unit test in `cc-net-esp32`, in source order.
pub const CASES: &[Case] = &[
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
        name: "telnet::the_port_and_banner_are_pinned",
        run: telnet::tests::the_port_and_banner_are_pinned,
    },
    Case {
        name: "telnet::the_heartbeat_is_thirty_seconds",
        run: telnet::tests::the_heartbeat_is_thirty_seconds,
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
        name: "telnet::the_stats_summary_names_the_floor",
        run: telnet::tests::the_stats_summary_names_the_floor,
    },
    Case {
        name: "telnet::the_hal_reexports_the_same_floor_the_portable_shed_uses",
        run: telnet::tests::the_hal_reexports_the_same_floor_the_portable_shed_uses,
    },
    Case {
        name: "telnet::a_fresh_server_has_no_client",
        run: telnet::tests::a_fresh_server_has_no_client,
    },
    Case {
        name: "telnet::the_task_priority_is_below_control_and_the_stack_is_four_k",
        run: telnet::tests::the_task_priority_is_below_control_and_the_stack_is_four_k,
    },
    Case {
        name: "telnet::the_fanout_is_one_filter_over_two_sinks",
        run: telnet::tests::the_fanout_is_one_filter_over_two_sinks,
    },
    Case {
        name: "web::the_config_upload_route_is_registered",
        run: web::tests::the_config_upload_route_is_registered,
    },
    Case {
        name: "web::the_advertised_options_handler_is_a_wildcard_over_the_api",
        run: web::tests::the_advertised_options_handler_is_a_wildcard_over_the_api,
    },
    Case {
        name: "web::every_api_route_is_registered",
        run: web::tests::every_api_route_is_registered,
    },
    Case {
        name: "web::every_route_the_frontend_calls_is_registered",
        run: web::tests::every_route_the_frontend_calls_is_registered,
    },
    Case {
        name: "web::every_advertised_api_route_is_covered_by_the_json_404",
        run: web::tests::every_advertised_api_route_is_covered_by_the_json_404,
    },
    Case {
        name: "web::the_parameter_help_route_is_a_get_and_nothing_else",
        run: web::tests::the_parameter_help_route_is_a_get_and_nothing_else,
    },
    Case {
        name: "web::every_registered_route_is_in_the_raw_handlers_own_table",
        run: web::tests::every_registered_route_is_in_the_raw_handlers_own_table,
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
        name: "web::the_large_response_limit_is_above_19kb",
        run: web::tests::the_large_response_limit_is_above_19kb,
    },
    Case {
        name: "web::the_shared_telemetry_round_trips",
        run: web::tests::the_shared_telemetry_round_trips,
    },
    Case {
        name: "web::a_read_does_not_consume_the_snapshot",
        run: web::tests::a_read_does_not_consume_the_snapshot,
    },
    Case {
        name: "web::the_radio_fields_survive_a_machine_publish",
        run: web::tests::the_radio_fields_survive_a_machine_publish,
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
        name: "web::a_client_side_route_serves_the_shell_but_a_missing_asset_does_not",
        run: web::tests::a_client_side_route_serves_the_shell_but_a_missing_asset_does_not,
    },
    Case {
        name: "web::wildcard_matching_leaves_the_api_routes_exact",
        run: web::tests::wildcard_matching_leaves_the_api_routes_exact,
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
    Case {
        name: "ota::a_session_refuses_a_second_claim_while_one_is_running",
        run: ota::tests::a_session_refuses_a_second_claim_while_one_is_running,
    },
    Case {
        name: "ota::a_finished_update_asks_for_exactly_one_restart",
        run: ota::tests::a_finished_update_asks_for_exactly_one_restart,
    },
    Case {
        name: "ota::a_failure_does_not_ask_for_a_restart",
        run: ota::tests::a_failure_does_not_ask_for_a_restart,
    },
    Case {
        name: "ota::a_claim_starts_with_no_verdict_and_a_verdict_is_read_once",
        run: ota::tests::a_claim_starts_with_no_verdict_and_a_verdict_is_read_once,
    },
    Case {
        name: "ota::a_refusal_keeps_its_reason_across_the_task_boundary",
        run: ota::tests::a_refusal_keeps_its_reason_across_the_task_boundary,
    },
    Case {
        name: "ota::the_status_reports_progress_as_bytes_arrive",
        run: ota::tests::the_status_reports_progress_as_bytes_arrive,
    },
    Case {
        name: "ota::a_filesystem_upload_is_scaled_against_the_filesystem_floor",
        run: ota::tests::a_filesystem_upload_is_scaled_against_the_filesystem_floor,
    },
];
