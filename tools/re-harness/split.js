// Split a parcel v1 bundle into modules: lines like  "id":[function(require,module,exports) {\n ... \n},{deps}],
const fs=require('fs'),path=require('path');
const [src,out]=process.argv.slice(2);
fs.mkdirSync(out,{recursive:true});
const txt=fs.readFileSync(src,'utf8');
const re=/(?:^|\n)(?:\}\,\{[^\n]*?\}\],)?"([A-Za-z0-9_$]+)":\[function\(require,module,exports\) \{\n/g;
let m,starts=[];
while((m=re.exec(txt))){starts.push({id:m[1],body:m.index+m[0].length});}
const index=[];
for(let i=0;i<starts.length;i++){
  const end=i+1<starts.length?txt.lastIndexOf('\n},{',starts[i+1].body):txt.length;
  const body=txt.slice(starts[i].body,end);
  const depEnd=txt.indexOf('\n',end+1);
  const deps=txt.slice(end+3,depEnd);
  fs.writeFileSync(path.join(out,starts[i].id+'.js'),body);
  index.push(starts[i].id+'\t'+body.length+'\t'+deps.slice(0,300));
}
fs.writeFileSync(path.join(out,'_index.tsv'),index.join('\n'));
console.log(starts.length,'modules');
