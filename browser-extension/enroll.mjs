const status=document.querySelector('#status');
async function send(type) {
 const result=await chrome.runtime.sendMessage({type});
 status.textContent=JSON.stringify(result); return result;
}
document.querySelector('#enroll').onclick=()=>send('enroll');
document.querySelector('#probe').onclick=()=>send('probe');
// Harness calls the same explicit enrollment actions in its disposable profile.
window.probeEnrollment=()=>send('enroll');
window.probeRun=()=>send('probe');
