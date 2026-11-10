/** In-memory only: a worker restart deliberately loses the input target. */
export class TargetRegistry {
 enroll(profile) { if (!profile) throw Error('profile_required'); this.profile=profile; }
 issue(profile,nonce,origin,expires) {
  if (profile!==this.profile || !nonce) throw Error('not_enrolled');
  this.pending={profile,nonce,origin,expires};
 }
 launch(profile,nonce,now) {
  const p=this.pending;
  if (!p || p.profile!==profile || p.nonce!==nonce || now>=p.expires) throw Error('stale_launch');
  this.pending=null; this.origin=p.origin; return p;
 }
 attach(tabId,session,generation) {
  if (!this.origin) throw Error('not_enrolled');
  this.target={tabId,session,generation,origin:this.origin}; return this.target;
 }
 validate(session,generation,tabId) {
  const t=this.target;
  if (!t || t.session!==session || t.generation!==generation || t.tabId!==tabId) throw Error('stale_target');
  return t;
 }
 invalidate() { this.target=null; this.origin=null; this.pending=null; }
}
