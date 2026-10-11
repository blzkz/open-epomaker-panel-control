// Fake transport replacing usb/client (HWv2). Records every packet exactly as iot_driver would send it.
const L = require('./loader');
const packets = [];
let responder = () => Buffer.alloc(64);
function applyChecksum(msg, type) {
  const b = Buffer.from(msg);
  if (type === 0) { let s = 0; for (let i = 0; i < 7; i++) s += b[i]; b[7] = (~s) & 0xff; }
  else if (type === 1) { let s = 0; for (let i = 0; i < 8; i++) s += b[i]; b[8] = (~s) & 0xff; }
  return b;
}
const client = {
  client: {},
  sendMsg: async (path, msg, ck, dangle) => { const b = applyChecksum(msg, ck); packets.push({ op: 'set', ck, data: [...b] }); responder.last = b; return true; },
  sendRawFeature: async (path, msg, ck, dangle) => { const b = applyChecksum(msg, ck); packets.push({ op: 'raw', ck, data: [...b] }); return true; },
  readMsg: async (path) => { const r = responder(responder.last, packets); packets.push({ op: 'get', data: r ? [...r] : null }); return r; },
  readRawFeature: async (path) => { const r = responder(responder.last, packets); packets.push({ op: 'getraw', data: r ? [...r] : null }); return r; },
  setLightType: async () => true,
  toggleMicrophoneMute: async () => false, getMicrophoneMute: async () => 2, muteMicrophone: async () => true,
  watchSystemInfo: async () => undefined,
};
L.overrides[L.findModule('exports.watchSystemInfo=')] = client;
L.overrides[L.findModule('exports.timeOutFUNC=')] = { sleep: async () => {}, timeOut: () => new Promise(() => {}), timeOutFUNC: (f) => f };
L.overrides[L.findModule('exports.mobxStore=')] = { mobxStore: { toastStore: { setErr: (e) => packets.push({ op: 'toastErr', msg: String(e) }) } } };
module.exports = { packets, setResponder: (f) => { responder = f; }, applyChecksum };
