// The editor is fully client-driven (live SSE store + browser fetch), so disable SSR
// for this route. The document list (`/`) still server-renders its shell.
export const ssr = false;
