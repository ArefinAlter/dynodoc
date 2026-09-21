import { createServer } from 'vite';
const server = await createServer({ server: { middlewareMode: true }, appType: 'custom' });
try {
  await server.ssrLoadModule('/scripts/benchmark-office-import.ts');
} finally {
  await server.close();
}
