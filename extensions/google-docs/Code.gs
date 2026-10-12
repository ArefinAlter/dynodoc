/** Google Docs Editor add-on. Credentials and queues are per user,
 * never DocumentProperties or shared ScriptProperties. No general Docs onEdit
 * trigger exists: the opt-in sidebar polls while open. */
const DYNODOC_ORIGIN = 'https://app.dynodoc.online';
const DYNODOC_HOST = 'google-docs';
const BLOCK_PREFIX = 'dynodoc.block.';
function onOpen() { DocumentApp.getUi().createAddonMenu().addItem('Open Dynodoc', 'showDynodoc').addToUi(); }
function onInstall(e) { onOpen(e); }
function showDynodoc() {
  DocumentApp.getUi().showSidebar(HtmlService.createTemplateFromFile('Sidebar').evaluate().setTitle('Dynodoc'));
}
function include(name) { return HtmlService.createHtmlOutputFromFile(name).getContent(); }
function scope_(expected) {
  const doc = DocumentApp.getActiveDocument(), tab = doc.getActiveTab();
  const key='dynodoc.'+doc.getId()+'.'+tab.getId();
  if(expected && expected!==key)throw new Error('The active tab changed. Return to the connected tab before capturing or applying.');
  return {doc:doc, tab:tab.asDocumentTab(), key:key};
}
function paragraphs_(scope) {
  const body=scope.tab.getBody(), paragraphs=[];
  for(let i=0;i<body.getNumChildren();i++){
    const child=body.getChild(i);
    if(child.getType()!==DocumentApp.ElementType.PARAGRAPH)throw new Error('This first connector supports plain body paragraphs only. Use file push for lists, tables and images.');
    const p=child.asParagraph();
    for(let j=0;j<p.getNumChildren();j++)if(p.getChild(j).getType()!==DocumentApp.ElementType.TEXT)throw new Error('Inline images and other embedded content require file push.');
    paragraphs.push(p);
  }
  if(paragraphs.length>2000)throw new Error('This add-on supports up to 2,000 paragraphs. Use file push for longer documents.');
  return paragraphs;
}
function bindDynodoc(blocks,scopeKey) {
  const s=scope_(scopeKey),paragraphs=paragraphs_(s);
  if(paragraphs.length!==blocks.length||paragraphs.some((p,i)=>p.getText()!==blocks[i].text))throw new Error('This local document differs from the chosen revision. Use file push to choose its starting version.');
  const mapped=anchors_(s);
  // Validate every existing anchor before creating any new anchor.
  paragraphs.forEach((p,i)=>{if(mapped[i]&&mapped[i]!==blocks[i].id)throw new Error('Existing paragraph identities belong to another history.');});
  paragraphs.forEach((p,i)=>{if(!mapped[i])s.tab.addNamedRange(BLOCK_PREFIX+blocks[i].id,s.tab.newRange().addElement(p).build());});
  return readDynodoc(scopeKey);
}
function anchors_(s) {
  const result={},body=s.tab.getBody();
  s.tab.getNamedRanges().filter(r=>r.getName().startsWith(BLOCK_PREFIX)).forEach(r=>{
    const elements=r.getRange().getRangeElements();
    if(elements.length!==1||elements[0].getElement().getType()!==DocumentApp.ElementType.PARAGRAPH)throw new Error('A block anchor changed shape. Use file push for structural changes.');
    const index=body.getChildIndex(elements[0].getElement());
    if(result[index])throw new Error('A paragraph has duplicate identities.');
    result[index]=r.getName().slice(BLOCK_PREFIX.length);
  });return result;
}
function readDynodoc(scopeKey) {
  const s=scope_(scopeKey),paragraphs=paragraphs_(s),mapped=anchors_(s);
  return paragraphs.map((p,i)=>{if(!mapped[i])throw new Error('A new paragraph or missing identity requires file push.');return {id:mapped[i],text:p.getText()};});
}
function applyDynodoc(expected,next,scopeKey) {
  const s=scope_(scopeKey),actual=readDynodoc(scopeKey),paragraphs=paragraphs_(s);
  if(JSON.stringify(actual)!==JSON.stringify(expected)||next.length!==actual.length||next.some((b,i)=>b.id!==actual[i].id))throw new Error('The local file changed after preview. Capture again before applying.');
  paragraphs.forEach((p,i)=>{if(expected[i].text!==next[i].text)p.setText(next[i].text);});
  return readDynodoc(scopeKey);
}
