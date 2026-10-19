// Offline release assets + real WKWebView audio graph. Does not use personal data.
import assert from 'node:assert/strict';
import { spawn, spawnSync } from 'node:child_process';
import { mkdtemp, mkdir, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { setTimeout as delay } from 'node:timers/promises';

assert.equal(process.platform, 'darwin');
assert.equal(spawnSync('npm', ['run', 'tauri', '--', 'build', '--no-bundle', '--features', 'desktop-test', '--config', JSON.stringify({ identifier: 'com.lifeisrpg.audio-check' })], { stdio: 'inherit' }).status, 0);
const root = await mkdtemp(join(tmpdir(), 'lifeisrpg-audio-'));
const port = 4446;
const child = spawn(resolve('src-tauri/target/release/lifeisrpg'), [], { env: { ...process.env, LIFEISRPG_TEST_DATA_DIR: root, TAURI_WEBDRIVER_PORT: String(port) }, stdio: ['ignore', 'pipe', 'pipe'] });
child.stderr.on('data', bytes => process.stderr.write(bytes));
let session;
async function request(path, body) {
  const r = await fetch(`http://127.0.0.1:${port}${path}`, { method: body ? 'POST' : 'GET', headers: { 'Content-Type': 'application/json' }, body: body ? JSON.stringify(body) : undefined, signal: AbortSignal.timeout(10_000) });
  const j = await r.json();
  if (!r.ok || j.value?.error) throw Error(JSON.stringify(j));
  return j.value;
}
async function until(fn, label) {
  const end = Date.now() + 15_000;
  while (Date.now() < end) { if (await fn()) return; await delay(50); }
  throw Error(`Timed out: ${label}`);
}
const execute = (script, ...args) => request(`/session/${session}/execute/sync`, { script, args });
async function click(css) {
  const e = await request(`/session/${session}/element`, { using: 'css selector', value: css });
  await request(`/session/${session}/element/${e['element-6066-11e4-a52e-4f735466cecf']}/click`, {});
}
const button = 'article[data-task-title="音效验收隔离任务"] button[aria-pressed]';
async function sound(label, completed) {
  await execute('window.__audio.starts=0;window.__audio.peak=0;');
  await click(button);
  await until(() => execute('return document.querySelector(arguments[0])?.getAttribute("aria-pressed")===arguments[1]', button, String(completed)), `${label} persisted`);
  await until(() => execute('return window.__audio.peak>0.01 && window.__audio.context.state==="running" && window.__audio.context.currentTime>window.__audio.startTime'), `${label} nonzero audio and advancing clock`);
  assert.equal(await execute('return window.__audio.starts'), 1);
  console.log(`PASS ${label}: ${JSON.stringify(await execute('return {peak:window.__audio.peak,state:window.__audio.context.state,time:window.__audio.context.currentTime}'))}`);
  await delay(350);
}
try {
  await until(async () => { try { return !!await request('/status'); } catch { return false; } }, 'WebDriver');
  session = (await request('/session', { capabilities: { alwaysMatch: {} } })).sessionId;
  await until(() => execute('return document.body.innerText.includes("已保存到此设备")'), 'isolated store');
  assert.equal(await execute('return location.protocol'), 'tauri:');
  // Tap actual PCM output; calling start() alone also succeeds on a dead context.
  await execute(`window.__audio={starts:0,peak:0,context:null,startTime:0};
    const start=AudioBufferSourceNode.prototype.start;
    AudioBufferSourceNode.prototype.start=function(...args){
      const c=this.context,a=c.createAnalyser(),b=new Float32Array(256);a.fftSize=256;this.connect(a);
      window.__audio.starts++;window.__audio.context=c;window.__audio.startTime=c.currentTime;
      const timer=setInterval(()=>{a.getFloatTimeDomainData(b);for(const v of b)window.__audio.peak=Math.max(window.__audio.peak,Math.abs(v));},5);
      setTimeout(()=>{clearInterval(timer);this.disconnect(a);},500);return start.apply(this,args);
    };`);
  await click('button[aria-label="添加上午任务"]');
  await until(() => execute('return !!document.querySelector("input[aria-label=任务标题]")'), 'task editor');
  await execute('const e=document.querySelector("input[aria-label=任务标题]");Object.getOwnPropertyDescriptor(HTMLInputElement.prototype,"value").set.call(e,"音效验收隔离任务");e.dispatchEvent(new Event("input",{bubbles:true}));');
  await click('button[type="submit"]');
  await until(() => execute('return !!document.querySelector(arguments[0])', button), 'created task');
  await sound('offline completion', true);
  await sound('undo', false);

  await execute('window.__audio.old=window.__audio.context;window.__audio.old.suspend();');
  await until(() => execute('return window.__audio.old.state==="suspended"'), 'suspended context');
  await sound('resume suspended session', true);
  assert.equal(await execute('return window.__audio.old===window.__audio.context'), true);

  await execute('window.__audio.old=window.__audio.context;window.__audio.old.close();');
  await until(() => execute('return window.__audio.old.state==="closed"'), 'closed context');
  await sound('recreate closed session', false);
  assert.equal(await execute('return window.__audio.old!==window.__audio.context'), true);

  await execute('window.__audio.old=window.__audio.context;window.dispatchEvent(new Event("focus"));');
  await until(() => execute('return window.__audio.old.state==="closed"'), 'foreground releases old session');
  await sound('fresh session after foreground return', true);
  assert.equal(await execute('return window.__audio.old!==window.__audio.context'), true);

  await execute('Array.from(document.querySelectorAll("button")).find(e=>e.textContent.includes("设置与数据")).click()');
  await click('input[aria-label="交互音效"]');
  await until(() => execute('const e=document.querySelector("input[aria-label=交互音效]");return !e.disabled&&!e.checked'), 'mute saved');
  await execute('Array.from(document.querySelectorAll("button")).find(e=>e.textContent.trim()==="Today今日行动").click()');
  await execute('window.__audio.starts=0;window.__audio.peak=0;');
  await click(button);
  await until(() => execute('return document.querySelector(arguments[0]).getAttribute("aria-pressed")==="false"', button), 'muted undo persisted');
  await delay(350);
  assert.equal(await execute('return window.__audio.starts'), 0);
  console.log('PASS mute saves task without starting sound');

  await execute('Array.from(document.querySelectorAll("button")).find(e=>e.textContent.includes("设置与数据")).click()');
  await click('input[aria-label="交互音效"]');
  await until(() => execute('const e=document.querySelector("input[aria-label=交互音效]");return !e.disabled&&e.checked'), 'unmute saved');
  await execute('Array.from(document.querySelectorAll("button")).find(e=>e.textContent.trim()==="Today今日行动").click()');
  await sound('unmute restores completion sound', true);
  await mkdir('test-results', { recursive: true });
  await writeFile('test-results/audio-release.json', JSON.stringify({ origin: await execute('return location.href'), isolatedData: root, result: 'passed', lastOutput: await execute('return {peak:window.__audio.peak,state:window.__audio.context.state,time:window.__audio.context.currentTime}') }, null, 2));
  console.log('PASS release audio regression checks');
} finally {
  if (child.exitCode === null) { child.kill('SIGTERM'); await new Promise(resolve => child.once('exit', resolve)); }
}
