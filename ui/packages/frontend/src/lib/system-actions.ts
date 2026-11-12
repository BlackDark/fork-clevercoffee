import * as m from "@/paraglide/messages.js";
import { apiFetch } from "./api-config";
import { ConfigUploadResponseSchema } from "./schemas";

const MAX_CONFIG_BYTES = 8 * 1024;

export type DeviceAction = "restart" | "factory-reset" | "wifi-reset";

/** Posts a restart or reset to the device; resolves with the message to show. */
export async function postDeviceAction(action: DeviceAction): Promise<string> {
  const response = await apiFetch(`/${action}`, {
    method: "POST",
    signal: AbortSignal.timeout(15000),
  });
  const text = await response.text();
  if (!response.ok)
    throw new Error(m.action_http_error({ status: response.status, text }));
  let result: unknown;
  try {
    result = JSON.parse(text);
  } catch {
    result = null;
  }
  if (
    result &&
    typeof result === "object" &&
    "success" in result &&
    result.success === false
  )
    throw new Error(m.action_device_refused());
  return m.action_request_accepted();
}

/** Uploads a settings backup; resolves with the message to show. */
export async function uploadConfig(file: File | null): Promise<string> {
  if (
    !file ||
    file.size === 0 ||
    file.size > MAX_CONFIG_BYTES ||
    !file.name.toLowerCase().endsWith(".json")
  )
    throw new Error(m.action_choose_json());
  const content = await file.text();
  let parsed: unknown;
  try {
    parsed = JSON.parse(content);
  } catch {
    throw new Error(m.action_config_invalid_json());
  }
  if (!parsed || typeof parsed !== "object" || Array.isArray(parsed))
    throw new Error(m.action_config_must_be_object());
  const response = await apiFetch("/config/upload", {
    method: "POST",
    body: content,
    signal: AbortSignal.timeout(30000),
  });
  const text = await response.text();
  if (!response.ok)
    throw new Error(m.action_http_error({ status: response.status, text }));
  const invalid = new Error(
    m.action_invalid_upload_response({ status: response.status }),
  );
  let payload: unknown;
  try {
    payload = JSON.parse(text);
  } catch {
    throw invalid;
  }
  const result = ConfigUploadResponseSchema.safeParse(payload);
  if (!result.success) throw invalid;
  if (!result.data.success)
    throw new Error(result.data.message || m.action_upload_failed());
  return result.data.message || m.action_config_restored();
}
