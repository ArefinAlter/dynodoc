import { error, redirect } from '@sveltejs/kit';
import type { RequestHandler } from './$types';
import { getConfig } from '$lib/server/config';
import { SAMPLE_WORKSPACE_URL } from '$lib/site/release';

// The desktop client's scripted copy used to be published here. The pre-alpha now runs on a real
// server: a visitor gets a sample workspace of their own in the real web workspace, and the desktop
// app itself is a download (see /try). Only the hosted marketing site answers; a self-hosted
// installation has no such page.
export const GET: RequestHandler = () => {
  if (!getConfig().marketing) error(404, 'Not found');
  redirect(308, SAMPLE_WORKSPACE_URL);
};
