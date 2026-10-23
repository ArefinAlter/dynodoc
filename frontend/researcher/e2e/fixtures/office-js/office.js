// A stand-in for Office.js, for the end-to-end tests only. The real library is not in this
// repository: an operator supplies it (docs/SELF-HOSTED-DEPLOYMENT-GUIDE.md, "Office.js").
// It reports a host that is not Word, so the Word pane shows its plain "open this in Word"
// message, which proves the pane loaded and ran this file instead of Microsoft's.
window.__dynodocOfficeJsStandIn = true;
window.Office = {
  onReady(callback) {
    callback({ host: 'Outlook' });
  },
  context: { requirements: { isSetSupported: () => false }, document: {} }
};
