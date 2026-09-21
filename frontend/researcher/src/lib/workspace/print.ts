import { pageDimensions, pageSetup, scriptFontStack, type PageSetup } from './office';
export function printCss(settings: PageSetup) {
  const page = pageSetup(settings),
    [w, h] = pageDimensions(page);
  const quoted = (text: string) => JSON.stringify(text.replace(/[\r\n\f]/g, ' '));
  return `@page { size: ${w}mm ${h}mm; margin: ${page.margin}mm;
    @top-center { content: ${quoted(page.header)}; font: 10pt Arial, ${scriptFontStack}; }
    @bottom-center { content: ${quoted(page.footer)} ${page.pageNumbers ? '"  " counter(page) " / " counter(pages)' : ''}; font: 10pt Arial, ${scriptFontStack}; }
  }
  .print-content { font: 11pt/1.6 Arial, ${scriptFontStack}; color:#202124; overflow-wrap:anywhere; }
  p { margin:.7em 0; widows:2; orphans:2; white-space:pre-wrap; }
  h1,h2,h3,h4,h5,h6 { break-after:avoid; line-height:1.25; } h1 {font-size:26pt} h2{font-size:19pt} h3{font-size:15pt}
  table {border-collapse:collapse; width:100%;table-layout:fixed} td,th {border:1px solid #b6c2cf;padding:6px;vertical-align:top;overflow-wrap:anywhere}
  thead{display:table-header-group} tr{break-inside:avoid} img{max-width:100%;max-height:${h - page.margin * 2 - 15}mm;object-fit:contain}
  [data-page-break]{break-after:page;height:0} .review-delete{text-decoration:line-through;color:#a02a32;background:#fce7e7} .review-insert{color:#15633a;text-decoration:underline;background:#e1f4e9}
  blockquote{border-inline-start:3px solid #b7c6d8;padding-inline-start:16px} pre{white-space:pre-wrap} a{color:#245999}
  ul[data-type=taskList]{list-style:none} li[data-type=taskItem]{display:flex;gap:8px}
  .pagedjs_page {background:white;margin:16px auto;box-shadow:0 1px 8px #18334725}
  `;
}

/** Input is editor-generated markup, nevertheless never execute embedded markup in preview. */
export function previewContent(html: string) {
  const doc = new DOMParser().parseFromString(html, 'text/html');
  doc
    .querySelectorAll('script,style,iframe,object,embed,link,meta,base,form,button,input')
    .forEach((n) => n.remove());
  for (const el of doc.querySelectorAll('*'))
    for (const a of [...el.attributes]) {
      if (
        /^on/i.test(a.name) ||
        a.name === 'srcdoc' ||
        (['href', 'src'].includes(a.name) &&
          !/^(https:\/\/|data:image\/(png|jpe?g|gif|webp);base64,|mailto:)/i.test(a.value))
      )
        el.removeAttribute(a.name);
    }
  const fragment = document.createDocumentFragment();
  const container = document.createElement('div');
  container.className = 'print-content';
  container.append(...Array.from(doc.body.childNodes));
  fragment.append(container);
  return fragment;
}
