/** Shared per-user credentials and compare-and-swap queue storage. Generated copies are checked by build-connectors. */
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
  if(!/^[0-9a-f-]{36}$/i.test(documentId)||! /^[0-9a-f]{64}$/.test(token))throw new Error('Use the file ID and a editor connection key from Projects.');
  const s=scope_(scopeKey),props=PropertiesService.getUserProperties(),key=s.key+'.credential',previous=props.getProperty(key);
  if(previous&&JSON.parse(previous).documentId!==documentId)throw new Error('This Google document is already connected to another file. Export its provenance before changing the binding.');
  props.setProperty(key,JSON.stringify({documentId:documentId,token:token}));
  try{return {checkpoint:request_('checkpoint',undefined,scopeKey),scope:s.key};}catch(e){if(previous)props.setProperty(key,previous);else props.deleteProperty(key);throw e;}
}
function checkpointDynodoc(scopeKey) { return request_('checkpoint',undefined,scopeKey); }
function historicalCheckpointDynodoc(throughSeq,scopeKey) { return request_('checkpoint',undefined,scopeKey,throughSeq); }
function pushDynodoc(body,scopeKey) {
  if(body.bundle.capture.host!==DYNODOC_HOST)throw new Error('Use a editor change bundle.');
  return request_('bundles',body,scopeKey);
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

function sameBlocks_(a,b){return a.length===b.length&&a.every((x,i)=>x.id===b[i].id&&(x.field||"content")===(b[i].field||"content")&&x.text===b[i].text);}
