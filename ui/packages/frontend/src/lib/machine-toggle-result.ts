/** The outcome of a machine toggle: did it work, and what state is it in now? */
export interface MachineToggleResult {
  success: boolean;
  error?: string;
  /**
   * The state the **device** reports after the toggle, when it reports one.
   *
   * `/api/pid` answers `{"success":true,"pidEnabled":<bool>}` and `/api/steam`
   * answers `{"success":true,"steamMode":<bool>}` — the firmware tells the
   * caller what it actually did, which is not always the inverse of what it was
   * before. `POST /api/pid` while the machine is in a state where the runtime PID
   * flag is forced off is the obvious case: the preference is stored and the
   * device answers with the runtime state, and a UI that assumed "it flipped"
   * would show the wrong thing until the next poll.
   *
   * `undefined` when the device did not say — the caller then has no better
   * information than "it did not fail", and should refetch rather than guess.
   */
  value?: boolean;
}

/**
 * Read a toggle response.
 *
 * `valueKey` names the body field carrying the new state. When the body is
 * missing, unreadable, or does not carry that field, `value` is left `undefined`
 * rather than guessed: a toggle that assumes it flipped is how a switch ends up
 * showing the wrong thing until a refresh.
 */
export async function parseToggleResponse(
  response: Response,
  valueKey?: string,
): Promise<MachineToggleResult> {
  if (!response.ok) {
    const data = (await response.json().catch(() => ({}))) as {
      error?: string;
    };
    return { success: false, error: data.error };
  }
  if (valueKey === undefined) {
    // Nothing to read and the request worked: the caller knows only that it did
    // not fail.
    return { success: true };
  }
  const data = (await response.json().catch(() => null)) as Record<
    string,
    unknown
  > | null;
  const raw = data?.[valueKey];
  if (typeof raw === "boolean") {
    return { success: true, value: raw };
  }
  if (typeof raw === "number") {
    return { success: true, value: raw !== 0 };
  }
  return { success: true };
}
