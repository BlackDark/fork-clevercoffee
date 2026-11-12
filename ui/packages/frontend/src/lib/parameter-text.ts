// Parameter labels and help texts, English. Keyed by parameter name; the device
// sends neither.
import * as m from "@/paraglide/messages.js";
import { hardwareTexts } from "./parameter-text-hardware";

type Text = { label?: () => string; help?: () => string };

const texts: Record<string, Text> = {
  ...hardwareTexts,
  "pid.enabled": {
    label: m.param_pid_enabled_label,
    help: m.param_pid_enabled_help,
  },
  "pid.use_ponm": {
    label: m.param_pid_use_ponm_label,
    help: m.param_pid_use_ponm_help,
  },
  "pid.ema_factor": {
    label: m.param_pid_ema_factor_label,
    help: m.param_pid_ema_factor_help,
  },
  "pid.regular.kp": {
    label: m.param_pid_regular_kp_label,
    help: m.param_pid_regular_kp_help,
  },
  "pid.regular.tn": {
    label: m.param_pid_regular_tn_label,
    help: m.param_pid_regular_tn_help,
  },
  "pid.regular.tv": {
    label: m.param_pid_regular_tv_label,
    help: m.param_pid_regular_tv_help,
  },
  "pid.regular.i_max": {
    label: m.param_pid_regular_i_max_label,
    help: m.param_pid_regular_i_max_help,
  },
  "pid.steam.kp": {
    label: m.param_pid_steam_kp_label,
    help: m.param_pid_steam_kp_help,
  },
  "safety.emergency_temp": {
    label: m.param_safety_emergency_temp_label,
    help: m.param_safety_emergency_temp_help,
  },
  "safety.emergency_hysteresis": {
    label: m.param_safety_emergency_hysteresis_label,
    help: m.param_safety_emergency_hysteresis_help,
  },
  TEMP: { label: m.param_TEMP_label },
  "brew.setpoint": {
    label: m.param_brew_setpoint_label,
    help: m.param_brew_setpoint_help,
  },
  "brew.temp_offset": {
    label: m.param_brew_temp_offset_label,
    help: m.param_brew_temp_offset_help,
  },
  "steam.setpoint": {
    label: m.param_steam_setpoint_label,
    help: m.param_steam_setpoint_help,
  },
  "pid.bd.enabled": {
    label: m.param_pid_bd_enabled_label,
    help: m.param_pid_bd_enabled_help,
  },
  "brew.pid_delay": {
    label: m.param_brew_pid_delay_label,
    help: m.param_brew_pid_delay_help,
  },
  "pid.bd.kp": { label: m.param_pid_bd_kp_label, help: m.param_pid_bd_kp_help },
  "pid.bd.tn": { label: m.param_pid_bd_tn_label, help: m.param_pid_bd_tn_help },
  "pid.bd.tv": { label: m.param_pid_bd_tv_label, help: m.param_pid_bd_tv_help },
  "brew.mode": { label: m.param_brew_mode_label, help: m.param_brew_mode_help },
  "brew.by_time.enabled": {
    label: m.param_brew_by_time_enabled_label,
    help: m.param_brew_by_time_enabled_help,
  },
  "brew.by_time.target_time": {
    label: m.param_brew_by_time_target_time_label,
    help: m.param_brew_by_time_target_time_help,
  },
  "brew.by_weight.enabled": {
    label: m.param_brew_by_weight_enabled_label,
    help: m.param_brew_by_weight_enabled_help,
  },
  "brew.by_weight.target_weight": {
    label: m.param_brew_by_weight_target_weight_label,
    help: m.param_brew_by_weight_target_weight_help,
  },
  "brew.by_weight.auto_tare": {
    label: m.param_brew_by_weight_auto_tare_label,
    help: m.param_brew_by_weight_auto_tare_help,
  },
  "brew.pre_infusion.enabled": {
    label: m.param_brew_pre_infusion_enabled_label,
    help: m.param_brew_pre_infusion_enabled_help,
  },
  "brew.pre_infusion.time": {
    label: m.param_brew_pre_infusion_time_label,
    help: m.param_brew_pre_infusion_time_help,
  },
  "brew.pre_infusion.pause": {
    label: m.param_brew_pre_infusion_pause_label,
    help: m.param_brew_pre_infusion_pause_help,
  },
  "display.template": {
    label: m.param_display_template_label,
    help: m.param_display_template_help,
  },
  "display.inverted": {
    label: m.param_display_inverted_label,
    help: m.param_display_inverted_help,
  },
  "display.language": {
    label: m.param_display_language_label,
    help: m.param_display_language_help,
  },
  "display.blinking.delta": {
    label: m.param_display_blinking_delta_label,
    help: m.param_display_blinking_delta_help,
  },
  "display.fullscreen_brew_timer": {
    label: m.param_display_fullscreen_brew_timer_label,
    help: m.param_display_fullscreen_brew_timer_help,
  },
  "display.fullscreen_manual_flush_timer": {
    label: m.param_display_fullscreen_manual_flush_timer_label,
    help: m.param_display_fullscreen_manual_flush_timer_help,
  },
  "display.fullscreen_hot_water_timer": {
    label: m.param_display_fullscreen_hot_water_timer_label,
    help: m.param_display_fullscreen_hot_water_timer_help,
  },
  "display.post_brew_timer_duration": {
    label: m.param_display_post_brew_timer_duration_label,
    help: m.param_display_post_brew_timer_duration_help,
  },
  "display.heating_logo": {
    label: m.param_display_heating_logo_label,
    help: m.param_display_heating_logo_help,
  },
  "display.pid_off_logo": {
    label: m.param_display_pid_off_logo_label,
    help: m.param_display_pid_off_logo_help,
  },
  "backflush.cycles": {
    label: m.param_backflush_cycles_label,
    help: m.param_backflush_cycles_help,
  },
  "backflush.fill_time": {
    label: m.param_backflush_fill_time_label,
    help: m.param_backflush_fill_time_help,
  },
  "backflush.flush_time": {
    label: m.param_backflush_flush_time_label,
    help: m.param_backflush_flush_time_help,
  },
  "maintenance.backflush_reminder.enabled": {
    label: m.param_maintenance_backflush_reminder_enabled_label,
    help: m.param_maintenance_backflush_reminder_enabled_help,
  },
  "maintenance.backflush_reminder.threshold": {
    label: m.param_maintenance_backflush_reminder_threshold_label,
    help: m.param_maintenance_backflush_reminder_threshold_help,
  },
  "standby.enabled": {
    label: m.param_standby_enabled_label,
    help: m.param_standby_enabled_help,
  },
  "standby.time": {
    label: m.param_standby_time_label,
    help: m.param_standby_time_help,
  },
  "mqtt.enabled": {
    label: m.param_mqtt_enabled_label,
    help: m.param_mqtt_enabled_help,
  },
  "mqtt.broker": {
    label: m.param_mqtt_broker_label,
    help: m.param_mqtt_broker_help,
  },
  "mqtt.port": { label: m.param_mqtt_port_label, help: m.param_mqtt_port_help },
  "mqtt.username": {
    label: m.param_mqtt_username_label,
    help: m.param_mqtt_username_help,
  },
  "mqtt.password": {
    label: m.param_mqtt_password_label,
    help: m.param_mqtt_password_help,
  },
  "mqtt.topic": {
    label: m.param_mqtt_topic_label,
    help: m.param_mqtt_topic_help,
  },
  "mqtt.hassio.enabled": {
    label: m.param_mqtt_hassio_enabled_label,
    help: m.param_mqtt_hassio_enabled_help,
  },
  "mqtt.hassio.prefix": {
    label: m.param_mqtt_hassio_prefix_label,
    help: m.param_mqtt_hassio_prefix_help,
  },
  "system.wifi.ssid": {
    label: m.param_system_wifi_ssid_label,
    help: m.param_system_wifi_ssid_help,
  },
  "system.wifi.password": {
    label: m.param_system_wifi_password_label,
    help: m.param_system_wifi_password_help,
  },
  "system.offline_mode": {
    label: m.param_system_offline_mode_label,
    help: m.param_system_offline_mode_help,
  },
  "system.hostname": {
    label: m.param_system_hostname_label,
    help: m.param_system_hostname_help,
  },
  "system.ota_password": {
    label: m.param_system_ota_password_label,
    help: m.param_system_ota_password_help,
  },
  "system.log_level": {
    label: m.param_system_log_level_label,
    help: m.param_system_log_level_help,
  },
  "system.timing_debug.enabled": {
    label: m.param_system_timing_debug_enabled_label,
    help: m.param_system_timing_debug_enabled_help,
  },
  "system.showdisplay.enabled": {
    label: m.param_system_showdisplay_enabled_label,
    help: m.param_system_showdisplay_enabled_help,
  },
  "system.auth.enabled": {
    label: m.param_system_auth_enabled_label,
    help: m.param_system_auth_enabled_help,
  },
  "system.auth.username": {
    label: m.param_system_auth_username_label,
    help: m.param_system_auth_username_help,
  },
  "system.auth.password": {
    label: m.param_system_auth_password_label,
    help: m.param_system_auth_password_help,
  },
  STEAM_MODE: {
    label: m.param_STEAM_MODE_label,
    help: m.param_STEAM_MODE_help,
  },
  BACKFLUSH_ON: {
    label: m.param_BACKFLUSH_ON_label,
    help: m.param_BACKFLUSH_ON_help,
  },
  TARE_ON: { label: m.param_TARE_ON_label, help: m.param_TARE_ON_help },
  CALIBRATION_ON: {
    label: m.param_CALIBRATION_ON_label,
    help: m.param_CALIBRATION_ON_help,
  },
  VERSION: { label: m.param_VERSION_label, help: m.param_VERSION_help },
};

/** The display label, or the parameter name when none is known. */
export function getParameterLabel(name: string): string {
  return texts[name]?.label?.() ?? name;
}

/** The help text (may contain links), or undefined. */
export function getParameterHelp(name: string): string | undefined {
  return texts[name]?.help?.();
}
