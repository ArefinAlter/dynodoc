import { json } from '@sveltejs/kit';
import type { RequestHandler } from './$types';

/** Liveness probe for the researcher frontend (parity with the API's /healthz). */
export const GET: RequestHandler = () => {
  return json({ status: 'ok', service: 'researcher-frontend' });
};
