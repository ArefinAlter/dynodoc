// The operator of the hosted Dynodoc service and the public facts about it, in one place.
// Supplied by the owner on 13 October 2026; legal pages, the footer, structured data and
// security.txt read from here so a value is never retyped.
//
// Not supplied, so never invented: hosting provider and location, domain registrar and DNS
// provider, representative or DPO, log retention, effective date. Pages show UNCONFIRMED.
// Governing law and forum are the draft's proposed terms, pending counsel review.

export const OPERATOR = {
  legalName: 'Verne Systems',
  tradingAs: 'Verne',
  website: 'https://vernesystems.com',
  websiteLabel: 'vernesystems.com',
  address: '8/A, Sher-e-Bangla Road, Zighatola, Dhaka-1209, Bangladesh',
  addressParts: {
    streetAddress: '8/A, Sher-e-Bangla Road, Zighatola',
    addressLocality: 'Dhaka',
    postalCode: '1209',
    addressCountry: 'BD'
  },
  companyRegistration: 'C-201411-1',
  tradeLicence: 'TRAD/DSCC/035498/2024',
  email: 'info@vernesystems.com',
  phone: '+880 1870 034384',
  phoneHref: 'tel:+8801870034384',
  /** Proposed governing law and forum (draft, pending counsel review). */
  governingLaw: 'the laws of Bangladesh',
  forum: 'the courts of Dhaka'
} as const;

/** Shown wherever the owner has not yet provided a value. */
export const UNCONFIRMED = 'Awaiting confirmation.';

export const EMAIL_HREF = `mailto:${OPERATOR.email}`;

/** The one public repository: the MIT engine, HTTP API and command-line tools. */
export const ENGINE_REPO_URL = 'https://github.com/ArefinAlter/dynodoc-engine';
export const ENGINE_ISSUES_URL = `${ENGINE_REPO_URL}/issues`;
export const ENGINE_ADVISORY_URL = `${ENGINE_REPO_URL}/security/advisories/new`;

/** The sentence every page uses about openness; the workspace and website are not open source. */
export const OPEN_SOURCE_STATEMENT =
  'The engine, API and command-line tools are open source under the MIT licence. The workspace and website are not published as open source.';
