// Runs the vendor's ORIGINAL device classes against a fake transport and records the exact
// HID feature reports they produce (and how they decode canned responses). The output is a
// plain-data JSON file of byte vectors used as conformance tests for the Rust implementation.
//
// Usage: NODE_PATH=vendor/app/resources/app/node_modules node tools/re-harness/gen_golden.js out.json [device-id] [Class]
//
// device-id is the model id from `supportYC500Dev` (default 1118, wired TH80); the class defaults
// to the one the vendor factory instantiates for that model. The keys used by the key-mapping and
// per-key color cases are picked from the model's own matrix and listed in `meta.keys`.
const fs = require('fs');
const path = require('path');
const F = require('./fake');
const L = require('./loader');

const RE = process.env.EPO_RE || path.join(__dirname, '..', '..', 'vendor', 're');
const INDEX = path.join(RE, 'index');

// Locate a module by a unique snippet of its source (ids are build-specific).
function findModule(snippet) {
  const hits = fs.readdirSync(INDEX).filter((f) => f.endsWith('.js'))
    .filter((f) => fs.readFileSync(path.join(INDEX, f), 'utf8').includes(snippet));
  if (hits.length !== 1) throw new Error(`module for ${snippet}: ${hits}`);
  return hits[0].replace(/\.js$/, '');
}

const support = L.load(findModule('exports.supportYC500Dev=void 0')).supportYC500Dev;
const devId = Number(process.argv[3] || 1118);
const devEntry = support.find((d) => d.id === devId);
if (!devEntry) throw new Error(`unknown device id ${devId}`);
const devType = Object.assign({}, devEntry, { devAddr: 'fake' });
const factorySrc = fs.readFileSync(path.join(INDEX, `${findModule('case"yc500_th80":return new')}.js`), 'utf8');
const className = process.argv[4]
  || (factorySrc.match(new RegExp(`case"${devType.name}":return new \\w+\\.(\\w+)\\(`)) || [])[1];
if (!className) throw new Error(`no class for ${devType.name}`);
const Model = L.load(findModule(`exports.${className}=void 0`))[className];

const hex = (a) => Buffer.from(a).toString('hex');
const cases = [];

function newDev() {
  const d = new Model(devType);
  d.currentProfile = 0;
  d.allLayerConfigs = [];
  d.reComData = { profile: 0, matrix: Buffer.from(d.defaultMatrix), marcroBuf: [] };
  d.reFnData = { profile: 0, matrix: undefined, marcroBuf: [] };
  return d;
}

// Keys used by the cases below, by role. The TH80 choices come first; a model that lacks one gets
// the next unused plain key of its own matrix instead (e.g. a numpad has no 'A' or F-keys).
const factory = [...newDev().defaultMatrix];
const entries = [];
for (let i = 0; i < factory.length; i += 4) entries.push(factory.slice(i, i + 4));
const plain = entries.filter((e) => e[0] === 0 && e[1] === 0 && e[2] !== 0 && e[3] === 0).map((e) => e[2]);
const picked = [];
const pick = (want) => {
  const k = plain.includes(want) && !picked.includes(want) ? want : plain.find((h) => !picked.includes(h));
  if (k === undefined) throw new Error(`${devType.name}: not enough keys`);
  picked.push(k);
  return k;
};
const KEYS = {
  a: pick(4), esc: pick(41), f: [57, 58, 59, 60, 61, 62].map(pick), mouse: pick(63),
  macros: [30, 31, 32, 33].map(pick), b: pick(5), shift: pick(225),
};
// a key whose matrix entry is not a plain HID usage (addressed by its 32-bit value)
const special = entries.find((e) => e[0] === 3 && e[1] === 0 && e[2] === 226 && e[3] === 0)
  || entries.find((e) => e[0] !== 0);
KEYS.special = special ? ((special[0] << 24) | (special[1] << 16) | (special[2] << 8) | special[3]) >>> 0 : null;

// responses: array of 64-byte arrays returned in order on each read.
async function record(name, fn, responses = []) {
  F.packets.length = 0;
  const queue = responses.map((r) => Buffer.from(r.concat(new Array(64 - r.length).fill(0))));
  F.setResponder(() => (queue.length ? queue.shift() : Buffer.alloc(64)));
  const dev = newDev();
  let result;
  try {
    result = await fn(dev);
  } catch (e) {
    result = { error: String(e) };
  }
  cases.push({
    name,
    writes: F.packets.filter((p) => p.op === 'set' || p.op === 'raw').map((p) => ({ ck: p.ck, data: hex(p.data) })),
    reads: F.packets.filter((p) => p.op === 'get').length,
    toasts: F.packets.filter((p) => p.op === 'toastErr').map((p) => p.msg),
    result: result === undefined ? null : JSON.parse(JSON.stringify(result, (k, v) => (v && v.type === 'Buffer' ? hex(v.data) : v))),
  });
}

