import { DOMParser } from '@xmldom/xmldom';
import { ulid } from 'ulid';
import { readOfficeZip, xmlText } from './bounded-zip';
import {
  deckOps,
  inferLayout,
  MAX_IMAGE_BYTES,
  round,
  SLIDE_HEIGHT_PT,
  type ImportedSlide,
  type ShapeFields,
  type ShapeRole,
  type ShapeStyle,
  type SlideLayout,
  type SlideSize
} from './presentation';
import type { ImportResult } from './files';

type X = NonNullable<ReturnType<DOMParser['parseFromString']>['documentElement']>;
interface Box {
  x: number;
  y: number;
  w: number;
  h: number;
}
interface Placeholder {
  type: string;
  idx: string;
  box?: Box;
  lvl1?: X;
}
interface Part {
  path: string;
  root: X;
  rels: Map<string, { target: string; type: string; external: boolean }>;
}
const children = (e: X | null | undefined, tag?: string): X[] =>
  e
    ? Array.from(e.childNodes).filter(
        (n): n is X => n.nodeType === 1 && (!tag || n.localName === tag)
      )
    : [];
const child = (e: X | null | undefined, tag: string) => children(e, tag)[0];
const path = (e: X | null | undefined, ...tags: string[]) =>
  tags.reduce<X | undefined>((node, tag) => child(node, tag), e || undefined);
const descendants = (e: X | null | undefined, name: string): X[] =>
  e ? Array.from(e.getElementsByTagName('*')).filter((n) => n.localName === name) : [];
const MAX_SLIDES = 500,
  MAX_SHAPES = 300,
  MAX_TEXT = 20000,
  MAX_MEDIA = 5 * 1024 * 1024;
const presetColors: Record<string, string> = {
  black: '#000000',
  white: '#ffffff',
  red: '#ff0000',
  green: '#008000',
  blue: '#0000ff',
  yellow: '#ffff00',
  gray: '#808080',
  orange: '#ffa500'
};
const layoutTypes: Record<string, SlideLayout> = {
  title: 'title',
  secHead: 'section',
  twoObj: 'two-content',
  twoTxTwoObj: 'two-content',
  twoColTx: 'two-content',
  blank: 'blank',
  obj: 'title-content',
  tx: 'title-content',
  titleOnly: 'title-content'
};

function resolve(base: string, target: string) {
  if (target.startsWith('/')) return target.slice(1);
  const parts = base.split('/').slice(0, -1);
  for (const piece of target.split('/')) {
    if (piece === '..') parts.pop();
    else if (piece && piece !== '.') parts.push(piece);
  }
  return parts.join('/');
}
const relsPath = (part: string) => {
  const i = part.lastIndexOf('/');
  return `${part.slice(0, i)}/_rels/${part.slice(i + 1)}.rels`;
};
const roleOf = (type: string, idx: string): ShapeRole | null | undefined =>
  type === 'title' || type === 'ctrTitle'
    ? 'title'
    : type === 'subTitle'
      ? 'subtitle'
      : type === 'body' || type === 'obj' || (!type && idx)
        ? 'body'
        : ['dt', 'ftr', 'sldNum', 'hdr'].includes(type)
          ? null
          : undefined;
/** Master placeholders are matched by broad kind; layouts and slides by index first. */
const family = (type: string) =>
  type === 'title' || type === 'ctrTitle'
    ? 'title'
    : ['dt', 'ftr', 'sldNum', 'hdr'].includes(type)
      ? type
      : 'body';

