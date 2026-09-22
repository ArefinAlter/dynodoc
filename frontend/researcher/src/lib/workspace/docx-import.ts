import { DOMParser } from '@xmldom/xmldom';
import { ulid } from 'ulid';
import { readOfficeZip, xmlText } from './bounded-zip';
import { bounded, pageSetup, safeFont, type PageSetup } from './office';
import type { TextBlock } from './files';
type X = NonNullable<ReturnType<DOMParser['parseFromString']>['documentElement']>;
type Properties = Record<string, unknown>;
const children = (e: X | null | undefined, tag?: string): X[] =>
  e
    ? Array.from(e.childNodes).filter(
        (n): n is X => n.nodeType === 1 && (!tag || n.localName === tag)
      )
    : [];
const child = (e: X | null | undefined, tag: string) => children(e, tag)[0];
const attr = (e: X | null | undefined, name = 'val') =>
  e?.getAttribute(`w:${name}`) || e?.getAttribute(name) || '';
const val = (e: X | null | undefined, tag: string) => attr(child(e, tag));
const enabled = (e: X | undefined) => !!e && !['0', 'false', 'off'].includes(attr(e));
const descendants = (e: X, name: string): X[] =>
  Array.from(e.getElementsByTagName('*')).filter((n) => n.localName === name);
const color = (v: string) => (/^[a-f\d]{6}$/i.test(v) ? `#${v}` : undefined);
const highlight: Record<string, string> = {
  yellow: '#ffff00',
  green: '#00ff00',
  cyan: '#00ffff',
  magenta: '#ff00ff',
  blue: '#0000ff',
  red: '#ff0000',
  darkBlue: '#000080',
  darkCyan: '#008080',
  darkGreen: '#008000',
  darkMagenta: '#800080',
  darkRed: '#800000',
  darkYellow: '#808000',
  darkGray: '#808080',
  lightGray: '#c0c0c0',
  black: '#000000',
  white: '#ffffff'
};

