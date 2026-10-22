// The Office add-in manifest generator (docs/SELF-HOSTED-EDITION.md section 7.2, P0-7).
// Pure: the same input always gives the same bytes, and nothing here reads a file or the
// environment, so the web server's manifest route, the offline builder
// (scripts/build-addins.mjs) and the tests all produce identical manifests.
//
// Fed the hosted service's values, it reproduces the committed extensions/<host>/manifest.xml
// byte for byte (manifest.test.ts pins that), so the template cannot drift from what the
// hosted service actually serves.
//
// Two variants of each manifest exist:
//   standard  the manifest in extensions/: a ribbon group through VersionOverrides.
//   catalog   the same add-in without VersionOverrides, for SharePoint app catalogs and
//             trusted add-in catalogs, which do not support it (section 7.4). It has no
//             ribbon button: the pane opens from Home > Add-ins > More Settings > My Organization.
import { requireAddinOrigin } from './origin.js';
import { xml } from './xml.js';
import { ADDIN_NAMESPACE, isUuid, uuidV5 } from './uuid.js';

export const OFFICE_KINDS = /** @type {const} */ (['word', 'excel', 'powerpoint']);
export const OFFICE_VARIANTS = /** @type {const} */ (['standard', 'catalog']);

/** @typedef {(typeof OFFICE_KINDS)[number]} OfficeKind */
/** @typedef {(typeof OFFICE_VARIANTS)[number]} OfficeVariant */

/** Office's limits for the strings below (Microsoft Learn, manifest element reference). */
export const LIMITS = Object.freeze({
  displayName: 125,
  description: 250,
  providerName: 256,
  shortString: 125,
  longString: 250
});
export const BRAND_MAX = 60;
export const SUFFIX_MAX = 30;
export const SUPPORT_URL_MAX = 300;

const KINDS = Object.freeze({
  word: {
    app: 'Word',
    host: 'Document',
    set: 'WordApi',
    minVersion: '1.3',
    noun: 'document',
    edits: 'existing paragraphs'
  },
  excel: {
    app: 'Excel',
    host: 'Workbook',
    set: 'ExcelApi',
    minVersion: '1.13',
    noun: 'workbook',
    edits: 'existing cell values and formulas'
  },
  powerpoint: {
    app: 'PowerPoint',
    host: 'Presentation',
    set: 'PowerPointApi',
    minVersion: '1.4',
    noun: 'presentation',
    edits: 'existing slide text'
  }
});

/** @param {unknown} kind @returns {kind is OfficeKind} */
export const isOfficeKind = (kind) => typeof kind === 'string' && kind in KINDS;
/** @param {unknown} variant @returns {variant is OfficeVariant} */
export const isOfficeVariant = (variant) => variant === 'standard' || variant === 'catalog';

/**
 * The add-in id for one installation, host and variant. Derived from the origin so that it
 * stays the same every time the manifest is generated (Office treats a changed id as a
 * different add-in), and so that two installations never share one.
 * @param {string} origin a validated origin
 * @param {OfficeKind} kind
 * @param {OfficeVariant} [variant]
 */
export function addinGuid(origin, kind, variant = 'standard') {
  return uuidV5(`dynodoc-office-addin\n${origin}\n${kind}\n${variant}`, ADDIN_NAMESPACE);
}

/**
 * The manifest `Version` for an application version: `1.<major>.<minor>.<patch>`. Raising
 * the application version raises it, which is how Office is made to replace a cached copy
 * (section 7.7); the leading 1 is the manifest format generation.
 * @param {string} appVersion a semantic version such as 0.1.0 (a pre-release suffix is ignored)
 */
export function manifestVersion(appVersion) {
  const match = /^(\d{1,5})\.(\d{1,5})\.(\d{1,5})(?:[-+][0-9A-Za-z.+-]*)?$/.exec(appVersion);
  if (!match || match.slice(1).some((part) => Number(part) > 65535))
    throw new RangeError(`The application version "${appVersion}" is not major.minor.patch.`);
  return `1.${Number(match[1])}.${Number(match[2])}.${Number(match[3])}`;
}

const VERSION = /^(\d{1,5})(?:\.(\d{1,5})){3}$/;
/** Control, line-separator and text-direction characters (compared by code point). */
const hasControl = (/** @type {string} */ text) =>
  [...text].some((char) => {
    const code = char.codePointAt(0) ?? 0;
    return (
      code < 0x20 ||
      (code >= 0x7f && code <= 0x9f) ||
      code === 0x2028 ||
      code === 0x2029 ||
      (code >= 0x202a && code <= 0x202e) ||
      (code >= 0x2066 && code <= 0x2069)
    );
  });

/**
 * One line of plain text.
 * @param {unknown} value @param {string} what @param {number} max
 */
