// Execute the ArkTS IME adapter against the controller contract, including asynchronous focus changes.
// Usage: node scripts/test_native_ime.cjs <typescript module>
const assert = require('node:assert/strict');
const fs = require('node:fs');
const vm = require('node:vm');
const ts = require(process.argv[2] || 'typescript');
const callbacks = new Map(), calls = [], messages = [];
let resolveAttach;
const controller = {
  attach: (show, config) => { calls.push(['attach', show, config]); return new Promise(resolve => { resolveAttach = resolve; }); },
  detach: async () => { calls.push(['detach']); },
  on: (name, callback) => callbacks.set(name, callback),
  off: name => callbacks.delete(name),
  changeSelection: async (...args) => { calls.push(['selection', ...args]); },
  updateCursor: async cursor => { calls.push(['cursor', cursor]); },
  showTextInput: async () => { calls.push(['show']); }
};
const source = fs.readFileSync('entry/src/main/ets/input/NativeImeAdapter.ets', 'utf8');
const result = ts.transpileModule(source, { compilerOptions: { target: ts.ScriptTarget.ES2020, module: ts.ModuleKind.CommonJS } });
const moduleShim = { exports: {} };
vm.runInNewContext(result.outputText, {
  module: moduleShim, exports: moduleShim.exports, console,
  // The adapter's debounce calls setTimeout; resolve it as a microtask so the test is timing-free.
  setTimeout: fn => { Promise.resolve().then(fn); return 0; },
  clearTimeout: () => {},
  require: name => {
    if (name === '@kit.IMEKit') return { inputMethod: { getController: () => controller, TextInputType: { TEXT: 0, VISIBLE_PASSWORD: 7 }, EnterKeyType: { NEWLINE: 0 } } };
    if (name === '@kit.BasicServicesKit') return {};
    throw new Error(name);
  }
});
const { NativeImeAdapter, NativeImeState } = moduleShim.exports;
// The adapter takes a focus host (the ArkUI page that owns the hidden anchor). Track its calls
// separately so the controller-call assertions below stay clean.
const hostCalls = [];
const host = { focusAnchor: () => hostCalls.push('focusAnchor'), restoreEditor: () => hostCalls.push('restoreEditor') };
const adapter = new NativeImeAdapter(message => messages.push(JSON.parse(message)), host);
const settle = async () => {
  for (let i = 0; i < 50; i++) await Promise.resolve();
  await new Promise(r => setTimeout(r, 0));
  for (let i = 0; i < 50; i++) await Promise.resolve();
};
const state = (focus = 'first') => Object.assign(new NativeImeState(), {
  active: true, focus, text: '你好😀', start: 2, end: 4, x: 15, y: 20, width: 1, height: 24, showRequest: 1
});
async function main() {
  adapter.setOrigin(10, 80);
  adapter.update(state());
  assert.equal(calls.filter(c => c[0] === 'attach').length, 1);
  // The Chinese-IME fix hinges on the anchor gaining focus before attach; guard that ordering.
  assert.equal(hostCalls[0], 'focusAnchor');
  adapter.update(state()); // Updates during attach must not attach twice.
  resolveAttach(); await settle();
  assert.equal(calls.filter(c => c[0] === 'attach').length, 1);
  assert.equal(callbacks.get('getLeftTextOfCursor')(2), '你好');
  assert.equal(callbacks.get('getTextIndexAtCursor')(), 4);
  assert.equal(calls.find(c => c[0] === 'cursor')[1].top, 100);
  callbacks.get('setPreviewText')('ni', { start: -1, end: -1 });
  callbacks.get('setPreviewText')('你好', { start: -1, end: -1 });
  callbacks.get('insertText')('你好');
  callbacks.get('finishTextPreview')();
  assert.deepEqual(messages.map(m => [m.kind, m.text]), [['ime_preedit', 'ni'], ['ime_preedit', '你好'], ['ime_commit', '你好']]);
  callbacks.get('deleteLeft')(2);
  assert.equal(messages.at(-1).count, 2);
  callbacks.get('moveCursor')(3);
  assert.equal(messages.at(-1).code, 3);
  callbacks.get('selectByRange')({ start: 0, end: 2 });
  assert.equal(messages.at(-1).end, 2);
  adapter.stop(); await settle();
  // Listeners are registered once and never torn down (off/on churn drops the IME binding); assert they persist.
  assert.ok(callbacks.has('insertText'), 'listeners persist across detach (registered once)');
  assert.equal(calls.filter(c => c[0] === 'detach').length, 1);
  // Focus is handed back to the editor on detach so hardware keys keep reaching it.
  assert.ok(hostCalls.includes('restoreEditor'));
  // A focus lost while attach is pending must detach as soon as attach finishes.
  adapter.update(state('second'));
  adapter.stop(); resolveAttach(); await settle();
  assert.ok(callbacks.has('insertText'), 'listeners stay registered across a re-attach (no off/on churn)');
  assert.equal(calls.filter(c => c[0] === 'detach').length, 2);
  // Help/search automatic focus opens the keyboard on attachment, without a second show.
  adapter.update(state('help'));
  assert.equal(calls.at(-1)[1], true);
  adapter.update(state('help')); resolveAttach(); await settle();
  assert.equal(calls.filter(c => c[0] === 'show').length, 0);
  adapter.update(Object.assign(state('help'), {showRequest:2})); await settle();
  assert.equal(calls.filter(c => c[0] === 'show').length, 1);
  adapter.stop(); await settle();
  console.log('Passed: IME attachment, cursor origin, context/selection, composition without duplicate commit, deletion, focus races and cleanup.');
}
main().catch(error => { console.error(error); process.exitCode = 1; });
