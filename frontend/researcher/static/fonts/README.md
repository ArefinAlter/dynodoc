# Public Sans

Public Sans, developed by the U.S. Web Design System, licensed under the SIL Open Font License (see OFL.txt).

Variable WOFF2 files retrieved from the Google Fonts distribution on 19 September 2026; self-hosted so readers make no requests to Google Fonts.

- Project: https://public-sans.digital.gov/
- License source: https://github.com/google/fonts/tree/main/ofl/publicsans

The editor also self-hosts Noto Sans Bengali, Noto Naskh Arabic, Noto Sans
Devanagari and Noto Sans Hebrew, downloaded from the corresponding `ofl/`
directories of https://github.com/google/fonts on 21 September 2026.
Each TTF is distributed under the SIL Open Font License 1.1; its adjacent
`*-OFL.txt` contains the copyright notices and license. These are script-specific
fallbacks loaded by the editors and print preview, without third-party font requests.
Microsoft font names in the toolbar use locally installed fonts; no Microsoft
font binaries are distributed. Users must have permission to upload custom fonts.

The public marketing site (`src/lib/styles/site.css`) self-hosts Geist and Geist
Mono, designed by Vercel with Basement Studio, under the SIL Open Font License 1.1
(`geist-OFL.txt`, `geist-mono-OFL.txt`; both fonts ship from the same
https://github.com/vercel/geist-font repository). The variable WOFF2 files
(latin and latin-ext subsets) were retrieved on 12 October 2026 from the
`@fontsource-variable/geist` and `@fontsource-variable/geist-mono` packages via
jsDelivr and checked for the `wOF2` signature. Characters outside those subsets
fall back to Public Sans. Readers make no third-party font requests.