function plainLine(value, what, max) {
  if (typeof value !== 'string' || value.trim() === '' || value !== value.trim())
    throw new RangeError(`${what} must be text with no space at either end.`);
  if ([...value].length > max) throw new RangeError(`${what} is longer than ${max} characters.`);
  if (hasControl(value)) throw new RangeError(`${what} must be a single line of plain text.`);
  return value;
}

/**
 * The support link: an https address of at most SUPPORT_URL_MAX characters without
 * credentials, as the URL parser writes it. Anything else (a mailto: link, http, a user name)
 * falls back to the installation's own address.
 * @param {unknown} value @param {string} fallback
 */
function supportAddress(value, fallback) {
  if (typeof value !== 'string' || value === '') return fallback;
  try {
    const url = new URL(value);
    if (
      url.protocol !== 'https:' ||
      url.username ||
      url.password ||
      url.href.length > SUPPORT_URL_MAX ||
      hasControl(url.href)
    )
      return fallback;
    return url.href;
  } catch {
    return fallback;
  }
}

/**
 * @typedef {object} OfficeManifestInput
 * @property {string} origin the installation's origin (https; http only with allowLoopbackHttp)
 * @property {string} brand the product name people see; replaces "Dynodoc" in every string
 * @property {OfficeKind} kind
 * @property {OfficeVariant} [variant] default standard
 * @property {string} version four numbers, such as 1.0.1.0 (see manifestVersion)
 * @property {string} [guid] default: derived from origin, kind and variant (addinGuid)
 * @property {string} [provider] the provider shown in the add-in details; default: brand
 * @property {string} [supportUrl] an https address; default: the installation's origin
 * @property {string} [displaySuffix] appended to the name, to tell staging from production
 * @property {boolean} [allowLoopbackHttp] accept an http://localhost origin (test stacks)
 * @property {string | null} [forbidDomain] refuse origins under this domain
 */

/**
 * "a" or "an" before a product name, by how the name starts. A heuristic for English names: a
 * vowel letter takes "an" except where it is sounded as a consonant (Unity, Euro, Use), and a
 * few silent-h words take "an".
 * @param {string} brand
 */
export function articleFor(brand) {
  if (/^(uni|use|usu|eu|one|once)/i.test(brand)) return 'a';
  if (/^(hour|honest|honou?r|heir)/i.test(brand)) return 'an';
  return /^[aeiou]/i.test(brand) ? 'an' : 'a';
}

/**
 * The description, in its full form when it fits Office's 250 characters and in a shorter
 * form for a long product name.
 * @param {typeof KINDS[OfficeKind]} k @param {string} brand
 */
function describe(k, brand) {
  const a = articleFor(brand);
  const full = `Connect this ${k.noun} to ${a} ${brand} project. ${brand} records edits to ${k.edits} while this pane is open, sends them for review, and brings approved changes from your team back into the ${k.noun}.`;
  if ([...full].length <= LIMITS.description) return full;
  return `Connect this ${k.noun} to ${a} ${brand} project. It records edits to ${k.edits} while this pane is open, sends them for review, and brings approved changes back.`;
}

/**
 * Render one manifest.
 * @param {OfficeManifestInput} input
 * @returns {string} the manifest, UTF-8 text with LF line ends and a final line end
 */
