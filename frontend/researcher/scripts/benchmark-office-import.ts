/** Synthetic parse-only benchmark: pnpm bench:office */
import { zipSync, strToU8 } from 'fflate';
import { performance } from 'node:perf_hooks';
import { readDocx } from '../src/lib/workspace/docx-import';
const paragraphs = 8000;
const body = Array.from(
  { length: paragraphs },
  (_, i) =>
    `<w:p><w:pPr><w:pStyle w:val="Body"/>${i % 20 === 0 ? '<w:pageBreakBefore/>' : ''}</w:pPr><w:r><w:t>Paragraph ${i}. A document revision keeps wording and formatting together. বাংলা العربية. </w:t></w:r><w:r><w:rPr><w:b/><w:color w:val="2059B0"/></w:rPr><w:t>Review this change.</w:t></w:r></w:p>`
).join('');
const file = zipSync({
  'word/document.xml': strToU8(
    `<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body>${body}</w:body></w:document>`
  ),
  'word/styles.xml': strToU8(
    '<w:styles xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:style w:styleId="Body"><w:rPr><w:rFonts w:ascii="Arial"/><w:sz w:val="22"/></w:rPr></w:style></w:styles>'
  )
});
const timings: number[] = [];
let blocks = 0,
  jsonBytes = 0;
for (let i = 0; i < 5; i++) {
  const start = performance.now();
  const result = readDocx(file);
  timings.push(performance.now() - start);
  blocks = result.blocks.length;
  jsonBytes = Buffer.byteLength(JSON.stringify(result));
}
const sorted = [...timings].sort((a, b) => a - b);
console.log(
  JSON.stringify(
    {
      kind: 'synthetic OOXML parse, not end-to-end import',
      paragraphs,
      explicitPageStarts: 400,
      zipBytes: file.length,
      blocks,
      jsonBytes,
      runsMs: timings.map((n) => Math.round(n)),
      medianMs: Math.round(sorted[2]),
      maxMs: Math.round(sorted[4]),
      rssBytes: process.memoryUsage().rss
    },
    null,
    2
  )
);
