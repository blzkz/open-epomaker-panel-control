const fs=require('fs');const [bundle,out]=process.argv.slice(2);
const txt=fs.readFileSync(bundle,'utf8');
const idRe=/"([A-Za-z0-9_$]+)":\[function\(require,module,exports\) \{\n/g;
let ids=[],m;while((m=idRe.exec(txt)))ids.push({id:m[1],pos:m.index});
const deps={};
for(let i=0;i<ids.length;i++){const end=i+1<ids.length?ids[i+1].pos:txt.length;const seg=txt.slice(ids[i].pos,end);const k=seg.lastIndexOf('\n},{');let d=seg.slice(k+3);d=d.slice(0,d.lastIndexOf('}')+1);try{deps[ids[i].id]=JSON.parse(d)}catch(e){deps[ids[i].id]={}}}
fs.writeFileSync(out,JSON.stringify(deps));
