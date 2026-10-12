# Marketplace assets

Store imagery for the six add-ins. Nothing here is a submission: no listing exists and
none has been approved. Owner-only steps are in
[`docs/listings/SUBMISSION-CHECKLIST.md`](../../docs/listings/SUBMISSION-CHECKLIST.md).

## Generated assets (committed)

`pnpm build:marketplace-assets` (in `frontend/researcher`) rasterises
`static/brand/mark.svg` with the installed Playwright Chromium and rewrites
`assets/*.png`; it also copies the Office sizes to
`static/connectors/assets/icon-<size>.png`, which the manifests link to. The script has
no dependencies beyond Playwright and writes the same bytes for the same inputs. Run
`pnpm exec vitest run src/lib/connectors` afterwards: it checks every file's pixel size
and that the served copies equal these files.

| File | Size | Used for |
| --- | --- | --- |
| `office-icon-16.png`, `office-icon-80.png` | 16, 80 | Ribbon button and group icons (manifest `bt:Image`) |
| `office-icon-32.png` | 32 | Manifest `IconUrl` and 32 px ribbon icon |
| `office-icon-64.png` | 64 | Manifest `HighResolutionIconUrl` |
| `office-store-logo-300.png` | 300 x 300 | Partner Center offer logo |
| `google-icon-128.png`, `google-icon-32.png` | 128, 32 | Marketplace SDK application icons (required) |
| `google-icon-96.png` | 96 | Only for a listing that also has a web app |
| `google-card-banner-220x140.png` | 220 x 140 | Marketplace card banner |
| `google-card-banner-440x280.png` | 440 x 280 | 2x of the card banner |

If the brand mark changes, regenerate, commit the PNGs, and treat the Office icons as a
manifest change (raise the manifest `Version`, redeploy the web build first).

## Screenshots are an owner step

The stores require screenshots that show the real product. They must be captured from
the add-ins running in installed hosts, with synthetic content only (Maya Chen, Sam
Okoye, Priya Nair; Board update, Q3 budget, Launch plan). They are deliberately not
generated here: a mock-up would misrepresent what a reviewer will see and could be
rejected as misleading.

Shot-lists, sizes and captions per host:
[word](../../docs/listings/word.md), [excel](../../docs/listings/excel.md),
[powerpoint](../../docs/listings/powerpoint.md),
[google-docs](../../docs/listings/google-docs.md),
[google-sheets](../../docs/listings/google-sheets.md),
[google-slides](../../docs/listings/google-slides.md).

Capture checklist:

1. Install the add-in from `extensions/INSTALL.md` in a real host, on the platform
   you will list. Use the acceptance fixtures from `extensions/ACCEPTANCE.md`.
2. Use a clean profile or window, light theme, 100% zoom, a window sized for the
   target (Microsoft: 1366 x 768 is commonly required; Google: 1280 x 800
   recommended, 640 x 400 and 2560 x 1600 accepted). Crop to the target size; do not
   scale up.
3. Remove anything personal: real names, email addresses, other open tabs, file paths,
   connection keys, approval codes that are still valid.
4. Save as PNG into a folder **outside the repository** (or a private one), named
   `<host>-<nn>-<slug>.png`. Do not commit store screenshots here unless they were
   captured from a real host and contain only synthetic content.
5. Upload in the store dashboards with the captions in the host files.
