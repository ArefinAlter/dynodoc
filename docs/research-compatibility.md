# Research file compatibility

These are the implemented interchange paths. They do not imply full application parity or bidirectional synchronization.

| Tool / format | Implemented path | Limits to review |
|---|---|---|
| Word DOCX | Upload; text, headings, lists and tables into editable blocks; export DOCX | Images, page layout, tracked changes and advanced Word fields do not round-trip fully. Original upload is retained. |
| Word DOC | Isolated LibreOffice conversion to DOCX, then the same importer | Requires converter container; conversion may fail on encrypted or damaged files. |
| Excel XLSX | Workbook sheets, values and formula strings; export XLSX | Charts, macros, external links and formatting from the original workbook are not imported. Formula engine coverage differs from Excel. |
| CSV | Import/export spreadsheet data | CSV has no styling, multi-sheet structure or typed schema. Export neutralizes spreadsheet formula injection. |
| ODK / KoboToolbox | XLSForm survey/choices/settings import and export; API project discovery, questionnaire source and response imports | Expressions and custom columns retained; validate in the destination tool. API imports are snapshots, with study-account validation pending. No automatic synchronization or writeback. |
| SurveyCTO | XLSForm-based questionnaire interchange | Vendor-specific fields require review and SurveyCTO validation; no live API connection yet. |
| REDCap | Data dictionary CSV import/export | Source columns retained. Preview does not execute REDCap logic. Cross-conversion of expressions is refused rather than silently translated. No project API synchronization yet. |
| CSPro | Import CSV/XLSX data exported by CSPro | Does not parse CSPro dictionaries, forms, logic or CSDB databases. |
| SurveyMonkey | Import CSV/XLSX response exports | No survey-designer import or SurveyMonkey OAuth/API connector yet. |

The demo accepts uploads up to 10 MB, up to 3,000 spreadsheet rows and 100 columns, and up to 5,000 operations in a batch. Review conversion notes before using an imported instrument for data collection. The preview renders answer controls but does not execute survey expressions.

The spreadsheet supports formulas through HyperFormula, cell formatting, copy/paste, search/filter, values-only sorting and multiple sheets. This is not a complete replacement for Google Sheets or Excel. The document editor provides common text, paragraph, list, link, image-URL and table tools; advanced publishing and review tools still need work.

For API imports, see [ODK/Kobo setup and limits](ODK-KOBO-SETUP.md). Other platform
connections remain file-based. A format round-trip test does not establish that
every vendor-specific expression will execute correctly in its collection tool.

Reference specifications: [XLSForm](https://xlsform.org/en/), [CSPro exports](https://www.csprousers.org/help/CSExport/select_export_method.html), [SurveyMonkey API](https://api.surveymonkey.com/v3/docs), [REDCap dictionary guide](https://ws.engr.illinois.edu/sitemanager/getfile.asp?id=1365).