export function renderOfficeManifest(input) {
  const { kind, variant = 'standard' } = input;
  if (!isOfficeKind(kind)) throw new RangeError(`Unknown add-in "${String(kind)}".`);
  if (!isOfficeVariant(variant)) throw new RangeError(`Unknown manifest variant "${variant}".`);
  const k = KINDS[kind];
  const origin = requireAddinOrigin(input.origin, {
    allowLoopbackHttp: input.allowLoopbackHttp === true,
    forbidDomain: input.forbidDomain ?? null
  });
  const brand = plainLine(input.brand, 'The product name', BRAND_MAX);
  const provider = plainLine(input.provider ?? brand, 'The provider name', LIMITS.providerName);
  const suffix =
    input.displaySuffix === undefined || input.displaySuffix === ''
      ? ''
      : ` ${plainLine(input.displaySuffix, 'The name suffix', SUFFIX_MAX)}`;
  if (typeof input.version !== 'string' || !VERSION.test(input.version))
    throw new RangeError('The version must be four numbers such as 1.0.1.0.');
  if (input.version.split('.').some((part) => Number(part) > 65535))
    throw new RangeError('Each part of the version must be 65535 or less.');
  const guid = input.guid ?? addinGuid(origin, kind, variant);
  if (!isUuid(guid)) throw new RangeError('The add-in id must be a lower-case UUID.');
  const support = supportAddress(input.supportUrl, `${origin}/`);

  const displayName = `${brand} for ${k.app}${suffix}`;
  const description = describe(k, brand);
  const openDescription = `Sign in to connect this file to ${articleFor(brand)} ${brand} project, send your changes for review and bring in the changes your team approved.`;
  if ([...displayName].length > LIMITS.displayName)
    throw new RangeError(`The add-in name is longer than ${LIMITS.displayName} characters.`);
  if ([...description].length > LIMITS.description)
    throw new RangeError(`The description is longer than ${LIMITS.description} characters.`);
  if ([...openDescription].length > LIMITS.longString)
    throw new RangeError(`The command description is longer than ${LIMITS.longString} characters.`);
  if ([...brand].length > LIMITS.shortString)
    throw new RangeError(`The product name is longer than ${LIMITS.shortString} characters.`);

  const head = xml`<?xml version="1.0" encoding="UTF-8"?>
<OfficeApp xmlns="http://schemas.microsoft.com/office/appforoffice/1.1" xmlns:bt="http://schemas.microsoft.com/office/officeappbasictypes/1.0" xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance" xsi:type="TaskPaneApp">
  <Id>${guid}</Id>
  <Version>${input.version}</Version>
  <ProviderName>${provider}</ProviderName>
  <DefaultLocale>en-US</DefaultLocale>
  <DisplayName DefaultValue="${displayName}" />
  <Description DefaultValue="${description}" />
  <IconUrl DefaultValue="${origin}/connectors/assets/icon-32.png" />
  <HighResolutionIconUrl DefaultValue="${origin}/connectors/assets/icon-64.png" />
  <SupportUrl DefaultValue="${support}" />
  <AppDomains><AppDomain>${origin}</AppDomain></AppDomains>
  <Hosts><Host Name="${k.host}" /></Hosts>
  <Requirements><Sets DefaultMinVersion="${k.minVersion}"><Set Name="${k.set}" /></Sets></Requirements>
  <DefaultSettings><SourceLocation DefaultValue="${origin}/connectors/${kind}" /></DefaultSettings>
  <Permissions>ReadWriteDocument</Permissions>
`;
  const ribbon = xml`  <VersionOverrides xmlns="http://schemas.microsoft.com/office/taskpaneappversionoverrides" xsi:type="VersionOverridesV1_0">
    <Hosts><Host xsi:type="${k.host}"><DesktopFormFactor>
      <FunctionFile resid="Commands.Url" />
      <ExtensionPoint xsi:type="PrimaryCommandSurface"><OfficeTab id="TabHome"><Group id="Dynodoc.Group">
        <Label resid="Group.Label" /><Icon><bt:Image size="16" resid="Icon.16" /><bt:Image size="32" resid="Icon.32" /><bt:Image size="80" resid="Icon.80" /></Icon>
        <Control xsi:type="Button" id="Dynodoc.Open"><Label resid="Open.Label" />
          <Supertip><Title resid="Open.Label" /><Description resid="Open.Description" /></Supertip>
          <Icon><bt:Image size="16" resid="Icon.16" /><bt:Image size="32" resid="Icon.32" /><bt:Image size="80" resid="Icon.80" /></Icon>
          <Action xsi:type="ShowTaskpane"><TaskpaneId>Dynodoc.Provenance</TaskpaneId><SourceLocation resid="Pane.Url" /></Action>
        </Control>
      </Group></OfficeTab></ExtensionPoint>
    </DesktopFormFactor></Host></Hosts>
    <Resources>
      <bt:Images><bt:Image id="Icon.16" DefaultValue="${origin}/connectors/assets/icon-16.png" /><bt:Image id="Icon.32" DefaultValue="${origin}/connectors/assets/icon-32.png" /><bt:Image id="Icon.80" DefaultValue="${origin}/connectors/assets/icon-80.png" /></bt:Images>
      <bt:Urls><bt:Url id="Commands.Url" DefaultValue="${origin}/connectors/commands.html" /><bt:Url id="Pane.Url" DefaultValue="${origin}/connectors/${kind}" /></bt:Urls>
      <bt:ShortStrings><bt:String id="Group.Label" DefaultValue="${brand}" /><bt:String id="Open.Label" DefaultValue="${brand}" /></bt:ShortStrings>
      <bt:LongStrings><bt:String id="Open.Description" DefaultValue="${openDescription}" /></bt:LongStrings>
    </Resources>
  </VersionOverrides>
`;
  return head + (variant === 'catalog' ? '' : ribbon) + '</OfficeApp>\n';
}

/** The file name a manifest is served and written under. @param {OfficeKind} kind @param {OfficeVariant} [variant] */
export const manifestFileName = (kind, variant = 'standard') =>
  variant === 'catalog' ? `${kind}-catalog.xml` : `${kind}.xml`;
