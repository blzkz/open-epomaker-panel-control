// Records the host-streamed frames of the "music follow" and "screen color" effects,
// produced by the vendor's own YC500 class, plus the level computation the vendor app
// applies to WebAudio spectrum data (MusicFollowStore.music3Delt).
//
// Usage: NODE_PATH=vendor/app/resources/app/node_modules node tools/re-harness/gen_golden_live.js out.json
const fs = require('fs');
const F = require('./fake');
const L = require('./loader');

const TH80 = L.load(L.findModule('exports.YC500_TH80=void 0')).YC500_TH80;
const support = L.load(L.findModule('exports.supportYC500Dev=void 0')).supportYC500Dev;
const devType = Object.assign({}, support.find((d) => d.id === 1118), { devAddr: 'fake' });
const hex = (a) => Buffer.from(a).toString('hex');

// Deterministic pseudo-random numbers.
let seed = 12345;
const rnd = (n) => {
  seed = (seed * 1103515245 + 12345) % 2147483648;
  return seed % n;
};

// Transcription of the closure that fills `music3Delt` in the vendor's MusicFollowStore
// (judge = Uint8 spectrum from AnalyserNode.getByteFrequencyData, mCount = 6 for keyboards).
function music3(judge, mCount) {
  let t = new Array(100);
  const e = judge.slice(0, 100);
  const r = Math.min(...e.slice(20, 40));
  for (let o = 0; o < 100; o++) t[o] = e[o] - r;
  const i = Math.max(...t);
  t = t.map((v) => {
    const s = i / (0 + mCount);
    const q = Math.round(v / s - 0);
    return q < 0 ? 0 : q > mCount ? mCount : q;
  });
  return t.slice(27, 59);
}

(async () => {
  const cases = [];
  async function record(name, fn) {
    F.packets.length = 0;
    const dev = new TH80(devType);
    await fn(dev);
    cases.push({
      name,
      writes: F.packets.filter((p) => p.op === 'set' || p.op === 'raw').map((p) => ({ op: p.op, ck: p.ck, data: hex(p.data) })),
    });
  }

  const levelSets = [new Array(32).fill(0), new Array(32).fill(6), Array.from({ length: 32 }, (_, i) => i % 7)];
  for (let k = 0; k < 5; k++) levelSets.push(Array.from({ length: 32 }, () => rnd(7)));
  for (const lv of levelSets) await record(`music ${JSON.stringify(lv)}`, (d) => d.setMusicFollow(lv, 0, false));

  for (const c of [[0, 0, 0, 255], [255, 255, 255, 255], [18, 52, 86, 255], [255, 0, 128, 255]]) {
    await record(`screen ${JSON.stringify(c)}`, (d) => d.setScreen(c));
  }

  const levels = [];
  const spectra = [new Array(1024).fill(0), new Array(1024).fill(200)];
  for (let k = 0; k < 20; k++) spectra.push(Array.from({ length: 1024 }, (_, i) => Math.max(0, Math.min(255, 220 - (i >> 2) + rnd(60) - 30))));
  for (const s of spectra) levels.push({ spectrum: hex(s.slice(0, 100)), levels: music3(Uint8Array.from(s), 6) });

  fs.writeFileSync(process.argv[2] || 'golden_live.json', JSON.stringify({
    meta: { generator: 'tools/re-harness/gen_golden_live.js', source: 'EPOMAKER Driver 1.2.66, YC500_TH80 + MusicFollowStore' },
    cases,
    levels,
  }, null, 1));
  console.error(`wrote ${cases.length} frame cases, ${levels.length} level cases`);
})();
