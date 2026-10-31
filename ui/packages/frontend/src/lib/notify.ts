import { toast } from "svelte-sonner";

export function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

export function reportError(error: unknown): void {
  toast.error(errorMessage(error));
}
