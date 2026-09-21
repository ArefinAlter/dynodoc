import { readDocx } from './docx-import';
self.onmessage = (event: MessageEvent<ArrayBuffer>) => {
  try {
    self.postMessage({ result: readDocx(new Uint8Array(event.data)) });
  } catch (error) {
    self.postMessage({ error: error instanceof Error ? error.message : 'DOCX conversion failed.' });
  }
};
