// A ZIP writer that stores files uncompressed. Stored entries make the output depend only on
// the input (no compressor version, no timestamps), so the same package has the same bytes and
// the same checksum on every machine; the packages are about 100 KB, so compression buys
// nothing worth giving that up. Plain JavaScript with no dependencies, so the offline builder
// runs on any machine that has Node.js.

const TABLE = (() => {
  const table = new Uint32Array(256);
  for (let n = 0; n < 256; n++) {
    let c = n;
    for (let k = 0; k < 8; k++) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1;
    table[n] = c >>> 0;
  }
  return table;
})();

/** CRC-32 (the ZIP and PNG polynomial). @param {Uint8Array} bytes */
export function crc32(bytes) {
  let crc = 0xffffffff;
  for (const byte of bytes) crc = TABLE[(crc ^ byte) & 0xff] ^ (crc >>> 8);
  return (crc ^ 0xffffffff) >>> 0;
}

export const ZIP_MAX_ENTRIES = 100;
const DOS_DATE_1980_01_01 = 0x0021;

/**
 * @param {{ name: string, data: string | Uint8Array }[]} entries in the order they are stored
 * @returns {Uint8Array}
 */
export function zipStored(entries) {
  if (entries.length > ZIP_MAX_ENTRIES) throw new RangeError('Too many files for one package.');
  const encoder = new TextEncoder();
  /** @type {Uint8Array[]} */ const local = [];
  /** @type {Uint8Array[]} */ const central = [];
  const seen = new Set();
  let offset = 0;
  for (const { name, data } of entries) {
    const nameBytes = encoder.encode(name);
    if (
      nameBytes.length === 0 ||
      nameBytes.length > 255 ||
      name.startsWith('/') ||
      name.includes('\\') ||
      name.split('/').some((part) => part === '' || part === '.' || part === '..')
    )
      throw new RangeError(`"${name}" is not a safe file name for a package.`);
    if (seen.has(name)) throw new RangeError(`"${name}" appears twice in the package.`);
    seen.add(name);
    const bytes = typeof data === 'string' ? encoder.encode(data) : data;
    const crc = crc32(bytes);
    const header = new DataView(new ArrayBuffer(30));
    header.setUint32(0, 0x04034b50, true);
    header.setUint16(4, 20, true); // version needed
    header.setUint16(6, 0x0800, true); // names are UTF-8
    header.setUint16(8, 0, true); // stored
    header.setUint16(10, 0, true); // time 00:00:00
    header.setUint16(12, DOS_DATE_1980_01_01, true);
    header.setUint32(14, crc, true);
    header.setUint32(18, bytes.length, true);
    header.setUint32(22, bytes.length, true);
    header.setUint16(26, nameBytes.length, true);
    header.setUint16(28, 0, true);
    local.push(new Uint8Array(header.buffer), nameBytes, bytes);
    const entry = new DataView(new ArrayBuffer(46));
    entry.setUint32(0, 0x02014b50, true);
    entry.setUint16(4, 20, true); // made by
    entry.setUint16(6, 20, true);
    entry.setUint16(8, 0x0800, true);
    entry.setUint16(10, 0, true);
    entry.setUint16(12, 0, true);
    entry.setUint16(14, DOS_DATE_1980_01_01, true);
    entry.setUint32(16, crc, true);
    entry.setUint32(20, bytes.length, true);
    entry.setUint32(24, bytes.length, true);
    entry.setUint16(28, nameBytes.length, true);
    entry.setUint32(42, offset, true);
    central.push(new Uint8Array(entry.buffer), nameBytes);
    offset += 30 + nameBytes.length + bytes.length;
  }
  const directorySize = central.reduce((sum, part) => sum + part.length, 0);
  const end = new DataView(new ArrayBuffer(22));
  end.setUint32(0, 0x06054b50, true);
  end.setUint16(8, entries.length, true);
  end.setUint16(10, entries.length, true);
  end.setUint32(12, directorySize, true);
  end.setUint32(16, offset, true);
  const parts = [...local, ...central, new Uint8Array(end.buffer)];
  const out = new Uint8Array(parts.reduce((sum, part) => sum + part.length, 0));
  let at = 0;
  for (const part of parts) {
    out.set(part, at);
    at += part.length;
  }
  return out;
}
