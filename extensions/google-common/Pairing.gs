/** Browser approval uses a hash; this secret travels only in HTTPS request headers. */
function pairingConfigDynodoc(scopeKey) {
  return {origin:DYNODOC_ORIGIN,host:DYNODOC_HOST,scope:scope_(scopeKey).key};
}
function connectApprovedDynodoc(token,scopeKey) {
  scope_(scopeKey);
  if(!/^[0-9a-f]{64}$/.test(token))throw new Error('Start sign-in again.');
  const response=UrlFetchApp.fetch(DYNODOC_ORIGIN+'/api/connectors/connection',{
    headers:{Authorization:'Bearer '+token},muteHttpExceptions:true,followRedirects:false
  });
  if(response.getResponseCode()===401)throw new Error('Approve this connection in your browser, then try again.');
  const value=JSON.parse(response.getContentText());
  if(response.getResponseCode()!==200)throw new Error(value.error||'Could not connect.');
  if(value.host!==DYNODOC_HOST)throw new Error('This connection belongs to a different editor.');
  return connectDynodoc(value.document_id,token,scopeKey);
}
