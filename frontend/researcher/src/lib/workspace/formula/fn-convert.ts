import { E } from './errors';
import { clean } from './values';
import { defScalar, num, str } from './fn-util';

/** CONVERT: unit table with SI and binary prefixes; unit names are case-sensitive. */
interface Unit {
  group: string;
  /** Size of one unit in the group's base unit. */
  factor: number;
  /** SI prefixes may be attached. */
  prefix?: boolean;
  binary?: boolean;
}
const units = new Map<string, Unit>();
function add(group: string, factor: number, names: string, prefix = false, binary = false) {
  for (const n of names.split(' ')) units.set(n, { group, factor, prefix, binary });
}
// mass (grams)
add('mass', 1, 'g', true);
add('mass', 14593.9029372064, 'sg');
add('mass', 453.59237, 'lbm');
add('mass', 1.660538782e-24, 'u', true);
add('mass', 28.349523125, 'ozm');
add('mass', 0.06479891, 'grain');
add('mass', 45359.237, 'cwt shweight');
add('mass', 50802.34544, 'uk_cwt lcwt hweight');
add('mass', 6350.29318, 'stone');
add('mass', 907184.74, 'ton');
add('mass', 1016046.9088, 'uk_ton LTON brton');
// length (metres)
add('length', 1, 'm', true);
add('length', 1609.344, 'mi');
add('length', 1852, 'Nmi');
add('length', 0.0254, 'in');
add('length', 0.3048, 'ft');
add('length', 0.9144, 'yd');
add('length', 1e-10, 'ang', true);
add('length', 1.143, 'ell');
add('length', 9.46073047258e15, 'ly');
add('length', 3.08567758149137e16, 'parsec pc');
add('length', 0.0254 / 72, 'Pica');
add('length', 0.0254 / 6, 'pica');
add('length', 1609.3472186944, 'survey_mi');
// time (seconds)
add('time', 31557600, 'yr');
add('time', 86400, 'day d');
add('time', 3600, 'hr');
add('time', 60, 'mn min');
add('time', 1, 'sec s', true);
// pressure (pascal)
add('pressure', 1, 'Pa p', true);
add('pressure', 101325, 'atm at', true);
add('pressure', 133.322, 'mmHg', true);
add('pressure', 6894.757293168, 'psi');
add('pressure', 133.322368421053, 'Torr');
// force (newton)
add('force', 1, 'N', true);
add('force', 1e-5, 'dyn dy', true);
add('force', 4.4482216152605, 'lbf');
add('force', 0.00980665, 'pond');
// energy (joule)
add('energy', 1, 'J', true);
add('energy', 1e-7, 'e', true);
add('energy', 4.184, 'c', true);
add('energy', 4.1868, 'cal', true);
add('energy', 1.602176487e-19, 'eV ev', true);
add('energy', 2684519.5376, 'HPh hh');
add('energy', 3600, 'Wh wh', true);
add('energy', 1.3558179483314, 'flb');
add('energy', 1055.05585262, 'BTU btu');
// power (watt)
add('power', 745.69987158227, 'HP h');
add('power', 735.49875, 'PS');
add('power', 1, 'W w', true);
// magnetism (tesla)
add('magnetism', 1, 'T', true);
add('magnetism', 1e-4, 'ga', true);
// volume (litres)
add('volume', 0.00492892159375, 'tsp');
add('volume', 0.01478676478125, 'tbs');
add('volume', 0.0295735295625, 'oz');
add('volume', 0.2365882365, 'cup');
add('volume', 0.473176473, 'pt us_pt');
add('volume', 0.56826125, 'uk_pt');
add('volume', 0.946352946, 'qt');
add('volume', 1.1365225, 'uk_qt');
add('volume', 3.785411784, 'gal');
add('volume', 4.54609, 'uk_gal');
add('volume', 1, 'l L lt', true);
add('volume', 1e-27, 'ang3 ang^3', true);
add('volume', 158.987294928, 'barrel');
add('volume', 35.23907016688, 'bushel');
add('volume', 2831.6846592, 'regton GRT');
add('volume', 1000, 'm3 m^3', true);
add('volume', 16.387064, 'in3 in^3');
add('volume', 28.316846592, 'ft3 ft^3');
add('volume', 764.554857984, 'yd3 yd^3');
// area (square metres)
add('area', 4046.8564224, 'uk_acre');
add('area', 4046.87260987, 'us_acre');
add('area', 1e-20, 'ang2 ang^2', true);
add('area', 100, 'ar', true);
add('area', 10000, 'ha');
add('area', 0.00064516, 'in2 in^2');
add('area', 0.09290304, 'ft2 ft^2');
add('area', 0.83612736, 'yd2 yd^2');
add('area', 2589988.110336, 'mi2 mi^2');
add('area', 3429904, 'Nmi2 Nmi^2');
add('area', 1, 'm2 m^2', true);
// speed (metres per second)
add('speed', 1, 'm/s m/sec', true);
add('speed', 1 / 3600, 'm/h m/hr', true);
add('speed', 0.44704, 'mph');
add('speed', 0.514444444444444, 'kn');
add('speed', 0.514773333333333, 'admkn');
// information (bits)
add('info', 1, 'bit', true, true);
add('info', 8, 'byte', true, true);

