// Extracts device facts (ids, USB ids, key matrix, lighting capabilities, physical key layout)
// for a family of models from the vendor app into this project's own JSON schema.
//
// Usage: NODE_PATH=vendor/app/resources/app/node_modules node tools/re-harness/dump_devices.js <name-regex> out.json
const fs = require('fs');
// The vendor app hides the host-streamed effects (music follow, screen color) unless it runs
// on Windows; pretend to be Windows so the device data lists every effect the firmware has.
Object.defineProperty(process, 'platform', { value: 'win32' });
const path = require('path');
require('./fake');
const L = require('./loader');

const RE = process.env.EPO_RE || path.join(__dirname, '..', '..', 'vendor', 're');
const INDEX = path.join(RE, 'index');
const files = fs.readdirSync(INDEX).filter((f) => f.endsWith('.js'));
const src = {};
const read = (f) => (src[f] = src[f] || fs.readFileSync(path.join(INDEX, f), 'utf8'));
function findModule(snippet) {
  const hits = files.filter((f) => read(f).includes(snippet));
  if (hits.length !== 1) throw new Error(`module for ${snippet}: ${hits.length}`);
  return hits[0].replace(/\.js$/, '');
}

const filter = new RegExp(process.argv[2] || '^yc500_th80');
const support = L.load(findModule('exports.supportYC500Dev=void 0')).supportYC500Dev;
const layouts = L.load(findModule('yc500_th80:{layout:'));
const registry = Object.values(layouts).find((v) => v && typeof v === 'object' && v.yc500_th80);
const factorySrc = read(findModule('case"yc500_th80":return new') + ".js");

const devices = [];
const models = {};
for (const d of support.filter((x) => filter.test(x.name))) {
  devices.push({
    id: d.id,
    name: d.name,
    display_name: d.displayName,
    company: d.company,
    vid: d.vid,
    pid: d.pid,
    usage_page: d.usagePage,
    usage: d.usage,
    layers: d.layer,
    fn_layers: d.fnLayer,
    dongle: d.id < 0,
  });
  if (models[d.name]) continue;
  // class used by the vendor factory for this model name
  const m = factorySrc.match(new RegExp(`case"${d.name}":return new \\w+\\.(\\w+)\\(`));
  const mod = L.load(findModule(`exports.${m[1]}=void 0`));
  const cls = mod[m[1]];
  if (typeof cls !== "function") { console.error("cannot load class", m[1], Object.keys(mod), L.log.slice(-5)); continue; }
  const inst = new cls(Object.assign({}, d, { devAddr: 'x' }));
  const vis = registry[d.name];
  models[d.name] = {
    matrix: Array.from(inst.defaultMatrix),
    macro_max: inst.MACROMAX,
    light_modes: d.layout.light.types,
    report_rates: d.layout.reportRate,
    other: d.otherSetting,
    layout: vis && {
      offset: vis.delt,
      keys: vis.layout.map((k) => ({ label: k.keyName, code: k.value, x: k.x, y: k.y, w: k.w, h: k.h })),
    },
  };
}

const mapping = L.load(findModule('exports.specialFunTablectionMap='));
fs.writeFileSync(process.argv[3] || 'devices.json', JSON.stringify({
  source: 'EPOMAKER Driver 1.2.66 (WIN20230421)',
  devices,
  models,
  special_functions: mapping.specialFunTablectionMap,
}, null, 1));
console.error(`devices: ${devices.length}, models: ${Object.keys(models).join(', ')}`);