export interface DocxImport {
  blocks: TextBlock[];
  warnings: string[];
  page: PageSetup;
}
export function readDocx(bytes: Uint8Array): DocxImport {
  const parts = readOfficeZip(bytes, (name) =>
    /^word\/(document\.xml|styles\.xml|numbering\.xml|comments\.xml|footnotes\.xml|endnotes\.xml|theme\/theme1\.xml|header\d+\.xml|footer\d+\.xml|_rels\/document\.xml\.rels|media\/[^/]+)$/.test(
      name
    )
  );
  const parse = (name: string) => {
    const source = xmlText(parts.get(name));
    if (!source) return undefined;
    return new DOMParser({
      onError: (level, message) => {
        if (level !== 'warning') throw new Error(`Invalid Word XML: ${message.slice(0, 120)}`);
      }
    }).parseFromString(source, 'application/xml').documentElement!;
  };
  const doc = parse('word/document.xml');
  if (!doc || doc.localName !== 'document')
    throw new Error('This is not a readable DOCX document.');
  const warnings = new Set<string>();
  const stylesXml = parse('word/styles.xml'),
    numbering = parse('word/numbering.xml');
  const styles = new Map(children(stylesXml, 'style').map((s) => [attr(s, 'styleId'), s]));
  const defaults = child(stylesXml, 'docDefaults');
  const relationships = new Map(
    children(parse('word/_rels/document.xml.rels')).map((r) => [r.getAttribute('Id') || '', r])
  );
  const comments = new Map(
    children(parse('word/comments.xml'), 'comment').map((c) => [
      attr(c, 'id'),
      {
        id: attr(c, 'id'),
        author: attr(c, 'author'),
        body: descendants(c, 't')
          .map((t) => t.textContent)
          .join(' ')
          .slice(0, 10000)
      }
    ])
  );
  const noteBodies = new Map(
    ['footnote', 'endnote'].flatMap((kind) =>
      children(parse(`word/${kind}s.xml`), kind)
        .filter((n) => Number(attr(n, 'id')) > 0)
        .map((n) => [
          `${kind}:${attr(n, 'id')}`,
          children(n, 'p')
            .map((p) =>
              descendants(p, 't')
                .map((t) => t.textContent || '')
                .join('')
            )
            .join('\n')
            .slice(0, 10000)
        ])
    )
  );
  const activeComments = new Set<string>();
  const theme = parse('word/theme/theme1.xml');
  const fonts = new Map(
    theme
      ? descendants(theme, 'fontScheme').flatMap((s) =>
          children(s).map(
            (f) => [f.localName, child(f, 'latin')?.getAttribute('typeface') || 'Arial'] as const
          )
        )
      : []
  );
  function runProps(p?: X): Properties {
    const o: Properties = {};
    for (const [xml, key] of [
      ['b', 'bold'],
      ['i', 'italic'],
      ['strike', 'strike'],
      ['u', 'underline']
    ] as const) {
      const e = child(p, xml);
      if (e) o[key] = enabled(e) && attr(e) !== 'none';
    }
    const f = child(p, 'rFonts');
    if (f)
      o.fontFamily = safeFont(
        attr(f, 'ascii') ||
          attr(f, 'hAnsi') ||
          attr(f, 'cs') ||
          fonts.get(attr(f, 'asciiTheme').startsWith('major') ? 'majorFont' : 'minorFont') ||
          'Arial'
      );
    const size = val(p, 'sz') || val(p, 'szCs');
    if (size) o.fontSize = `${bounded(Number(size) / 2, 1, 400, 11)}pt`;
    if (color(val(p, 'color'))) o.color = color(val(p, 'color'));
    if (child(p, 'highlight')) o.highlight = highlight[val(p, 'highlight')] || null;
    if (child(p, 'vertAlign')) {
      o.superscript = val(p, 'vertAlign') === 'superscript';
      o.subscript = val(p, 'vertAlign') === 'subscript';
    }
    return o;
  }
  function paraProps(p?: X): Properties {
    const o: Properties = {};
    const align = val(p, 'jc');
    if (align)
      o.textAlign =
        ({ both: 'justify', start: 'left', end: 'right' } as Record<string, string>)[align] ||
        align;
    if (child(p, 'bidi')) o.dir = enabled(child(p, 'bidi')) ? 'rtl' : 'ltr';
    const s = child(p, 'spacing');
    if (s) {
      if (attr(s, 'before')) o.spaceBefore = Number(attr(s, 'before')) / 20;
      if (attr(s, 'after')) o.spaceAfter = Number(attr(s, 'after')) / 20;
      if (attr(s, 'line'))
        o.lineHeight =
          attr(s, 'lineRule') === 'auto' || !attr(s, 'lineRule')
            ? String(Number(attr(s, 'line')) / 240)
            : `${Number(attr(s, 'line')) / 20}pt`;
    }
    const indent = child(p, 'ind');
    if (indent) {
      o.indent = Number(attr(indent, 'left') || attr(indent, 'start')) / 20;
      o.firstLine = (Number(attr(indent, 'firstLine')) - Number(attr(indent, 'hanging'))) / 20;
    }
    if (child(p, 'keepNext')) o.keepWithNext = enabled(child(p, 'keepNext'));
    if (child(p, 'pageBreakBefore')) o.pageBreakBefore = enabled(child(p, 'pageBreakBefore'));
    return o;
  }
  const resolved = new Map<string, { run: Properties; para: Properties }>();
  function style(id: string, seen = new Set<string>()): { run: Properties; para: Properties } {
    if (!id || !styles.has(id) || seen.has(id) || seen.size > 32) return { run: {}, para: {} };
    const cached = resolved.get(id);
    if (cached) return cached;
    seen.add(id);
    const source = styles.get(id)!;
    const base = style(val(source, 'basedOn'), seen);
    const result = {
      run: { ...base.run, ...runProps(child(source, 'rPr')) },
      para: { ...base.para, ...paraProps(child(source, 'pPr')) }
    };
    resolved.set(id, result);
    return result;
  }
  const defaultRun = runProps(child(child(defaults, 'rPrDefault'), 'rPr'));
  const defaultPara = paraProps(child(child(defaults, 'pPrDefault'), 'pPr'));
  const normal = children(stylesXml, 'style').find(
    (s) => attr(s, 'type') === 'paragraph' && attr(s, 'default') === '1'
  );
  const defaultStyle = style(normal ? attr(normal, 'styleId') : 'Normal');
  let mediaTotal = 0;
  function imageBlock(e: X): TextBlock | undefined {
    const blip = descendants(e, 'blip')[0],
      rel = relationships.get(blip?.getAttribute('r:embed') || '');
    if (!rel || rel.getAttribute('TargetMode') === 'External') {
      warnings.add('External or unsupported images remain in the original file.');
      return;
    }
    const target = rel.getAttribute('Target') || '',
      name = target.startsWith('/word/') ? target.slice(1) : `word/${target}`;
    const data = parts.get(name),
      ext = name.split('.').pop()?.toLowerCase();
    if (
      !data ||
      !['png', 'jpg', 'jpeg', 'gif', 'webp'].includes(ext || '') ||
      data.length > 1024 * 1024 ||
      mediaTotal + data.length > 5 * 1024 * 1024
    ) {
      warnings.add('Some images exceed the supported format or size; consult the original file.');
      return;
    }
    mediaTotal += data.length;
    let binary = '';
    for (let i = 0; i < data.length; i += 8192)
      binary += String.fromCharCode(...data.subarray(i, i + 8192));
    const desc = descendants(e, 'docPr')[0];
    const extent = descendants(e, 'extent')[0];
    const width = Number(extent?.getAttribute('cx')) / 9525;
    const height = Number(extent?.getAttribute('cy')) / 9525;
    return {
      type: 'image',
      attrs: {
        ...(width > 0 && height > 0
          ? { width: Math.round(width), height: Math.round(height) }
          : {}),
        src: `data:image/${ext === 'jpg' ? 'jpeg' : ext};base64,${btoa(binary)}`,
        alt: desc?.getAttribute('descr') || desc?.getAttribute('name') || 'Imported image'
      }
    };
  }
  function marks(props: Properties): NonNullable<TextBlock['marks']> {
    const result: NonNullable<TextBlock['marks']> = [];
    for (const type of ['bold', 'italic', 'underline', 'strike', 'superscript', 'subscript'])
      if (props[type]) result.push({ type });
    const attrs = Object.fromEntries(
      ['fontFamily', 'fontSize', 'color'].filter((k) => props[k]).map((k) => [k, props[k]])
    );
    if (Object.keys(attrs).length) result.push({ type: 'textStyle', attrs });
    if (props.highlight) result.push({ type: 'highlight', attrs: { color: props.highlight } });
    return result;
  }
  function inline(
    e: X,
    inherited: Properties,
    extra: NonNullable<TextBlock['marks']> = []
  ): TextBlock[] {
    const out: TextBlock[] = [];
    for (const n of children(e)) {
      switch (n.localName) {
        case 'r': {
          const p = child(n, 'rPr');
          out.push(
            ...inline(n, { ...inherited, ...style(val(p, 'rStyle')).run, ...runProps(p) }, extra)
          );
          break;
        }
        case 't':
        case 'delText': {
          const text = n.textContent || '';
          if (text)
            out.push({
              type: 'text',
              text,
              marks: [
                ...marks(inherited),
                ...extra,
                ...[...activeComments].flatMap((id) =>
                  comments.has(id) ? [{ type: 'sourceComment', attrs: comments.get(id)! }] : []
                )
              ]
            });
          break;
        }
        case 'tab':
          out.push({ type: 'text', text: '\t', marks: [...marks(inherited), ...extra] });
          break;
        case 'br':
          out.push({ type: attr(n, 'type') === 'page' ? 'pageBreak' : 'hardBreak' });
          break;
        case 'cr':
          out.push({ type: 'hardBreak' });
          break;
        case 'hyperlink': {
          const href =
            relationships.get(n.getAttribute('r:id') || '')?.getAttribute('Target') || '';
          out.push(
            ...inline(
              n,
              inherited,
              /^(https?:\/\/|mailto:)/i.test(href)
                ? [...extra, { type: 'link', attrs: { href } }]
                : extra
            )
          );
          break;
        }
        case 'ins':
        case 'del': {
          const date = attr(n, 'date');
          out.push(
            ...inline(n, inherited, [
              ...extra,
              {
                type: 'trackedChange',
                attrs: {
                  id: `word-${attr(n, 'id') || ulid()}`,
                  kind: n.localName === 'ins' ? 'insert' : 'delete',
                  author: attr(n, 'author') || 'Word reviewer',
                  date: Number.isFinite(Date.parse(date)) ? date : ''
                }
              }
            ])
          );
          break;
        }
        case 'commentRangeStart':
          activeComments.add(attr(n, 'id'));
          break;
        case 'commentRangeEnd':
          activeComments.delete(attr(n, 'id'));
          break;
        case 'drawing':
        case 'pict': {
          const image = imageBlock(n);
          if (image) out.push(image);
          break;
        }
        case 'oMath':
        case 'oMathPara': {
          out.push({
            type: 'text',
            text:
              descendants(n, 't')
                .map((t) => t.textContent)
                .join('') || '[Equation]'
          });
          warnings.add(
            'Word equations are imported as linear text. Keep the original for native equation layout.'
          );
          break;
        }
        case 'footnoteReference':
        case 'endnoteReference': {
          const kind = n.localName === 'footnoteReference' ? 'footnote' : 'endnote';
          out.push({
            type: 'documentNote',
            attrs: { id: ulid(), kind, body: noteBodies.get(`${kind}:${attr(n, 'id')}`) || '' }
          });
          warnings.add(
            'Footnote and endnote bodies are imported as text. The browser shows them in the notes panel; DOCX export preserves native note references.'
          );
          break;
        }
        case 'rPr':
        case 'pPr':
        case 'commentReference':
        case 'bookmarkStart':
        case 'bookmarkEnd':
        case 'proofErr':
        case 'lastRenderedPageBreak':
        case 'instrText':
        case 'fldChar':
          break;
        default:
          if (children(n).length) out.push(...inline(n, inherited, extra));
      }
    }
    return out;
  }
  function paragraph(e: X): TextBlock[] {
    const p = child(e, 'pPr'),
      id = val(p, 'pStyle'),
      inherited = style(id);
    const heading = /heading\s*([1-6])/i.exec(id || val(styles.get(id), 'name'));
    const outline = val(p, 'outlineLvl') || val(child(styles.get(id), 'pPr'), 'outlineLvl');
    const level = heading
      ? Number(heading[1])
      : outline && Number(outline) < 6
        ? Number(outline) + 1
        : 0;
    const attrs = {
      ...defaultPara,
      ...defaultStyle.para,
      ...inherited.para,
      ...paraProps(p),
      ...(level ? { level } : {})
    };
    const nodes = inline(e, {
      ...defaultRun,
      ...defaultStyle.run,
      ...inherited.run,
      ...runProps(child(p, 'rPr'))
    });
    const blocks: TextBlock[] = [],
      images: TextBlock[] = [];
    let content: TextBlock[] = [];
    const flush = () => {
      blocks.push({ type: level ? 'heading' : 'paragraph', attrs, content });
      content = [];
    };
    for (const n of nodes) {
      if (n.type === 'pageBreak') {
        flush();
        blocks.push(n);
      } else if (n.type === 'image') images.push(n);
      else content.push(n);
    }
    flush();
    if (images.length) {
      blocks.push(...images);
      warnings.add(
        'Images are positioned after their containing paragraph; floating placement requires review.'
      );
    }
    return blocks;
  }
  function listInfo(e: X) {
    const p = child(e, 'pPr'),
      np = child(p, 'numPr') || child(child(styles.get(val(p, 'pStyle')), 'pPr'), 'numPr');
    const id = val(np, 'numId');
    if (!id || id === '0') return;
    const num = children(numbering, 'num').find((n) => attr(n, 'numId') === id);
    const abstract = children(numbering, 'abstractNum').find(
      (n) => attr(n, 'abstractNumId') === val(num, 'abstractNumId')
    );
    const level = Number(val(np, 'ilvl')) || 0;
    const definition = children(abstract, 'lvl').find((n) => Number(attr(n, 'ilvl')) === level);
    const format = val(definition, 'numFmt');
    if (!['decimal', 'bullet', ''].includes(format))
      warnings.add('Custom list numbering is normalized to decimal or bullets.');
    return {
      id,
      level,
      type: format === 'bullet' ? 'bulletList' : 'orderedList',
      start: Number(val(definition, 'start')) || 1
    };
  }
  function blocks(e: X): TextBlock[] {
    const out: TextBlock[] = [];
    let stack: { id: string; list: TextBlock }[] = [];
    for (const n of children(e)) {
      if (n.localName === 'p') {
        const parsed = paragraph(n),
          info = listInfo(n);
        if (!info) {
          stack = [];
          out.push(...parsed);
          continue;
        }
        const depth = Math.min(info.level, stack.length, 8);
        if (!stack[depth] || stack[depth].id !== info.id || stack[depth].list.type !== info.type) {
          stack = stack.slice(0, depth);
          const list: TextBlock = {
            type: info.type,
            attrs: info.type === 'orderedList' ? { start: info.start } : {},
            content: []
          };
          if (depth && stack[depth - 1].list.content?.at(-1))
            stack[depth - 1].list.content!.at(-1)!.content!.push(list);
          else out.push(list);
          stack[depth] = { id: info.id, list };
        }
        stack = stack.slice(0, depth + 1);
        stack[depth].list.content!.push({ type: 'listItem', content: parsed });
      } else if (n.localName === 'tbl') {
        stack = [];
        const table: TextBlock = { type: 'table', content: [] };
        const spanning = new Map<number, TextBlock>();
        for (const row of children(n, 'tr')) {
          const cells: TextBlock[] = [];
          let column = 0;
          for (const cell of children(row, 'tc')) {
            const p = child(cell, 'tcPr'),
              colspan = bounded(val(p, 'gridSpan'), 1, 100, 1),
              merge = child(p, 'vMerge');
            if (merge && attr(merge) !== 'restart' && spanning.has(column)) {
              const above = spanning.get(column)!;
              above.attrs!.rowspan = Number(above.attrs!.rowspan) + 1;
              column += colspan;
              continue;
            }
            const result: TextBlock = {
              type: enabled(child(child(row, 'trPr'), 'tblHeader')) ? 'tableHeader' : 'tableCell',
              attrs: {
                colspan,
                rowspan: 1,
                backgroundColor: color(attr(child(p, 'shd'), 'fill')),
                verticalAlign: val(p, 'vAlign') === 'center' ? 'middle' : val(p, 'vAlign') || 'top'
              },
              content: blocks(cell)
            };
            if (!result.content!.length) result.content = [{ type: 'paragraph' }];
            if (merge) spanning.set(column, result);
            else spanning.delete(column);
            cells.push(result);
            column += colspan;
          }
          table.content!.push({ type: 'tableRow', content: cells });
        }
        if (table.content!.length) out.push(table);
      } else if (n.localName !== 'sectPr') {
        stack = [];
        out.push(...blocks(n));
      }
    }
    return out;
  }
  const body = child(doc, 'body');
  if (!body) throw new Error('DOCX has no document body.');
  const result = blocks(body);
  if (result.length > 19000)
    throw new Error('This document exceeds the 19,000-block import limit.');
  const sections = descendants(body, 'sectPr'),
    section = sections.at(-1),
    size = child(section, 'pgSz'),
    margin = child(section, 'pgMar');
  if (sections.length > 1)
    warnings.add('Multiple Word sections use the last section’s page setup in the editable copy.');
  const width = Number(attr(size, 'w')) / 56.6929,
    height = Number(attr(size, 'h')) / 56.6929;
  const page = pageSetup({
    size:
      Math.abs(Math.max(width, height) - 355.6) < 3
        ? 'Legal'
        : Math.abs(Math.min(width, height) - 215.9) < 3
          ? 'Letter'
          : 'A4',
    landscape: width > height,
    margin: margin ? Number(attr(margin, 'left')) / 56.6929 : 25.4,
    language: attr(child(child(defaults, 'rPrDefault'), 'lang')) || 'en'
  });
  for (const [part, slot] of [
    ['headerReference', 'header'],
    ['footerReference', 'footer']
  ] as const) {
    const ref = child(section, part),
      target = relationships.get(ref?.getAttribute('r:id') || '')?.getAttribute('Target');
    if (target) {
      const source = parse(target.startsWith('/word/') ? target.slice(1) : `word/${target}`);
      if (source)
        page[slot] = descendants(source, 't')
          .map((t) => t.textContent)
          .join(' ')
          .slice(0, 200);
    }
  }
  if (
    descendants(doc, 'rPrChange').length ||
    descendants(doc, 'pPrChange').length ||
    descendants(doc, 'moveFrom').length ||
    descendants(doc, 'moveTo').length
  )
    warnings.add(
      'Text insertions/deletions are preserved for review. Formatting revisions and tracked moves are flattened; consult the original.'
    );
  warnings.add(
    'Review conversion before use: advanced page layout, fields, embedded objects, section-specific headers, font availability and native equations may differ. The original upload remains available.'
  );
  return {
    blocks: result.length ? result : [{ type: 'paragraph' }],
    warnings: [...warnings],
    page
  };
}
