import { Extension, Node, Mark, mergeAttributes } from '@tiptap/core';
import { bounded, safeColor, safeFont, scriptFontStack } from './office';
import { FontFamily } from '@tiptap/extension-text-style';

export const OfficeFontFamily = FontFamily.extend({
  addGlobalAttributes() {
    return [
      {
        types: ['textStyle'],
        attributes: {
          fontFamily: {
            default: null,
            parseHTML: (el) =>
              el.getAttribute('data-font-family') ||
              el.style.fontFamily?.split(',')[0].replace(/["']/g, '') ||
              null,
            renderHTML: (a) =>
              a.fontFamily
                ? {
                    'data-font-family': safeFont(a.fontFamily),
                    style: `font-family:${safeFont(a.fontFamily)},${scriptFontStack}`
                  }
                : {}
          }
        }
      }
    ];
  }
});

export const OfficeParagraph = Extension.create({
  name: 'officeParagraph',
  addGlobalAttributes() {
    return [
      {
        types: ['paragraph', 'heading', 'blockquote', 'codeBlock'],
        attributes: {
          dir: {
            default: 'auto',
            parseHTML: (el) => el.getAttribute('dir') || 'auto',
            renderHTML: (a) => ({ dir: ['ltr', 'rtl'].includes(a.dir) ? a.dir : 'auto' })
          },
          lang: {
            default: null,
            parseHTML: (el) => el.getAttribute('lang'),
            renderHTML: (a) => (a.lang && /^[\w-]{2,20}$/.test(a.lang) ? { lang: a.lang } : {})
          },
          lineHeight: {
            default: null,
            parseHTML: (el) => el.style.lineHeight || null,
            renderHTML: (a) =>
              a.lineHeight && /^\d+(\.\d+)?(pt)?$/.test(String(a.lineHeight))
                ? { style: `line-height:${a.lineHeight}` }
                : {}
          },
          spaceBefore: {
            default: null,
            parseHTML: (el) => (el.style.marginTop ? parseFloat(el.style.marginTop) : null),
            renderHTML: (a) =>
              a.spaceBefore === null
                ? {}
                : { style: `margin-top:${bounded(a.spaceBefore, 0, 144, 0)}pt` }
          },
          spaceAfter: {
            default: null,
            parseHTML: (el) => (el.style.marginBottom ? parseFloat(el.style.marginBottom) : null),
            renderHTML: (a) =>
              a.spaceAfter === null
                ? {}
                : { style: `margin-bottom:${bounded(a.spaceAfter, 0, 144, 0)}pt` }
          },
          indent: {
            default: 0,
            parseHTML: (el) => parseFloat(el.style.marginInlineStart) || 0,
            renderHTML: (a) =>
              a.indent ? { style: `margin-inline-start:${bounded(a.indent, 0, 144, 0)}pt` } : {}
          },
          firstLine: {
            default: 0,
            parseHTML: (el) => parseFloat(el.style.textIndent) || 0,
            renderHTML: (a) =>
              a.firstLine ? { style: `text-indent:${bounded(a.firstLine, -72, 144, 0)}pt` } : {}
          },
          keepWithNext: {
            default: false,
            parseHTML: (el) => el.style.breakAfter === 'avoid',
            renderHTML: (a) => (a.keepWithNext ? { style: 'break-after:avoid' } : {})
          },
          pageBreakBefore: {
            default: false,
            parseHTML: (el) => el.style.breakBefore === 'page',
            renderHTML: (a) => (a.pageBreakBefore ? { style: 'break-before:page' } : {})
          }
        }
      },
      {
        types: ['tableCell', 'tableHeader'],
        attributes: {
          backgroundColor: {
            default: null,
            parseHTML: (el) => el.getAttribute('data-fill'),
            renderHTML: (a) =>
              safeColor(a.backgroundColor)
                ? { 'data-fill': a.backgroundColor, style: `background-color:${a.backgroundColor}` }
                : {}
          },
          verticalAlign: {
            default: 'top',
            parseHTML: (el) => el.style.verticalAlign || 'top',
            renderHTML: (a) => ({
              style: `vertical-align:${['top', 'middle', 'bottom'].includes(a.verticalAlign) ? a.verticalAlign : 'top'}`
            })
          }
        }
      }
    ];
  }
});

export const PageBreak = Node.create({
  name: 'pageBreak',
  group: 'block',
  atom: true,
  parseHTML: () => [{ tag: 'div[data-page-break]' }],
  renderHTML: ({ HTMLAttributes }) => [
    'div',
    mergeAttributes(HTMLAttributes, {
      'data-page-break': '',
      'aria-label': 'Page break',
      role: 'separator',
      style: 'break-after:page'
    })
  ],
  addKeyboardShortcuts() {
    return { 'Mod-Enter': () => this.editor.commands.insertContent({ type: 'pageBreak' }) };
  }
});

/** Imported comments stay attached to their text; native collaboration comments use the event log. */
export const SourceComment = Mark.create({
  name: 'sourceComment',
  inclusive: false,
  addAttributes: () => ({ id: { default: '' }, author: { default: '' }, body: { default: '' } }),
  parseHTML: () => [
    {
      tag: 'span[data-source-comment]',
      getAttrs: (el) => ({
        id: el.getAttribute('data-source-comment'),
        author: el.getAttribute('data-author'),
        body: el.getAttribute('data-comment')
      })
    }
  ],
  renderHTML: ({ mark }) => [
    'span',
    {
      'data-source-comment': mark.attrs.id,
      'data-author': mark.attrs.author,
      'data-comment': mark.attrs.body,
      title: `${mark.attrs.author}: ${mark.attrs.body}`,
      class: 'source-comment'
    },
    0
  ]
});
