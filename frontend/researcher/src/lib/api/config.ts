/** API base URL, configurable per environment (`VITE_API_BASE_URL`). */
export const API_BASE_URL: string =
  (import.meta.env?.VITE_API_BASE_URL as string | undefined) ?? 'http://127.0.0.1:8080';
