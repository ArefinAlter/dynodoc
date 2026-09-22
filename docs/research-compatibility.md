# Research file compatibility

These are the implemented interchange paths. They do not imply full application parity or bidirectional synchronization.

| Tool / format | Implemented path | Limits to review |
|---|---|---|
| Word DOCX | Upload; text, headings, lists and tables into editable blocks; export DOCX | Common embedded images, text revisions and text note bodies are supported. Section layout, floating objects, note formatting and advanced fields do not round-trip fully. Original upload is retained. |
| Word DOC | Isolated LibreOffice conversion to DOCX, then the same importer | Requires converter container; conversion may fail on encrypted or damaged files. |
| Excel XLSX | Workbook sheets, values and formula strings; export XLSX | Common formatting, named ranges and rectangular same-sheet column/bar/line/pie charts are supported. Native pivot caches, macros, external connections and other charts remain unsupported. Formula coverage differs from Excel. |
| CSV | Import/export spreadsheet data | CSV has no styling, multi-sheet structure or typed schema. Export neutralizes spreadsheet formula injection. |
| ODK / KoboToolbox | XLSForm survey/choices/settings import and export; API project discovery, questionnaire source and response imports | Expressions and custom columns retained; validate in the destination tool. API imports are snapshots, with study-account validation pending. No automatic synchronization or writeback. |
| SurveyCTO | XLSForm questionnaire interchange; wide JSON API response import | Vendor-specific fields require destination validation; API responses are snapshots, without source-form discovery or writeback. |
| REDCap | Dictionary CSV import/export; project metadata and raw record API import | Source columns retained; native REDCap logic must be validated in REDCap. No incremental synchronization or writeback. |
| Google Forms | Separate read-only OAuth imports form definitions as questionnaires and responses as sheets | Editor form ID required; grids flatten; branching, quiz grading and media need review. One-hour connection; no synchronization/writeback. |
| LaTeX / Overleaf | Equation editing, `.tex`/source ZIP import, `.tex`/Overleaf project ZIP export | Unknown commands retained as source blocks; no TeX execution or live sync. Included files/media/bibliography stay in the original ZIP. Word export retains LaTeX source rather than native equation objects. |
| CSPro | Import CSV/XLSX data exported by CSPro | Does not parse CSPro dictionaries, forms, logic or CSDB databases. |
| SurveyMonkey | CSV/XLSX response files; survey discovery and bulk API response import | Bearer token supplied per connection. Nested answers retain source IDs as JSON columns; no survey-designer conversion or bidirectional synchronization. |

The demo accepts uploads up to 10 MB, up to 3,000 spreadsheet rows and 100 columns, and up to 5,000 operations in a batch. Review conversion notes before using an imported instrument for data collection. The preview executes the documented XLSForm expression subset, including relevance, calculations, required rules and constraints. Repeats, media capture and vendor-specific execution remain destination-platform checks.

The spreadsheet supports formulas through HyperFormula, cell formatting, copy/paste, search/filter, relative-formula sorting, workbook names, charts, refreshable pivot summaries, whole-sheet protection and multiple sheets. This is not a complete replacement for Google Sheets or Excel. The document editor provides tabs, context comments, checklists, equations and common text, paragraph, list, link, image-URL and table tools; advanced publishing and review tools still need work.

For API imports, see [all research connections and their limits](RESEARCH-CONNECTIONS.md)
and [ODK/Kobo setup](ODK-KOBO-SETUP.md). A format round-trip test does not establish that
every vendor-specific expression will execute correctly in its collection tool.

Reference specifications: [XLSForm](https://xlsform.org/en/), [CSPro exports](https://www.csprousers.org/help/CSExport/select_export_method.html), [SurveyMonkey API](https://api.surveymonkey.com/v3/docs), [REDCap dictionary guide](https://ws.engr.illinois.edu/sitemanager/getfile.asp?id=1365).