(async () => {
  const layout = devType.layout.light.types;

  // ---- lighting (SET_LEDPARAM) ----
  const colors = [0xff0000, 0x00ff00, 0x123456, 0xffffff];
  for (const t of layout) {
    const opts = t.options || [undefined];
    for (const option of opts) {
      for (const dazzle of t.dazzle ? [false, true] : [false]) {
        for (const [speed, value] of [[0, 0], [2, 3], [4, 4]]) {
          const rgb = t.rgb ? colors[(speed + value) % colors.length] : undefined;
          const cfg = { type: t.type, speed, value, dazzle, option, rgb };
          await record(`led_set ${JSON.stringify(cfg)}`, (d) => d.setLightSetting(cfg));
        }
      }
    }
  }
  await record('led_set {"type":"LightOff"}', (d) => d.setLightSetting({ type: 'LightOff' }));

  // ---- lighting decode (GET_LEDPARAM) ----
  for (let mode = 0; mode <= 24; mode++) {
    for (const optByte of [0x07, 0x08, 0x17, 0x28, 0x31, 0x00, 0x14]) {
      const resp = [0x87, mode, 1, 3, optByte, 0x12, 0x34, 0x56];
      await record(`led_get ${hex(resp)}`, (d) => d.getLightSetting(), [resp]);
    }
  }
  await record('led_get 87420000', (d) => d.getLightSetting(), [[0x87, 66, 0, 0, 0]]);

  // ---- report rate ----
  for (const r of [125, 250, 500, 1000]) await record(`rate_set ${r}`, (d) => d.setReportRate(r));
  for (const v of [1, 2, 4, 8, 3]) await record(`rate_get ${v}`, (d) => d.getReportRate(), [[0x84, 0, v]]);

  // ---- profile ----
  for (const p of [0, 1, 2]) await record(`profile_set ${p}`, (d) => d.setCurrentProfile(p));
  for (const p of [0, 1, 2]) await record(`profile_get ${p}`, (d) => d.getCurrentProfile(), [[0x85, p]]);
  await record('rate_set_profile2 1000', (d) => { d.currentProfile = 2; return d.setReportRate(1000); });

  // ---- keyboard options ----
  const optCases = [
    { winKeyLockControl: true },
    { system: true },
    { wasdKeyAndArrowKeyExchange: true },
    { ledOff: true },
    { sLedOff: true },
    { keyboardMode: true },
    { keyboardLockControl: true },
    { keyboardFnKeyMatrix: true },
    { powerSaveMode: true },
    { winKeyLockControl: true, system: true, wasdKeyAndArrowKeyExchange: true, ledOff: true, sLedOff: true, keyboardMode: true, keyboardLockControl: true, keyboardFnKeyMatrix: true, powerSaveMode: true },
  ];
  const fullOpts = (o) => Object.assign({ winKeyLockControl: false, system: false, wasdKeyAndArrowKeyExchange: false, ledOff: false, sLedOff: false, keyboardMode: false, keyboardLockControl: false, keyboardFnKeyMatrix: false, powerSaveMode: false }, o);
  for (const o of optCases) await record(`opts_set ${JSON.stringify(o)}`, (d) => d.setKeyboardOption(fullOpts(o)));
  for (const b of [[0, 0, 0], [0xff, 1, 1], [0x05, 0, 0], [0x4a, 1, 0]]) await record(`opts_get ${hex(b)}`, (d) => d.getKeyboardOption(), [[0x86, 0, ...b]]);

  // ---- debounce / sleep ----
  for (const v of [1, 5, 10, 30]) await record(`debounce_set ${v}`, (d) => d.setDeBounce(v));
  await record('debounce_get 7', (d) => d.getDeBounce(), [[0x91, 0, 7]]);
  const sleepCfg = { time_bt: 600, time_24: 300, deepTime_bt: 1800, deepTime_24: 3600 };
  await record(`sleep_set ${JSON.stringify(sleepCfg)}`, (d) => d.setSleepKeyTime(sleepCfg));
  await record('sleep_get', (d) => d.getSleepKeyTime(), [[0x92, 0x58, 0x02, 0x2c, 0x01, 0x08, 0x07, 0x10, 0x0e]]);
  await record('sleep_get_clamp', (d) => d.getSleepKeyTime(), [[0x92, 10, 0, 0, 0, 1, 0, 0xff, 0xff]]);

  // ---- misc ----
  await record('battery_get', (d) => d.getBattery(), [[0x83, 77, 1]]);
  await record('battery_get_full', (d) => d.getBattery(), [[0x83, 100, 2]]);
  await record('fw_get', (d) => d.getFirmwareVersion(), [[0x80, 0x34, 0x12]]);
  await record('reset', (d) => d.setReSet());
  await record('autoosen_set 1', (d) => d.setAutoOsen(true));
  await record('autoosen_get', (d) => d.getAutoOsen(), [[0x97, 1]]);

  // ---- key remap: simple writes ----
  const keyCfgs = [
    { type: 'combo', original: KEYS.a, skey: 'none', key: 5, key2: 0 }, // A -> B
    { type: 'combo', original: KEYS.a, skey: 'ctrl', key: 6, key2: 0 }, // A -> Ctrl+C
    { type: 'combo', original: KEYS.esc, skey: 'shift', key: 30, key2: 31 },
    { type: 'forbidden', original: KEYS.f[0] }, // disable caps
    { type: 'ConfigChangeToMouseBtn', original: KEYS.f[1], key: -1000 },
    { type: 'ConfigFunction', original: KEYS.f[2], key: '静音' },
    { type: 'ConfigFunction', original: KEYS.f[3], key: '音量大' },
    { type: 'ConfigFunction', original: KEYS.f[4], key: '计算器' },
    { type: 'ConfigFunction', original: KEYS.f[5], key: 'fn' },
  ];
  if (KEYS.special !== null) keyCfgs.push({ type: 'combo', original: KEYS.special, skey: 'none', key: 30, key2: 0 });
  keyCfgs.push({ type: 'combo', original: KEYS.shift, skey: 'none', key: 229, key2: 0, index: 0 });
  const ifc = L.load(findModule('exports.MouseKey=void 0'));
  for (const k of Object.keys(ifc.MouseKey).filter((x) => isNaN(Number(x)))) {
    keyCfgs.push({ type: 'ConfigChangeToMouseBtn', original: KEYS.mouse, key: ifc.MouseKey[k] });
  }
  for (const c of keyCfgs) {
    await record(`key_simple ${JSON.stringify(c)}`, (d) => d.setKeyConfigSimple(Object.assign({}, c)));
    await record(`key_simple_p1 ${JSON.stringify(c)}`, (d) => { d.currentProfile = 1; return d.setKeyConfigSimple(Object.assign({}, c)); });
    await record(`fn_simple ${JSON.stringify(c)}`, (d) => d.setFnKeyConfigSimple(Object.assign({}, c), 0));
  }

  // ---- macros ----
  const macro1 = {
    type: 'ConfigMacro', original: KEYS.macros[0], macroType: 'repeat_times', repeatCount: 1,
    macro: [
      { type: 'keyboard', action: 'down', value: 4 }, { type: 'delay', value: 50 },
      { type: 'keyboard', action: 'up', value: 4 }, { type: 'delay', value: 200 },
      { type: 'keyboard', action: 'down', value: 5 }, { type: 'keyboard', action: 'up', value: 5 },
    ],
  };
  const macro2 = {
    type: 'ConfigMacro', original: KEYS.macros[1], macroType: 'on_off', repeatCount: 3,
    macro: [
      { type: 'mouse_button', action: 'down', value: -1000 }, { type: 'delay', value: 1000 },
      { type: 'mouse_button', action: 'up', value: -1000 },
      { type: 'keyboard', action: 'down', value: 225 }, { type: 'delay', value: 127 },
      { type: 'keyboard', action: 'down', value: 30 }, { type: 'delay', value: 128 },
      { type: 'keyboard', action: 'up', value: 30 }, { type: 'keyboard', action: 'up', value: 225 },
    ],
  };
  const macro3 = Object.assign({}, macro1, { original: KEYS.macros[2], macroType: 'touch_repeat', repeatCount: 65535 });
  for (const m of [macro1, macro2, macro3]) {
    await record(`macro_key ${JSON.stringify(m)}`, (d) => d.setKeyConfigSimple(JSON.parse(JSON.stringify(m))));
  }
  // long macro spanning several chunks
  const longMacro = { type: 'ConfigMacro', original: KEYS.macros[3], macroType: 'repeat_times', repeatCount: 2, macro: [] };
  for (let i = 0; i < 40; i++) longMacro.macro.push({ type: 'keyboard', action: i % 2 ? 'up' : 'down', value: 4 + (i >> 1) }, { type: 'delay', value: 10 + i });
  await record('macro_key long', (d) => d.setKeyConfigSimple(JSON.parse(JSON.stringify(longMacro))));
  // macro byte buffer encoding only
  await record('macro_encode macro1', async (d) => { await d.setMacro(JSON.parse(JSON.stringify(macro1)), 3); return d.reComData.marcroBuf[3]; });
  await record('macro_encode macro2', async (d) => { await d.setMacro(JSON.parse(JSON.stringify(macro2)), 7); return d.reComData.marcroBuf[7]; });

  // ---- full matrix write (used e.g. when restoring a whole layer) ----
  const m = [...newDev().defaultMatrix];
  m[4 * 9] = 0; m[4 * 9 + 2] = 5; // swap A to B at index of 'A'
  await record('matrix_write_raw', (d) => d._setKeyConfig(m));
  await record('fn_matrix_write_raw', (d) => d._setFnKeyConfig(m, 0));

  // ---- full matrix read + decode ----
  const mm = [...newDev().defaultMatrix];
  const idx = (hid) => { for (let i = 0; i < mm.length; i += 4) if (mm[i] === 0 && mm[i + 1] === 0 && mm[i + 2] === hid && mm[i + 3] === 0) return i; };
  let i4 = idx(KEYS.a); mm[i4] = 0; mm[i4 + 1] = 224; mm[i4 + 2] = 6; mm[i4 + 3] = 0; // A -> ctrl+c
  i4 = idx(KEYS.f[0]); mm.fill(0, i4, i4 + 4); // caps disabled
  i4 = idx(KEYS.f[1]); mm.splice(i4, 4, 1, 0, 240, 0); // F1 -> left click
  i4 = idx(KEYS.f[2]); mm.splice(i4, 4, 3, 0, 205, 0); // F2 -> play/pause
  i4 = idx(KEYS.f[3]); mm.splice(i4, 4, 0, 0, 1, 0); // F3 -> unknown 0.0.1.0
  i4 = idx(KEYS.f[4]); mm.splice(i4, 4, 3, 0, 0x99, 0x09); // F4 -> unknown consumer
  const chunks = []; for (let c = 0; c < 8; c++) chunks.push(mm.slice(64 * c, 64 * c + 64));
  await record('matrix_read', (d) => d.getKeyConfig(0, true), chunks);
  await record('fn_matrix_read', (d) => d.getFnKeyConfig(0, true), chunks);

  // macro read/decode
  const mbuf = [...newDev().defaultMatrix].fill(0).slice(0, 256);
  const macroBytes = Buffer.alloc(256);
  // repeat=2, A down 30ms, A up 0 / 300ms, mouse move
  macroBytes.writeUInt16LE(2, 0);
  Buffer.from([4, 0x80 | 30, 4, 0x00, 0x2c, 0x01, 0xf0, 0x80 | 5, 0xf0, 0x05]).copy(macroBytes, 2);
  const mchunks = []; for (let c = 0; c < 4; c++) mchunks.push([...macroBytes.slice(64 * c, 64 * c + 64)]);
  await record('macro_read 5', (d) => d.getMacro(5), mchunks);
  void mbuf;

  // ---- per-key RGB (user picture) ----
  await record('userpic_simple', (d) => d.setLightPicSimple({ hid: KEYS.a, rgb: 0x112233 }, 1));
  await record('userpic_simple_esc', (d) => d.setLightPicSimple({ hid: KEYS.esc, rgb: 0xff8000 }, 0));
  const pic = [{ hid: KEYS.esc, rgb: 0xff0000 }, { hid: KEYS.a, rgb: 0x00ff00 }, { hid: KEYS.b, rgb: 0x0000ff }];
  await record('userpic_full', (d) => d.setLightPic(pic));
  const pics = newDev().picsCount;
  const picBuf = Buffer.alloc(64 * pics);
  picBuf[0] = 0xff; picBuf[1] = 0; picBuf[2] = 0; // index 0 (Esc) red
  picBuf[3 * 9] = 0; picBuf[3 * 9 + 1] = 0xff; picBuf[3 * 9 + 2] = 0; // index 9 ('A') green
  const pchunks = []; for (let c = 0; c < pics; c++) pchunks.push([...picBuf.slice(64 * c, 64 * c + 64)]);
  await record('userpic_read', (d) => d.getLightPic(), pchunks);

  const meta = {
    generator: 'tools/re-harness/gen_golden.js',
    source: `EPOMAKER Driver 1.2.66 (WIN20230421), class ${className}`,
    device: { id: devType.id, name: devType.name, vid: devType.vid, pid: devType.pid },
    picsCount: newDev().picsCount,
    macroMax: newDev().MACROMAX,
    keys: KEYS,
  };
  fs.writeFileSync(process.argv[2] || 'golden.json', JSON.stringify({ meta, cases }, null, 1));
  console.error(`wrote ${cases.length} cases`);
})();
