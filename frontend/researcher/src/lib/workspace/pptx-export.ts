import type { DocumentState } from '$lib/api/types';
import { readDeck, resolvedStyle, slideSizes, themes, type SlideShape } from './presentation';

const hex = (color: string) => color.replace('#', '').toUpperCase();
const names = { title: 'Title', subtitle: 'Subtitle', body: 'Content' } as const;
/** Shape names carry the placeholder role so a Dynodoc export imports back with roles. */
function objectName(shape: SlideShape, index: number) {
  const base = shape.role
    ? names[shape.role]
    : shape.kind === 'image'
      ? 'Picture'
      : shape.kind === 'shape'
        ? 'Rectangle'
        : 'TextBox';
  return `${base} ${index + 1}`;
}

/** Writes a PowerPoint file with pptxgenjs (loaded only when a deck is exported). */
export async function writePptx(
  state: DocumentState,
  title: string
): Promise<Uint8Array<ArrayBuffer>> {
  const { default: PptxGenJS } = await import('pptxgenjs');
  const deck = readDeck(state);
  const size = slideSizes[deck.size];
  const theme = themes[deck.theme];
  const pptx = new PptxGenJS();
  pptx.defineLayout({ name: 'DYNODOC', width: size.width, height: size.height });
  pptx.layout = 'DYNODOC';
  pptx.title = title;
  pptx.company = 'Dynodoc';
  for (const slide of deck.slides) {
    const out = pptx.addSlide();
    out.background = { color: hex(slide.background || theme.background) };
    if (slide.hidden) out.hidden = true;
    if (slide.notes) out.addNotes(slide.notes);
    slide.shapes.forEach((shape, index) => {
      const box = {
        x: (shape.x / 100) * size.width,
        y: (shape.y / 100) * size.height,
        w: (shape.w / 100) * size.width,
        h: (shape.h / 100) * size.height,
        objectName: objectName(shape, index)
      };
      if (shape.kind === 'image') {
        if (shape.src) out.addImage({ data: shape.src, altText: shape.alt || '', ...box });
        return;
      }
      const style = resolvedStyle(shape, deck.theme);
      const lines = shape.text.split('\n');
      out.addText(
        lines.map((line, i) => ({
          text: line,
          options: {
            breakLine: i < lines.length - 1,
            ...(shape.style.bullets && line.trim() ? { bullet: true } : {})
          }
        })),
        {
          ...box,
          ...(shape.kind === 'shape' ? { shape: pptx.ShapeType.rect } : {}),
          fontFace: style.font,
          fontSize: style.size,
          color: hex(style.color),
          bold: !!shape.style.bold,
          italic: !!shape.style.italic,
          ...(shape.style.underline ? { underline: { style: 'sng' as const } } : {}),
          align: style.align,
          valign: style.valign,
          ...(style.fill ? { fill: { color: hex(style.fill) } } : {})
        }
      );
    });
  }
  return (await pptx.write({ outputType: 'uint8array' })) as Uint8Array<ArrayBuffer>;
}
