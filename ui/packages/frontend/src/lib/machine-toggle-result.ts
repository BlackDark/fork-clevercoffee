export interface MachineToggleResult {
  success: boolean;
  error?: string;
}

export async function parseToggleResponse(
  response: Response,
): Promise<MachineToggleResult> {
  if (response.ok) {
    return { success: true };
  }
  const data = (await response.json().catch(() => ({}))) as { error?: string };
  return { success: false, error: data.error };
}