/** Reads the supported PPTX subset into Dynodoc slide nodes, reporting what is not kept. */
export function importPptx(bytes: Uint8Array, filename: string): ImportResult {
  const entries = readOfficeZip(bytes, (name) =>
    /^ppt\/(presentation\.xml|_rels\/presentation\.xml\.rels|(slides|slideLayouts|slideMasters|notesSlides)\/(_rels\/)?[^/]+\.xml(\.rels)?|theme\/theme\d+\.xml|media\/[^/]+)$/.test(
      name
    )
  );
  const warnings = new Set<string>();
  const parse = (name: string) => {
    const source = xmlText(entries.get(name));
    if (!source) return undefined;
    return new DOMParser({
      onError: (level, message) => {
        if (level !== 'warning')
          throw new Error(`Invalid PowerPoint XML: ${message.slice(0, 120)}`);
      }
    }).parseFromString(source, 'application/xml').documentElement!;
  };
  const cache = new Map<string, Part | undefined>();
  const part = (name: string): Part | undefined => {
    if (cache.has(name)) return cache.get(name);
    const root = parse(name);
    const rels = new Map(
      children(parse(relsPath(name))).map((r) => [
        r.getAttribute('Id') || '',
        {
          target: resolve(name, r.getAttribute('Target') || ''),
          type: r.getAttribute('Type') || '',
          external: r.getAttribute('TargetMode') === 'External'
        }
      ])
    );
    const result = root ? { path: name, root, rels } : undefined;
    cache.set(name, result);
    return result;
  };
  const related = (p: Part | undefined, suffix: string) =>
    [...(p?.rels.values() || [])].find((r) => r.type.endsWith(suffix))?.target;

  const presentation = part('ppt/presentation.xml');
  if (!presentation || presentation.root.localName !== 'presentation')
    throw new Error('This is not a readable PowerPoint (.pptx) presentation.');
  const sldSz = child(presentation.root, 'sldSz');
  const cx = Number(sldSz?.getAttribute('cx')) || 12192000,
    cy = Number(sldSz?.getAttribute('cy')) || 6858000;
  const ratio = cx / cy;
  const size: SlideSize = Math.abs(ratio - 4 / 3) < Math.abs(ratio - 16 / 9) ? '4:3' : '16:9';
  if (Math.min(Math.abs(ratio - 4 / 3), Math.abs(ratio - 16 / 9)) > 0.05)
    warnings.add(`The original slide size is shown at the nearest supported ratio (${size}).`);
  // Keep text proportional to slide height: Dynodoc slides are 540 pt tall.
  const scale = SLIDE_HEIGHT_PT / (cy / 12700);
  const defaultStyle = path(presentation.root, 'defaultTextStyle', 'lvl1pPr');

  const firstMaster = part(
    related(presentation, '/slideMaster') || 'ppt/slideMasters/slideMaster1.xml'
  );
  const themeXml = part(related(firstMaster, '/theme') || 'ppt/theme/theme1.xml');
  const palette: Record<string, string> = {};
  for (const c of children(path(themeXml?.root, 'themeElements', 'clrScheme'))) {
    const value =
      child(c, 'srgbClr')?.getAttribute('val') || child(c, 'sysClr')?.getAttribute('lastClr');
    if (value && /^[0-9a-f]{6}$/i.test(value))
      palette[c.localName || ''] = `#${value.toLowerCase()}`;
  }
  const clrMap = child(firstMaster?.root, 'clrMap');
  for (const [alias, fallback] of [
    ['tx1', 'dk1'],
    ['bg1', 'lt1'],
    ['tx2', 'dk2'],
    ['bg2', 'lt2']
  ])
    palette[alias] = palette[clrMap?.getAttribute(alias) || fallback] || palette[fallback];
  const colorValue = (e: X | undefined): string | undefined => {
    if (!e) return;
    const srgb = child(e, 'srgbClr')?.getAttribute('val');
    if (srgb && /^[0-9a-f]{6}$/i.test(srgb)) return `#${srgb.toLowerCase()}`;
    const scheme = child(e, 'schemeClr')?.getAttribute('val');
    if (scheme) return palette[scheme === 'phClr' ? 'tx1' : scheme];
    const sys = child(e, 'sysClr')?.getAttribute('lastClr');
    if (sys && /^[0-9a-f]{6}$/i.test(sys)) return `#${sys.toLowerCase()}`;
    return presetColors[child(e, 'prstClr')?.getAttribute('val') || ''];
  };
  const solid = (e: X | undefined) => colorValue(child(e, 'solidFill'));
  const background = (p: Part | undefined): string | undefined => {
    const bg = path(p?.root, 'cSld', 'bg');
    if (!bg) return;
    const props = child(bg, 'bgPr');
    const color = props ? solid(props) : colorValue(child(bg, 'bgRef'));
    if (!color) warnings.add('Gradient and picture backgrounds are not reproduced.');
    return color;
  };
  const xfrmBox = (xfrm: X | undefined): Box | undefined => {
    const off = child(xfrm, 'off'),
      ext = child(xfrm, 'ext');
    if (!off || !ext) return;
    if (Number(xfrm?.getAttribute('rot'))) warnings.add('Rotated objects are shown unrotated.');
    return {
      x: Number(off.getAttribute('x')) || 0,
      y: Number(off.getAttribute('y')) || 0,
      w: Number(ext.getAttribute('cx')) || 0,
      h: Number(ext.getAttribute('cy')) || 0
    };
  };
  const placeholders = (p: Part | undefined): Placeholder[] =>
    descendants(path(p?.root, 'cSld', 'spTree'), 'sp').flatMap((sp) => {
      const ph = path(sp, 'nvSpPr', 'nvPr', 'ph');
      if (!ph) return [];
      return [
        {
          type: ph.getAttribute('type') || '',
          idx: ph.getAttribute('idx') || '',
          box: xfrmBox(path(sp, 'spPr', 'xfrm')),
          lvl1: path(sp, 'txBody', 'lstStyle', 'lvl1pPr')
        }
      ];
    });
  const masters = new Map<string, { list: Placeholder[]; styles: X | undefined }>();
  const layoutsCache = new Map<
    string,
    { list: Placeholder[]; master?: string; type: string; bg?: string }
  >();
  function layoutInfo(name: string | undefined) {
    if (!name) return undefined;
    if (!layoutsCache.has(name)) {
      const layout = part(name);
      const master = related(layout, '/slideMaster');
      if (master && !masters.has(master)) {
        const m = part(master);
        masters.set(master, {
          list: placeholders(m),
          styles: child(m?.root, 'txStyles')
        });
      }
      layoutsCache.set(name, {
        list: placeholders(layout),
        master,
        type: layout?.root.getAttribute('type') || '',
        bg: background(layout) || background(part(master || ''))
      });
    }
    return layoutsCache.get(name);
  }

  let mediaTotal = 0;
  function image(p: Part, pic: X): string | undefined {
    const blip = descendants(child(pic, 'blipFill'), 'blip')[0];
    const rel = p.rels.get(blip?.getAttribute('r:embed') || '');
    if (!rel || rel.external) {
      warnings.add('Linked or unsupported pictures are not reproduced.');
      return;
    }
    const data = entries.get(rel.target),
      ext = rel.target.split('.').pop()?.toLowerCase() || '';
    if (
      !data ||
      !['png', 'jpg', 'jpeg', 'gif', 'webp'].includes(ext) ||
      data.length > MAX_IMAGE_BYTES ||
      mediaTotal + data.length > MAX_MEDIA
    ) {
      warnings.add(
        'Pictures over 1 MB, beyond 5 MB in total or in formats other than PNG, JPEG, GIF and WebP are not reproduced.'
      );
      return;
    }
    mediaTotal += data.length;
    let binary = '';
    for (let i = 0; i < data.length; i += 8192)
      binary += String.fromCharCode(...data.subarray(i, i + 8192));
    return `data:image/${ext === 'jpg' ? 'jpeg' : ext};base64,${btoa(binary)}`;
  }

  const slides: ImportedSlide[] = [];
  const slideIds = children(child(presentation.root, 'sldIdLst'), 'sldId');
  if (slideIds.length > MAX_SLIDES)
    throw new Error(`This workspace imports up to ${MAX_SLIDES} slides per presentation.`);
  for (const sldId of slideIds) {
    const target = presentation.rels.get(sldId.getAttribute('r:id') || '')?.target;
    const slide = target ? part(target) : undefined;
    if (!slide) continue;
    const layout = layoutInfo(related(slide, '/slideLayout'));
    const master = masters.get(layout?.master || '');
    if (child(slide.root, 'transition')) warnings.add('Slide transitions are not reproduced.');
    if (child(slide.root, 'timing')) warnings.add('Animations are not reproduced.');
    const shapes: ShapeFields[] = [];
    const toPercent = (b: Box): Pick<ShapeFields, 'x' | 'y' | 'w' | 'h'> => ({
      x: round((b.x / cx) * 100),
      y: round((b.y / cy) * 100),
      w: round(Math.max(0.5, (b.w / cx) * 100)),
      h: round(Math.max(0.5, (b.h / cy) * 100))
    });
    const inherited = (ph: X | undefined) => {
      if (!ph) return { chain: [defaultStyle] as (X | undefined)[] };
      const type = ph.getAttribute('type') || '',
        idx = ph.getAttribute('idx') || '';
      const fromLayout =
        layout?.list.find((p) => idx && p.idx === idx) ||
        layout?.list.find((p) => p.type === type || (!type && !p.type && p.idx));
      const fromMaster = master?.list.find(
        (p) => family(p.type) === family(fromLayout?.type ?? type)
      );
      const kind = family(fromLayout?.type ?? type);
      const styles = child(
        master?.styles,
        kind === 'title' ? 'titleStyle' : kind === 'body' ? 'bodyStyle' : 'otherStyle'
      );
      return {
        box: fromLayout?.box || fromMaster?.box,
        chain: [fromLayout?.lvl1, fromMaster?.lvl1, child(styles, 'lvl1pPr')]
      };
    };
    function textShape(sp: X, box: Box | undefined, role: ShapeRole | undefined, ph?: X) {
      const body = child(sp, 'txBody');
      const base = inherited(ph);
      const chain = [path(body, 'lstStyle', 'lvl1pPr'), ...base.chain];
      const pick = <T>(read: (e: X) => T | undefined) => {
        for (const e of chain) {
          const v = e ? read(e) : undefined;
          if (v !== undefined) return v;
        }
      };
      const paragraphs = children(body, 'p');
      const lines = paragraphs.map((p) =>
        children(p)
          .map((n) =>
            n.localName === 'r' || n.localName === 'fld'
              ? child(n, 't')?.textContent || ''
              : n.localName === 'br'
                ? '\n'
                : ''
          )
          .join('')
      );
      if (paragraphs.some((p) => Number(child(p, 'pPr')?.getAttribute('lvl')) > 0))
        warnings.add('Nested bullet levels are flattened to one level.');
      let text = lines.join('\n');
      if (text.length > MAX_TEXT) {
        text = text.slice(0, MAX_TEXT);
        warnings.add('Very long text boxes are shortened; consult the original file.');
      }
      const firstIndex = Math.max(
        0,
        lines.findIndex((l) => l.trim())
      );
      const first = paragraphs[firstIndex];
      const pPr = child(first, 'pPr');
      const run = children(first).find(
        (n) => (n.localName === 'r' || n.localName === 'fld') && child(n, 't')?.textContent
      );
      const rPr = child(run, 'rPr');
      const bulletOf = (e: X) =>
        child(e, 'buNone') ? false : child(e, 'buChar') || child(e, 'buAutoNum') ? true : undefined;
      const style: ShapeStyle = {};
      const fontScale =
        Number(path(body, 'bodyPr', 'normAutofit')?.getAttribute('fontScale')) || 100000;
      const sz =
        Number(rPr?.getAttribute('sz')) ||
        Number(pick((e) => child(e, 'defRPr')?.getAttribute('sz') || undefined)) ||
        (role === 'title' ? 4400 : role ? 2800 : 1800);
      style.size = Math.max(6, Math.round(((sz / 100) * scale * fontScale) / 100000));
      const flag = (name: string) => {
        const own = rPr?.getAttribute(name);
        const value = own ?? pick((e) => child(e, 'defRPr')?.getAttribute(name) || undefined);
        return value === '1' || value === 'true';
      };
      if (flag('b')) style.bold = true;
      if (flag('i')) style.italic = true;
      const underline =
        rPr?.getAttribute('u') ?? pick((e) => child(e, 'defRPr')?.getAttribute('u') || undefined);
      if (underline && underline !== 'none') style.underline = true;
      const color = solid(rPr) ?? pick((e) => solid(child(e, 'defRPr')));
      if (color) style.color = color;
      const algn = pPr?.getAttribute('algn') || pick((e) => e.getAttribute('algn') || undefined);
      if (algn === 'ctr') style.align = 'center';
      else if (algn === 'r') style.align = 'right';
      else if (algn === 'l' || algn === 'just') style.align = 'left';
      const bullets = (pPr && bulletOf(pPr)) ?? pick(bulletOf) ?? role === 'body';
      if (bullets) style.bullets = true;
      const fill = solid(child(sp, 'spPr'));
      const geometry = path(sp, 'spPr', 'prstGeom')?.getAttribute('prst');
      const kind = !ph && fill ? 'shape' : 'text';
      if (kind === 'shape') style.fill = fill;
      if (kind === 'shape' && geometry && geometry !== 'rect')
        warnings.add('Shapes other than rectangles are shown as rectangles.');
      if (!box || (!text.trim() && !role && kind === 'text')) return;
      shapes.push({ kind, ...(role ? { role } : {}), text, ...toPercent(box), style });
    }
    function walk(nodes: X[], map: (b: Box) => Box) {
      for (const node of nodes) {
        if (shapes.length >= MAX_SHAPES) {
          warnings.add(`Only the first ${MAX_SHAPES} objects on a slide are imported.`);
          return;
        }
        const name = node.localName;
        if (name === 'sp') {
          const ph = path(node, 'nvSpPr', 'nvPr', 'ph');
          const role = ph
            ? roleOf(ph.getAttribute('type') || '', ph.getAttribute('idx') || '')
            : undefined;
          if (role === null) {
            warnings.add('Footers, dates and slide numbers are not reproduced.');
            continue;
          }
          const own = xfrmBox(path(node, 'spPr', 'xfrm'));
          const box = own ? map(own) : ph ? inherited(ph).box : undefined;
          const nameAttr = path(node, 'nvSpPr', 'cNvPr')?.getAttribute('name') || '';
          const named = /^Title\b/i.test(nameAttr)
            ? 'title'
            : /^Subtitle\b/i.test(nameAttr)
              ? 'subtitle'
              : /^(Content|Body)\b/i.test(nameAttr)
                ? 'body'
                : undefined;
          textShape(node, box, role || (ph ? undefined : named), ph);
        } else if (name === 'pic') {
          const own = xfrmBox(path(node, 'spPr', 'xfrm'));
          const ph = path(node, 'nvPicPr', 'nvPr', 'ph');
          const box = own ? map(own) : ph ? inherited(ph).box : undefined;
          const src = image(slide!, node);
          const alt = path(node, 'nvPicPr', 'cNvPr')?.getAttribute('descr') || '';
          if (src && box)
            shapes.push({ kind: 'image', text: '', ...toPercent(box), style: {}, src, alt });
        } else if (name === 'grpSp') {
          warnings.add('Grouped objects are imported as separate objects.');
          const xfrm = path(node, 'grpSpPr', 'xfrm');
          const own = xfrmBox(xfrm),
            chOff = child(xfrm, 'chOff'),
            chExt = child(xfrm, 'chExt');
          const ox = Number(chOff?.getAttribute('x')) || 0,
            oy = Number(chOff?.getAttribute('y')) || 0;
          const sx =
            own && Number(chExt?.getAttribute('cx'))
              ? own.w / Number(chExt!.getAttribute('cx'))
              : 1;
          const sy =
            own && Number(chExt?.getAttribute('cy'))
              ? own.h / Number(chExt!.getAttribute('cy'))
              : 1;
          walk(children(node), (b) =>
            map(
              own
                ? {
                    x: own.x + (b.x - ox) * sx,
                    y: own.y + (b.y - oy) * sy,
                    w: b.w * sx,
                    h: b.h * sy
                  }
                : b
            )
          );
        } else if (name === 'graphicFrame')
          warnings.add('Tables, charts, SmartArt and embedded objects are not reproduced.');
        else if (name === 'cxnSp') warnings.add('Connector lines are not reproduced.');
        else if (name === 'AlternateContent') walk(children(child(node, 'Fallback')), map);
      }
    }
    walk(children(path(slide.root, 'cSld', 'spTree')), (b) => b);
    const notesPart = part(related(slide, '/notesSlide') || '');
    const notesBody = descendants(path(notesPart?.root, 'cSld', 'spTree'), 'sp').find(
      (sp) => path(sp, 'nvSpPr', 'nvPr', 'ph')?.getAttribute('type') === 'body'
    );
    const notes = children(child(notesBody, 'txBody'), 'p')
      .map((p) =>
        descendants(p, 't')
          .map((t) => t.textContent || '')
          .join('')
      )
      .join('\n')
      .trim()
      .slice(0, MAX_TEXT);
    const bg = background(slide) || layout?.bg;
    slides.push({
      layout: layoutTypes[layout?.type || ''] || inferLayout(shapes),
      notes,
      ...(bg ? { background: bg } : {}),
      hidden: slide.root.getAttribute('show') === '0',
      shapes
    });
  }
  if (!slides.length) throw new Error('This presentation has no readable slides.');
  warnings.add(
    'Slide text, placeholders, pictures, solid colours, speaker notes and hidden slides are imported. Masters, themes, custom fonts and mixed formatting within one text box are simplified; the original file remains available.'
  );
  const rootId = ulid();
  const ops = deckOps(filename.replace(/\.[^.]+$/, ''), size, slides, rootId);
  if (ops.length > 20000)
    throw new Error(
      'This presentation has too many objects to import. Split it into smaller files.'
    );
  return { kind: 'presentation', ops, warnings: [...warnings], filename, rootId };
}
