import type { JSONContent } from '@tiptap/core';
import type { DocumentState } from '$lib/api/types';
import { download, rootOp, itemOp, paragraph, type ImportResult } from './files';
import { ulid } from 'ulid';
export function latexEscape(text: string) {
  return text.replace(
    /[\\{}$&#%_^~]/g,
    (c) =>
      ({
        '\\': '\\textbackslash{}',
        '{': '\\{',
        '}': '\\}',
        $: '\\$',
        '&': '\\&',
        '#': '\\#',
        '%': '\\%',
        _: '\\_',
        '^': '\\textasciicircum{}',
        '~': '\\textasciitilde{}'
      })[c]!
  );
}
export function toLatex(block: JSONContent): string {
  if (block.type === 'inlineMath') return `$${String(block.attrs?.latex || '')}$`;
  if (block.type === 'blockMath') return `\\[${String(block.attrs?.latex || '')}\\]\n`;
  if (block.type === 'text') {
    let text = latexEscape(block.text || '');
    for (const mark of block.marks || []) {
      if (mark.type === 'bold') text = `\\textbf{${text}}`;
      if (mark.type === 'italic') text = `\\textit{${text}}`;
      if (mark.type === 'underline') text = `\\underline{${text}}`;
      if (mark.type === 'superscript') text = `\\textsuperscript{${text}}`;
      if (mark.type === 'subscript') text = `\\textsubscript{${text}}`;
    }
    return text;
  }
  const children = () => (block.content || []).map(toLatex).join('');
  switch (block.type) {
    case 'heading':
      return `\\${['section', 'subsection', 'subsubsection', 'paragraph', 'subparagraph', 'subparagraph'][Math.min(5, Math.max(0, Number(block.attrs?.level || 1) - 1))]}{${children()}}\n`;
    case 'paragraph':
      return children() + '\n\n';
    case 'hardBreak':
      return '\\\\\n';
    case 'bulletList':
    case 'taskList':
      return '\\begin{itemize}\n' + children() + '\\end{itemize}\n';
    case 'orderedList':
      return '\\begin{enumerate}\n' + children() + '\\end{enumerate}\n';
    case 'listItem':
    case 'taskItem':
      return '\\item ' + children();
    case 'blockquote':
      return '\\begin{quote}\n' + children() + '\\end{quote}\n';
    case 'codeBlock':
      return (
        '\\begin{quote}\\ttfamily ' +
        latexEscape((block.content || []).map((x) => x.text || '').join('')) +
        '\\end{quote}\n'
      );
    case 'table': {
      const width = block.content?.[0]?.content?.length || 1;
      return (
        `\\begin{longtable}{${'l'.repeat(width)}}\n` +
        (block.content || [])
          .map(
            (row) =>
              (row.content || [])
                .map((cell) => (cell.content || []).map(toLatex).join('').trim())
                .join(' & ') + ' \\\\\n'
          )
          .join('') +
        '\\end{longtable}\n'
      );
    }
    case 'image':
      return `\\textit{[Image: ${latexEscape(String(block.attrs?.alt || block.attrs?.src || ''))}]}\n`;
    case 'horizontalRule':
      return '\\par\\noindent\\rule{\\linewidth}{0.4pt}\n';
    default:
      return children();
  }
}
export function latexDocument(state: DocumentState, title: string) {
  const live = Object.values(state.nodes).filter((n) => !n.deleted);
  const root = live.find((n) => n.type === 'form');
  const tabs = live
    .filter((n) => n.type === 'section' && n.current_fields.kind === 'document_tab')
    .sort((a, b) => a.pos.localeCompare(b.pos));
  const body = [root, ...tabs]
    .filter((n) => !!n)
    .map(
      (parent) =>
        (parent!.type === 'section'
          ? `\\section*{${latexEscape(String(parent!.current_fields.label || ''))}}\n`
          : '') +
        live
          .filter((n) => n.type === 'item' && n.parent_id === parent!.id)
          .sort((a, b) => a.pos.localeCompare(b.pos))
          .map((n) =>
            toLatex(
              (n.current_fields.content as JSONContent) ||
                paragraph(String(n.current_fields.label || ''))
            )
          )
          .join('')
    )
    .join('\n');
  return `\\documentclass[11pt]{article}\n\\usepackage{fontspec}\n\\usepackage{amsmath,amssymb,longtable,geometry}\n\\geometry{margin=1in}\n\\title{${latexEscape(title)}}\n\\author{}\n\\date{}\n\\begin{document}\n\\maketitle\n${body}\n\\end{document}\n`;
}
export async function exportLatex(state: DocumentState, title: string, zip = false) {
  const source = latexDocument(state, title);
  if (!zip) {
    download(source, title + '.tex', 'application/x-tex');
    return;
  }
  const { default: JSZip } = await import('jszip');
  const archive = new JSZip();
  archive.file('main.tex', source);
  archive.file(
    'dynodoc-content.json',
    JSON.stringify({ format: 'dynodoc-content-v1', title, kind: 'document', state }, null, 2)
  );
  archive.file(
    'README.txt',
    'Upload this ZIP to Overleaf as a new project. Select LuaLaTeX or XeLaTeX (fontspec). The main.tex file contains document tabs, headings, lists, tables, basic text marks and equations. Images are labelled placeholders; attach media and adjust layout in Overleaf. Advanced Word page layout and custom LaTeX macros are not round-tripped. dynodoc-content.json retains the original editable block structure.\n'
  );
  download(
    await archive.generateAsync({ type: 'blob' }),
    title + '-overleaf.zip',
    'application/zip'
  );
}
export function importLatex(source: string, filename: string): ImportResult {
  const rootId = ulid();
  const warnings = [
    'LaTeX import supports plain paragraphs, section headings and display equations. Other LaTeX is retained as editable source blocks, never executed. Review formatting and keep the original upload for full fidelity.'
  ];
  const body = source.match(/\\begin\{document\}([\s\S]*?)\\end\{document\}/)?.[1] ?? source;
  const parts = body
    .split(
      /(\\(?:subsubsection|subsection|section)\*?\{[^{}]*\}|\\\[[\s\S]*?\\\]|\$\$[\s\S]*?\$\$|\\begin\{equation\*?\}[\s\S]*?\\end\{equation\*?\})/
    )
    .filter((x) => x.trim());
  const blocks: JSONContent[] = [];
  for (const part of parts) {
    const heading = part.match(/^\\(subsubsection|subsection|section)\*?\{([^{}]*)\}$/);
    if (heading) {
      blocks.push({
        type: 'heading',
        attrs: { level: ['section', 'subsection', 'subsubsection'].indexOf(heading[1]) + 1 },
        content: [{ type: 'text', text: heading[2] }]
      });
      continue;
    }
    if (/^(\\\[|\$\$|\\begin\{equation)/.test(part)) {
      blocks.push({
        type: 'blockMath',
        attrs: {
          latex: part
            .replace(/^(\\\[|\$\$|\\begin\{equation\*?\})/, '')
            .replace(/(\\\]|\$\$|\\end\{equation\*?\})$/, '')
        }
      });
      continue;
    }
    for (const text of part
      .split(/\n\s*\n/)
      .map((x) => x.trim())
      .filter(Boolean))
      blocks.push(
        /\\[a-zA-Z]+/.test(text)
          ? { type: 'codeBlock', attrs: { language: 'latex' }, content: [{ type: 'text', text }] }
          : paragraph(text.replace(/\n/g, ' '))
      );
  }
  if (blocks.length > 2000) throw new Error('Split this LaTeX document into smaller files.');
  return {
    kind: 'document',
    rootId,
    filename,
    warnings,
    ops: [
      rootOp(rootId, filename.replace(/\.[^.]+$/, '')),
      ...(blocks.length ? blocks : [paragraph('')]).map((content, i) =>
        itemOp(rootId, i, {
          content,
          label:
            content.type === 'blockMath'
              ? String(content.attrs?.latex || '')
              : (content.content || []).map((x) => x.text || '').join('')
        })
      )
    ]
  };
}
export async function importOverleaf(file: File) {
  const { default: JSZip } = await import('jszip');
  const archive = await JSZip.loadAsync(await file.arrayBuffer());
  const entries = Object.values(archive.files).filter((x) => !x.dir && x.name.endsWith('.tex'));
  if (entries.length > 500)
    throw new Error('This project contains too many source files. Upload its main .tex file.');
  const main = entries.find((x) => x.name === 'main.tex') || entries[0];
  if (!main) throw new Error('No .tex source file was found in this ZIP.');
  const info = main as unknown as { _data?: { uncompressedSize?: number } };
  if (!info._data?.uncompressedSize || info._data.uncompressedSize > 10 * 1024 * 1024)
    throw new Error('The main LaTeX source must be smaller than 10 MiB.');
  const result = importLatex(await main.async('string'), file.name);
  result.warnings.push(
    `Imported ${main.name}. Included files, images, bibliography and custom packages remain in the original ZIP; they are not expanded into the editor.`
  );
  return result;
}
