export class RevisionStore<T extends {revision:number}> {
  value: T | null = null;
  accept(next:T): 'applied'|'old'|'gap' {
    if(this.value && next.revision<=this.value.revision) return 'old';
    if(this.value && next.revision>this.value.revision+1) return 'gap';
    this.value=next;return 'applied';
  }
  reset(snapshot:T) { if(!this.value || snapshot.revision>=this.value.revision)this.value=snapshot; }
}
export class Operations {
  busy = new Set<string>();
  async run<T>(key:string, work:()=>Promise<T>):Promise<T> {
    if(this.busy.has(key)) throw new Error('Operation already running');
    this.busy.add(key);
    try { return await work(); } finally { this.busy.delete(key); }
  }
}
