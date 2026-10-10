/** Development Google Docs Editor add-on. Credentials and queues are per user,
 * never DocumentProperties or shared ScriptProperties. No general Docs onEdit
 * trigger exists: the opt-in sidebar polls while open. */
const DYNODOC_ORIGIN = 'https://app.dynodoc.online';
const DYNODOC_HOST = 'google-docs';
const BLOCK_PREFIX = 'dynodoc.block.';
function onOpen() { DocumentApp.getUi().createAddonMenu().addItem('Open provenance', 'showDynodoc').addToUi(); }
function onInstall(e) { onOpen(e); }
function showDynodoc() {
  DocumentApp.getUi().showSidebar(HtmlService.createTemplateFromFile('Sidebar').evaluate().setTitle('Dynodoc provenance'));
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
  if(paragraphs.length>2000)throw new Error('Use up to 2,000 paragraphs in this development connector.');
  return paragraphs;
}
function request_(action,body,scopeKey,throughSeq) {
  const s=scope_(scopeKey), raw=PropertiesService.getUserProperties().getProperty(s.key+'.credential');
  if(!raw)throw new Error('Connect this Google document first.');
  const credential=JSON.parse(raw);
  if(throughSeq!==undefined && (action!=='checkpoint'||!Number.isSafeInteger(throughSeq)||throughSeq<0))throw new Error('Use a valid historical checkpoint revision.');
  const search=throughSeq!==undefined?'?through_seq='+throughSeq:'';
  const response=UrlFetchApp.fetch(DYNODOC_ORIGIN+'/api/connectors/documents/'+credential.documentId+'/'+action+search,{
    method:body?'post':'get',headers:{Authorization:'Bearer '+credential.token},contentType:'application/json',payload:body?JSON.stringify(body):undefined,muteHttpExceptions:true,followRedirects:false
  });
  const value=JSON.parse(response.getContentText());
  if(response.getResponseCode()!==200)throw new Error(value.error||'Dynodoc request failed');
  return value;
}
function connectDynodoc(documentId,token,scopeKey) {
  if(!/^[0-9a-f-]{36}$/i.test(documentId)||! /^[0-9a-f]{64}$/.test(token))throw new Error('Use the file ID and a Google Docs connection key from Projects.');
  const s=scope_(scopeKey),props=PropertiesService.getUserProperties(),key=s.key+'.credential',previous=props.getProperty(key);
  if(previous&&JSON.parse(previous).documentId!==documentId)throw new Error('This Google document is already connected to another file. Export its provenance before changing the binding.');
  props.setProperty(key,JSON.stringify({documentId:documentId,token:token}));
  try{return {checkpoint:request_('checkpoint',undefined,scopeKey),scope:s.key};}catch(e){if(previous)props.setProperty(key,previous);else props.deleteProperty(key);throw e;}
}
function checkpointDynodoc(scopeKey) { return request_('checkpoint',undefined,scopeKey); }
function historicalCheckpointDynodoc(throughSeq,scopeKey) { return request_('checkpoint',undefined,scopeKey,throughSeq); }
function pushDynodoc(body,scopeKey) {
  if(body.bundle.capture.host!=='google-docs')throw new Error('Use a Google Docs change bundle.');
  return request_('bundles',body,scopeKey);
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
function loadLedger(scopeKey) {
  const s=scope_(scopeKey),props=PropertiesService.getUserProperties(),raw=props.getProperty(s.key+'.ledger');
  if(!raw)return {generation:null,payload:null};
  const manifest=JSON.parse(raw);let payload='';
  for(let i=0;i<manifest.count;i++){const part=props.getProperty(s.key+'.queue.'+manifest.generation+'.'+i);if(part===null)throw new Error('Saved provenance is incomplete. Do not overwrite it.');payload+=part;}
  return {generation:manifest.generation,payload:JSON.parse(payload)};
}
function saveLedger(payload,expectedGeneration,scopeKey) {
  const lock=LockService.getUserLock();lock.waitLock(5000);
  const s=scope_(scopeKey),props=PropertiesService.getUserProperties(),key=s.key+'.ledger',raw=props.getProperty(key),previous=raw?JSON.parse(raw):null;
  try{
    if((previous?previous.generation:null)!==expectedGeneration)throw new Error('Another sidebar changed the saved provenance. Reopen this sidebar before continuing.');
    const text=JSON.stringify(payload);
    if(Utilities.newBlob(text).getBytes().length>100000)throw new Error('Export your provenance: this development queue has reached 100 KB. Capture is stopped; your previous saved queue is intact.');
    const generation=Utilities.getUuid(),parts=[];
    for(let start=0;start<text.length;){let end=Math.min(start+2000,text.length);if(end<text.length&&/[\uD800-\uDBFF]/.test(text[end-1]))end--;parts.push(text.slice(start,end));start=end;}
    const created=[];
    try{
      parts.forEach((part,i)=>{const partKey=s.key+'.queue.'+generation+'.'+i;props.setProperty(partKey,part);created.push(partKey);});
      props.setProperty(key,JSON.stringify({generation:generation,count:parts.length}));
    }catch(e){created.forEach(k=>props.deleteProperty(k));throw e;}
    if(previous)for(let i=0;i<previous.count;i++)props.deleteProperty(s.key+'.queue.'+previous.generation+'.'+i);
    return generation;
  }finally{lock.releaseLock();}
}
