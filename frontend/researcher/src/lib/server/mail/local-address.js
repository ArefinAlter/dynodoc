// Which addresses count as "this machine or a private network", for SMTP_SECURITY=none-local.
// Pure and dependency-free (node:net only): the configuration module loads it too.
import { BlockList, isIP } from 'node:net';

const PRIVATE = new BlockList();
for (const [network, prefix] of /** @type {[string, number][]} */ ([
  ['127.0.0.0', 8],
  ['10.0.0.0', 8],
  ['172.16.0.0', 12],
  ['192.168.0.0', 16],
  ['169.254.0.0', 16],
  ['100.64.0.0', 10]
]))
  PRIVATE.addSubnet(network, prefix, 'ipv4');
PRIVATE.addAddress('::1', 'ipv6');
PRIVATE.addSubnet('fc00::', 7, 'ipv6');
PRIVATE.addSubnet('fe80::', 10, 'ipv6');

/**
 * Loopback, private (RFC 1918, IPv6 unique-local), link-local and shared (100.64.0.0/10)
 * addresses. A host name is never local by this test: resolve it first.
 * @param {string} address
 */
export function isLocalAddress(address) {
  const family = isIP(address);
  if (family === 0) return false;
  const mapped = /^::ffff:(\d+\.\d+\.\d+\.\d+)$/i.exec(address);
  if (mapped) return PRIVATE.check(mapped[1], 'ipv4');
  return PRIVATE.check(address, family === 4 ? 'ipv4' : 'ipv6');
}