const TEMPERATURE = new Set(['C', 'cel', 'F', 'fah', 'K', 'kel', 'Rank', 'Reau']);
const SI: [string, number][] = [
  ['da', 1e1],
  ['Y', 1e24],
  ['Z', 1e21],
  ['E', 1e18],
  ['P', 1e15],
  ['T', 1e12],
  ['G', 1e9],
  ['M', 1e6],
  ['k', 1e3],
  ['h', 1e2],
  ['e', 1e1],
  ['d', 1e-1],
  ['c', 1e-2],
  ['m', 1e-3],
  ['u', 1e-6],
  ['n', 1e-9],
  ['p', 1e-12],
  ['f', 1e-15],
  ['a', 1e-18],
  ['z', 1e-21],
  ['y', 1e-24]
];
const BINARY: [string, number][] = [
  ['ki', 2 ** 10],
  ['Mi', 2 ** 20],
  ['Gi', 2 ** 30],
  ['Ti', 2 ** 40],
  ['Pi', 2 ** 50],
  ['Ei', 2 ** 60],
  ['Zi', 2 ** 70],
  ['Yi', 2 ** 80]
];

function lookup(name: string): { group: string; factor: number } | undefined {
  const direct = units.get(name);
  if (direct) return { group: direct.group, factor: direct.factor };
  for (const [p, scale] of [...BINARY, ...SI]) {
    if (!name.startsWith(p) || name.length === p.length) continue;
    const unit = units.get(name.slice(p.length));
    if (!unit || !unit.prefix) continue;
    if (BINARY.some(([b]) => b === p) && !unit.binary) continue;
    // the leading "m" of m/s-style names is the unit itself; SI prefix scales by the unit
    return { group: unit.group, factor: unit.factor * scale };
  }
  return undefined;
}
function toKelvin(v: number, unit: string): number {
  switch (unit) {
    case 'C':
    case 'cel':
      return v + 273.15;
    case 'F':
    case 'fah':
      return ((v + 459.67) * 5) / 9;
    case 'Rank':
      return (v * 5) / 9;
    case 'Reau':
      return v * 1.25 + 273.15;
    default:
      return v;
  }
}
function fromKelvin(k: number, unit: string): number {
  switch (unit) {
    case 'C':
    case 'cel':
      return k - 273.15;
    case 'F':
    case 'fah':
      return (k * 9) / 5 - 459.67;
    case 'Rank':
      return (k * 9) / 5;
    case 'Reau':
      return (k - 273.15) * 0.8;
    default:
      return k;
  }
}

defScalar('CONVERT', 3, 3, (a, ctx) => {
  const value = num(a[0], ctx);
  const from = str(a[1]),
    to = str(a[2]);
  if (TEMPERATURE.has(from) || TEMPERATURE.has(to)) {
    if (!TEMPERATURE.has(from) || !TEMPERATURE.has(to)) return E.NA;
    return clean(fromKelvin(toKelvin(value, from), to));
  }
  const x = lookup(from),
    y = lookup(to);
  if (!x || !y || x.group !== y.group) return E.NA;
  return clean((value * x.factor) / y.factor);
});
