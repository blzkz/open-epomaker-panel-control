// Build id->names map from parcel dep maps (index tsv col 3 has deps JSON, maybe truncated -> re-read from bundle)
const fs=require('fs');
const [bundle,out]=process.argv.slice(2);
const txt=fs.readFileSync(bundle,'utf8');
const re=/\n\},(\{[^\n]*?\})\],"([A-Za-z0-9_$]+)":\[function/g;
// deps for module X appear right before next module's id; need previous id ordering
const idRe=/"([A-Za-z0-9_$]+)":\[function\(require,module,exports\) \{\n/g;
let ids=[],m;while((m=idRe.exec(txt)))ids.push({id:m[1],pos:m.index});
const names={};
for(let i=0;i<ids.length;i++){
  const end=i+1<ids.length?ids[i+1].pos:txt.length;
  const seg=txt.slice(ids[i].pos,end);
  const k=seg.lastIndexOf('\n},{');
  if(k<0)continue;
  let d=seg.slice(k+3); d=d.slice(0,d.lastIndexOf('}')+1);
  try{const o=JSON.parse(d);for(const [n,t] of Object.entries(o)){(names[t]=names[t]||new Set()).add(n)}}catch(e){}
}
const lines=Object.entries(names).map(([id,s])=>id+'\t'+[...s].join(' '));
fs.writeFileSync(out,lines.join('\n'));
console.log(lines.length);
