/// <reference types="vite/client" />

interface ViteTypeOptions {
  strictImportMetaEnv: unknown;
}
interface ImportMetaEnv {
  readonly VITE_SERVER_ENDPOINT?: string;
  readonly VITE_BASE_PATH: string;
  readonly VITE_API_PATH?: string;
  readonly VITE_MOCK_MODE?: string;
}

interface ImportMeta {
  readonly env: ImportMetaEnv;
}
