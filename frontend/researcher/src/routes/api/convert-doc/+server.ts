import { env } from '$env/dynamic/private';
import { json } from '@sveltejs/kit';
import { accessToken } from '$lib/server/session';
import type { RequestHandler } from './$types';
export const POST: RequestHandler = async ({ request, url, cookies }) => {
  if (request.headers.get('origin') !== url.origin)
    return json({ error: 'Invalid origin' }, { status: 403 });
  if (!(await accessToken(cookies, url.protocol === 'https:')))
    return json({ error: 'Please sign in' }, { status: 401 });
  if (!env.CONVERTER_URL || !env.CONVERTER_KEY)
    return json(
      {
        error:
          'Legacy Word conversion is not configured. Save the document as DOCX in Word or LibreOffice and upload that copy.'
      },
      { status: 503 }
    );
  const length = Number(request.headers.get('content-length'));
  if (length > 11 * 1024 * 1024)
    return json({ error: 'Use a file smaller than 10 MB.' }, { status: 413 });
  try {
    const form = await request.formData(),
      file = form.get('file');
    if (
      !(file instanceof File) ||
      file.size > 10 * 1024 * 1024 ||
      !file.name.toLowerCase().endsWith('.doc')
    )
      return json({ error: 'Choose a .doc file under 10 MB.' }, { status: 400 });
    const result = await fetch(env.CONVERTER_URL + '/convert', {
      method: 'POST',
      headers: { 'content-type': 'application/msword', 'x-converter-key': env.CONVERTER_KEY },
      body: await file.arrayBuffer(),
      signal: AbortSignal.timeout(50000)
    });
    if (!result.ok)
      return json(
        {
          error:
            result.status === 429
              ? 'The converter is busy. Please try again shortly.'
              : 'This Word file could not be converted. Open it in Word and save it as DOCX.'
        },
        { status: result.status }
      );
    return new Response(result.body, {
      headers: {
        'content-type': 'application/vnd.openxmlformats-officedocument.wordprocessingml.document',
        'cache-control': 'private, no-store'
      }
    });
  } catch {
    return json(
      { error: 'Conversion failed. Save as DOCX and try uploading again.' },
      { status: 422 }
    );
  }
};
