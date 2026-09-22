import { readWorkbook } from './xlsx-import';

self.onmessage = async (event: MessageEvent<ArrayBuffer>) => {
  try {
    self.postMessage({ result: await readWorkbook(event.data) });
  } catch (error) {
    self.postMessage({ error: error instanceof Error ? error.message : 'XLSX conversion failed.' });
  }
};
