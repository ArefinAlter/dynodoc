import * as w from 'docx';
import type { DocumentState } from '$lib/api/types';
import type { TextBlock } from './files';
import { bounded, pageSetup, pageDimensions, safeColor, safeFont } from './office';

/** Export the supported OOXML subset directly, retaining text review metadata. */
export async function writeDocx(state: DocumentState, title: string): Promise<Blob> {
  const live = Object.values(state.nodes).filter((n) => !n.deleted);
  const root = live.find((n) => n.type === 'form');
  const page = pageSetup(root?.current_fields.page_setup),
    [width, height] = pageDimensions(page);
  const footnotes: Record<string, { children: w.Paragraph[] }> = {},
    endnotes: Record<string, { children: w.Paragraph[] }> = {};
  let noteId = 1;
  const comments: w.ICommentOptions[] = [];
  let revisionId = 0,
    listId = 0;
  const numbering: w.INumberingOptions['config'][number][] = [];
  function runs(block: TextBlock): w.ParagraphChild[] {
    if (block.type === 'documentNote') {
      const id = noteId++,
        kind = block.attrs?.kind === 'endnote' ? 'endnote' : 'footnote';
      (kind === 'footnote' ? footnotes : endnotes)[String(id)] = {
        children: String(block.attrs?.body || '')
          .split('\n')
          .map((text) => new w.Paragraph(text))
      };
      return [kind === 'footnote' ? new w.FootnoteReferenceRun(id) : new w.EndnoteReferenceRun(id)];
    }
    if (block.type === 'hardBreak') return [new w.TextRun({ break: 1 })];
    if (block.type === 'inlineMath' || block.type === 'blockMath')
      return [new w.TextRun({ text: String(block.attrs?.latex || ''), font: 'Cambria Math' })];
    if (block.type === 'image') {
      const source = String(block.attrs?.src || ''),
        match = /^data:image\/(png|jpe?g|gif);base64,([a-z\d+/=]+)$/i.exec(source);
      if (match) {
        const bytes = Uint8Array.from(atob(match[2]), (c) => c.charCodeAt(0));
        const imageWidth = bounded(block.attrs?.width, 20, 600, 400),
          imageHeight = bounded(block.attrs?.height, 20, 700, 260);
        return [
          new w.ImageRun({
            type: match[1] === 'jpeg' ? 'jpg' : (match[1] as 'png' | 'jpg' | 'gif'),
            data: bytes,
            transformation: { width: imageWidth, height: imageHeight },
            altText: {
              title: String(block.attrs?.alt || ''),
              description: String(block.attrs?.alt || ''),
              name: 'Document image'
            }
          })
        ];
      }
      return [
        new w.TextRun({ text: `[Image: ${String(block.attrs?.alt || source)}]`, italics: true })
      ];
    }
    if (block.text !== undefined) {
      const marks = block.marks || [],
        has = (type: string) => marks.some((m) => m.type === type),
        style = marks.find((m) => m.type === 'textStyle')?.attrs || {},
        highlight = safeColor(marks.find((m) => m.type === 'highlight')?.attrs?.color);
      const size = style.fontSize
        ? parseFloat(String(style.fontSize)) * (String(style.fontSize).endsWith('px') ? 1.5 : 2)
        : undefined;
      const options: w.IRunOptions = {
        text: block.text,
        bold: has('bold'),
        italics: has('italic'),
        boldComplexScript: has('bold'),
        italicsComplexScript: has('italic'),
        underline: has('underline') ? {} : undefined,
        strike: has('strike'),
        superScript: has('superscript'),
        subScript: has('subscript'),
        font: style.fontFamily ? safeFont(style.fontFamily) : undefined,
        size,
        sizeComplexScript: size,
        color: safeColor(style.color).slice(1) || undefined,
        shading: highlight ? { fill: highlight.slice(1), type: w.ShadingType.CLEAR } : undefined,
        language: { value: page.language }
      };
      const review = marks.find((m) => m.type === 'trackedChange')?.attrs;
      let run: w.ParagraphChild = new w.TextRun(options);
      if (review) {
        const attrs = {
          ...options,
          id: revisionId++,
          author: String(review.author || 'Editor'),
          date: Number.isFinite(Date.parse(String(review.date)))
            ? String(review.date)
            : new Date(0).toISOString()
        };
        run = review.kind === 'delete' ? new w.DeletedTextRun(attrs) : new w.InsertedTextRun(attrs);
      }
      const href = String(marks.find((m) => m.type === 'link')?.attrs?.href || '');
      if (/^(https?:\/\/|mailto:)/i.test(href))
        run = new w.ExternalHyperlink({ link: href, children: [run] });
      const comment = marks.find((m) => m.type === 'sourceComment')?.attrs;
      if (comment) {
        const id = comments.length;
        comments.push({
          id,
          author: String(comment.author || 'Word reviewer'),
          children: [new w.Paragraph(String(comment.body || ''))]
        });
        return [
          new w.CommentRangeStart(id),
          run,
          new w.CommentRangeEnd(id),
          new w.TextRun({ children: [new w.CommentReference(id)] })
        ];
      }
      return [run];
    }
    return (block.content || []).flatMap(runs);
  }
  function blocks(
    block: TextBlock,
    list?: { reference: string; level: number }
  ): (w.Paragraph | w.Table)[] {
    if (block.type === 'pageBreak') return [new w.Paragraph({ children: [new w.PageBreak()] })];
    if (block.type === 'table')
      return [
        new w.Table({
          width: { size: 100, type: w.WidthType.PERCENTAGE },
          rows: (block.content || []).map(
            (row) =>
              new w.TableRow({
                tableHeader: row.content?.every((c) => c.type === 'tableHeader'),
                children: (row.content || []).map(
                  (cell) =>
                    new w.TableCell({
                      columnSpan: Number(cell.attrs?.colspan) || 1,
                      rowSpan: Number(cell.attrs?.rowspan) || 1,
                      verticalAlign:
                        cell.attrs?.verticalAlign === 'middle'
                          ? w.VerticalAlign.CENTER
                          : cell.attrs?.verticalAlign === 'bottom'
                            ? w.VerticalAlign.BOTTOM
                            : w.VerticalAlign.TOP,
                      shading: safeColor(cell.attrs?.backgroundColor)
                        ? { fill: safeColor(cell.attrs?.backgroundColor).slice(1) }
                        : undefined,
                      children: (cell.content?.length
                        ? cell.content
                        : [{ type: 'paragraph' }]
                      ).flatMap((c) => blocks(c))
                    })
                )
              })
          )
        })
      ];
    if (['bulletList', 'orderedList', 'taskList'].includes(block.type)) {
      const reference = `list-${listId++}`,
        level = Math.min(list?.level ?? 0, 8);
      numbering.push({
        reference,
        levels: Array.from({ length: 9 }, (_, i) => ({
          level: i,
          format: block.type === 'orderedList' ? w.LevelFormat.DECIMAL : w.LevelFormat.BULLET,
          text: block.type === 'orderedList' ? `%${i + 1}.` : '•',
          start: Number(block.attrs?.start) || 1,
          style: { paragraph: { indent: { left: 720 * (i + 1), hanging: 360 } } }
        }))
      });
      return (block.content || []).flatMap((item) =>
        (item.content || []).flatMap((c, i) =>
          blocks(
            c,
            ['bulletList', 'orderedList', 'taskList'].includes(c.type)
              ? { reference, level: level + 1 }
              : i === 0
                ? { reference, level }
                : undefined
          )
        )
      );
    }
    if (['listItem', 'taskItem', 'blockquote'].includes(block.type))
      return (block.content || []).flatMap((c) => blocks(c, list));
    const attrs = block.attrs || {},
      level = bounded(attrs.level, 1, 6, 1),
      align = String(attrs.textAlign || 'left');
    const spacing = attrs.lineHeight ? String(attrs.lineHeight) : '';
    return [
      new w.Paragraph({
        children: runs(block),
        heading:
          block.type === 'heading'
            ? [
                w.HeadingLevel.HEADING_1,
                w.HeadingLevel.HEADING_2,
                w.HeadingLevel.HEADING_3,
                w.HeadingLevel.HEADING_4,
                w.HeadingLevel.HEADING_5,
                w.HeadingLevel.HEADING_6
              ][level - 1]
            : undefined,
        alignment: (
          {
            left: w.AlignmentType.LEFT,
            center: w.AlignmentType.CENTER,
            right: w.AlignmentType.RIGHT,
            justify: w.AlignmentType.JUSTIFIED
          } as Record<string, (typeof w.AlignmentType)[keyof typeof w.AlignmentType]>
        )[align],
        bidirectional: attrs.dir === 'rtl',
        keepNext: attrs.keepWithNext === true,
        pageBreakBefore: attrs.pageBreakBefore === true,
        widowControl: true,
        indent: {
          left: bounded(attrs.indent, 0, 144, 0) * 20,
          firstLine: Math.max(0, Number(attrs.firstLine) || 0) * 20,
          hanging: Math.max(0, -(Number(attrs.firstLine) || 0)) * 20
        },
        spacing: {
          before:
            attrs.spaceBefore === undefined || attrs.spaceBefore === null
              ? undefined
              : bounded(attrs.spaceBefore, 0, 144, 0) * 20,
          after:
            attrs.spaceAfter === undefined || attrs.spaceAfter === null
              ? undefined
              : bounded(attrs.spaceAfter, 0, 144, 0) * 20,
          line: spacing ? parseFloat(spacing) * (spacing.endsWith('pt') ? 20 : 240) : undefined,
          lineRule: spacing.endsWith('pt') ? w.LineRuleType.EXACT : w.LineRuleType.AUTO
        },
        numbering: list
      })
    ];
  }
  const tabs = live
    .filter(
      (n) => n.type === 'form' || (n.type === 'section' && n.current_fields.kind === 'document_tab')
    )
    .sort((a, b) => (a.type === 'form' ? -1 : b.type === 'form' ? 1 : a.pos.localeCompare(b.pos)));
  const children: (w.Paragraph | w.Table | w.TableOfContents)[] = [];
  for (const tab of tabs) {
    if (tab.type !== 'form')
      children.push(
        new w.Paragraph({
          text: String(tab.current_fields.label || 'Tab'),
          heading: w.HeadingLevel.HEADING_1,
          pageBreakBefore: true
        })
      );
    for (const n of live
      .filter((n) => n.type === 'item' && n.parent_id === tab.id)
      .sort((a, b) => a.pos.localeCompare(b.pos))) {
      const source = n.current_fields.content as TextBlock | undefined;
      if (source?.type === 'tableOfContents') {
        children.push(
          new w.TableOfContents('Contents', {
            hyperlink: true,
            headingStyleRange: '1-3',
            beginDirty: true,
            contentChildren: [
              new w.Paragraph('Contents'),
              ...(Array.isArray(source.attrs?.entries) ? source.attrs.entries : []).map(
                (entry: { text: string }) => new w.Paragraph(String(entry.text))
              )
            ]
          })
        );
        continue;
      }
      const content = blocks(
        (n.current_fields.content as TextBlock) || {
          type: 'paragraph',
          content: [{ type: 'text', text: String(n.current_fields.label || '') }]
        }
      );
      children.push(...content);
    }
  }
  // Block comments are exported in a separate notes section: never fabricate precise text anchors.
  if (state.comments.length) {
    children.push(
      new w.Paragraph({ text: 'Document comments', heading: w.HeadingLevel.HEADING_1 })
    );
    for (const c of state.comments) children.push(new w.Paragraph(c.body));
  }
  const footer: w.ParagraphChild[] = [new w.TextRun(page.footer)];
  if (page.pageNumbers)
    footer.push(
      new w.TextRun({ children: ['  ', w.PageNumber.CURRENT, ' / ', w.PageNumber.TOTAL_PAGES] })
    );
  const doc = new w.Document({
    title,
    creator: 'Dynodoc',
    footnotes,
    endnotes,
    numbering: { config: numbering },
    comments: { children: comments },
    sections: [
      {
        properties: {
          page: {
            size: { width: Math.round(width * 56.6929), height: Math.round(height * 56.6929) },
            margin: {
              top: page.margin * 56.6929,
              right: page.margin * 56.6929,
              bottom: page.margin * 56.6929,
              left: page.margin * 56.6929
            }
          }
        },
        headers: { default: new w.Header({ children: [new w.Paragraph(page.header)] }) },
        footers: {
          default: new w.Footer({
            children: [new w.Paragraph({ children: footer, alignment: w.AlignmentType.CENTER })]
          })
        },
        children: children.length ? children : [new w.Paragraph('')]
      }
    ]
  });
  return w.Packer.toBlob(doc);
}
