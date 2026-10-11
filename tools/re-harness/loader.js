// Minimal parcel-module loader that executes the vendor's original modules with stubbed I/O.
const fs = require('fs'), path = require('path');
const RE = process.env.EPO_RE || path.join(__dirname, '..', '..', 'vendor', 're');
const DIR = path.join(RE, 'index');
const deps = JSON.parse(fs.readFileSync(path.join(DIR, '_deps.json')));
const cache = {};
const overrides = {}; // id -> exports
const log = [];
function deepStub(name) {
  const f = function () { return deepStub(name + '()'); };
  return new Proxy(f, {
    get(t, k) {
      if (k === Symbol.toPrimitive) return () => name;
      if (k === 'then') return undefined;
      if (k === '__esModule') return true;
      return deepStub(name + '.' + String(k));
    },
    apply() { return deepStub(name + '()'); },
    construct() { return deepStub('new ' + name); },
  });
}
function load(id) {
  if (overrides[id]) return overrides[id];
  if (cache[id]) return cache[id].exports;
  const file = path.join(DIR, id + '.js');
  const module = { exports: {} };
  cache[id] = module;
  const code = fs.readFileSync(file, 'utf8');
  const fn = new Function('require', 'module', 'exports', code);
  const req = (name) => {
    const target = deps[id] && deps[id][name];
    if (target) {
      try { return load(target); } catch (e) { log.push(`stub ${name} (${target}): ${e.message}`); return deepStub(name); }
    }
    if (name === 'buffer') return require('buffer');
    try { return require(name); } catch (e) { log.push(`stub ext ${name}`); return deepStub(name); }
  };
  try {
    fn.call(module.exports, req, module, module.exports);
  } catch (e) {
    delete cache[id]; // allow a clean retry once the dependency cycle is resolved
    throw e;
  }
  return module.exports;
}
// Module ids change between vendor builds: locate a module by a unique snippet of its source.
function findModule(snippet) {
  const hits = fs.readdirSync(DIR).filter((f) => f.endsWith('.js'))
    .filter((f) => fs.readFileSync(path.join(DIR, f), 'utf8').includes(snippet));
  if (hits.length !== 1) throw new Error(`expected one module containing ${snippet}, found ${hits.length}`);
  return hits[0].replace(/\.js$/, '');
}
module.exports = { load, overrides, deps, log, deepStub, findModule };
