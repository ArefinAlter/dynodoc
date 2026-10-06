/** Development Sheets adapter. Named rows retain the engine's stable IDs.
 * Dates, merges, structural and format edits require file push. */
const DYNODOC_ORIGIN='https://app.dynodoc.online',DYNODOC_HOST='google-sheets',BLOCK_PREFIX='dynodoc_';
function onOpen(){SpreadsheetApp.getUi().createAddonMenu().addItem('Open provenance','showDynodoc').addToUi();}
function onInstall(e){onOpen(e);}
function showDynodoc(){SpreadsheetApp.getUi().showSidebar(HtmlService.createTemplateFromFile('Sidebar').evaluate().setTitle('Dynodoc provenance'));}
function include(name){return HtmlService.createHtmlOutputFromFile(name).getContent();}
function scope_(expected){const doc=SpreadsheetApp.getActiveSpreadsheet(),key='dynodoc.'+doc.getId();if(expected&&expected!==key)throw new Error('Return to the connected spreadsheet.');return {doc:doc,key:key};}
function rows_(config,scopeKey,binding){
  const targets=config.targets;
  const s=scope_(scopeKey),groups=config.groups.map(g=>g.name),sheets=s.doc.getSheets(),known=new Set(targets.map(t=>BLOCK_PREFIX+t.id));
  if(sheets.length!==groups.length||groups.some(g=>!s.doc.getSheetByName(g)))throw new Error('Sheet names or structure changed. Use file push.');
  if(s.doc.getNamedRanges().some(n=>n.getName().startsWith(BLOCK_PREFIX)&&!known.has(n.getName())))throw new Error('This workbook has identities from another history.');
  sheets.forEach(sheet=>{const ts=targets.filter(t=>t.group===sheet.getName());if(sheet.getLastRow()>Math.max(...ts.map(t=>t.index))+1||sheet.getLastColumn()>Math.max(...ts.map(t=>t.column))+1)throw new Error('New rows or columns require file push.');});
  return [...new Map(targets.map(t=>[t.id,t])).values()].map(t=>{
    const sheet=s.doc.getSheetByName(t.group),columns=targets.filter(x=>x.id===t.id).length,range=sheet.getRange(t.index+1,1,1,columns),name=BLOCK_PREFIX+t.id,anchor=s.doc.getRangeByName(name);
    if(range.getMergedRanges().length)throw new Error('Merged cells require file push.');
    if(anchor&&(anchor.getSheet().getSheetId()!==sheet.getSheetId()||anchor.getRow()!==t.index+1||anchor.getColumn()!==1||anchor.getNumRows()!==1||anchor.getNumColumns()!==columns))throw new Error('A row identity moved or changed. Use file push.');
    if(!binding&&!anchor)throw new Error('A row identity is missing. Use file push.');
    return {t:t,range:range,name:name,anchor:anchor,values:range.getValues()[0],formulas:range.getFormulas()[0]};
  });
}
function cellText_(value,formula,type){if(formula){if(type==='text')throw new Error('A text cell became a formula. Use file push.');return formula;}if(value instanceof Date)throw new Error('Date cells require file push.');if(!['string','number','boolean'].includes(typeof value)||typeof value==='string'&&value.startsWith('='))throw new Error('Unsupported cell value. Use file push.');if(value!==''&&((type==='text'&&typeof value!=='string')||(type==='number'&&typeof value!=='number')||(type==='boolean'&&typeof value!=='boolean')))throw new Error('A cell type changed. Use file push.');return String(value);}
function blocks_(rows,targets){return targets.map(t=>{const r=rows.find(r=>r.t.id===t.id);return {id:t.id,field:t.field,text:cellText_(r.values[t.column],r.formulas[t.column],t.cellType)};});}
function bindDynodoc(blocks,config,scopeKey){const targets=config.targets;const rows=rows_(config,scopeKey,true),actual=blocks_(rows,targets);if(!sameBlocks_(actual,blocks))throw new Error('This spreadsheet differs from the chosen revision. Use file push.');const s=scope_(scopeKey);rows.forEach(r=>{if(!r.anchor)s.doc.setNamedRange(r.name,r.range);});return readDynodoc(config,scopeKey);}
function readDynodoc(config,scopeKey){const targets=config.targets;return blocks_(rows_(config,scopeKey,false),targets);}
function cellValue_(text,type){if(text===''||text.startsWith('='))return text;if(type==='number'){const n=Number(text);if(!Number.isFinite(n))throw new Error('This update changes a numeric cell type. Use file push.');return n;}if(type==='boolean'){if(!['true','false'].includes(text))throw new Error('This update changes a boolean cell type. Use file push.');return text==='true';}if(!type&&text.trim()&&Number.isFinite(Number(text)))return Number(text);if(!type&&['true','false'].includes(text))return text==='true';return text;}
function applyDynodoc(expected,next,config,scopeKey){const targets=config.targets;const rows=rows_(config,scopeKey,false),actual=blocks_(rows,targets);if(!sameBlocks_(actual,expected)||next.length!==actual.length||next.some((b,i)=>b.id!==actual[i].id||b.field!==actual[i].field))throw new Error('The local file changed after preview. Capture again.');const values=next.map((b,i)=>cellValue_(b.text,targets[i].cellType));scope_(scopeKey);next.forEach((b,i)=>{if(b.text!==expected[i].text){const t=targets[i],cell=rows.find(r=>r.t.id===t.id).range.getCell(1,t.column+1);if(b.text.startsWith('='))cell.setFormula(b.text);else cell.setValue(values[i]);}});SpreadsheetApp.flush();return readDynodoc(config,scopeKey);}
