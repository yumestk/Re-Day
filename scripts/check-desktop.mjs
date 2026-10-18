// Actual macOS WKWebView + Rust + SQLite. Requires the Vite server on :1420.
import assert from 'node:assert/strict';
import { spawn, spawnSync } from 'node:child_process';
import { mkdtemp, mkdir, readFile, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { resolve, join } from 'node:path';
import { setTimeout as delay } from 'node:timers/promises';

assert.equal(process.platform, 'darwin', 'This check targets macOS WKWebView.');
assert.equal(spawnSync('cargo', ['build', '--manifest-path', 'src-tauri/Cargo.toml', '--features', 'desktop-test'], { stdio: 'inherit' }).status, 0);
const root = await mkdtemp(join(tmpdir(), 'lifeisrpg-desktop-'));
const output = resolve('test-results');
await mkdir(output, { recursive: true });
const port = 4445;
let child, session;
async function request(path, body, method = body ? 'POST' : 'GET') {
  const response = await fetch(`http://127.0.0.1:${port}${path}`, { method, headers: { 'Content-Type': 'application/json' }, body: body ? JSON.stringify(body) : undefined, signal: AbortSignal.timeout(10_000) });
  const result = await response.json();
  if (!response.ok || result.value?.error) throw new Error(JSON.stringify(result));
  return result.value;
}
async function until(check, label) {
  const end = Date.now() + 12_000;
  while (Date.now() < end) { if (await check()) return; await delay(70); }
  throw new Error(`Timed out: ${label}`);
}
const execute = (script, ...args) => request(`/session/${session}/execute/sync`, { script, args });
async function element(selector) {
  const value = await request(`/session/${session}/element`, { using: 'css selector', value: selector });
  return value['element-6066-11e4-a52e-4f735466cecf'];
}
const click = async selector => request(`/session/${session}/element/${await element(selector)}/click`, {});
async function fill(selector, text) {
  const id = await element(selector);
  await request(`/session/${session}/element/${id}/clear`, {});
  await request(`/session/${session}/element/${id}/value`, { text });
}
const text = () => execute('return document.body.innerText');
async function has(value) { return (await text()).includes(value); }
async function screenshot(name) {
  await execute('window.scrollTo(0,0)');
  await delay(450);
  await writeFile(join(output, `${name}.png`), Buffer.from(await request(`/session/${session}/screenshot`), 'base64'));
}
async function start() {
  child = spawn(resolve('src-tauri/target/debug/lifeisrpg'), [], { env: { ...process.env, LIFEISRPG_TEST_DATA_DIR: root, TAURI_WEBDRIVER_PORT: String(port) }, stdio: ['ignore', 'pipe', 'pipe'] });
  child.stderr.on('data', chunk => process.stderr.write(chunk));
  await until(async () => { try { return !!await request('/status'); } catch { return false; } }, 'embedded WebDriver');
  session = (await request('/session', { capabilities: { alwaysMatch: {} } })).sessionId;
  await until(() => has('已保存到此设备'), 'Rust day data');
  assert.equal(await execute('return !!window.__TAURI_INTERNALS__'), true);
}
async function stop() {
  if (!child || child.exitCode !== null) return;
  child.kill('SIGTERM');
  await new Promise(resolve => child.once('exit', resolve));
  await delay(250);
}
async function sql(statement) {
  const pointer = JSON.parse(await readFile(join(root, 'active.json'), 'utf8'));
  const database = join(root, 'datasets', pointer.datasetId, 'lifeisrpg.sqlite3');
  const result = spawnSync('/usr/bin/sqlite3', [database, statement], { encoding: 'utf8' });
  assert.equal(result.status, 0, result.stderr);
  return result.stdout.trim();
}
async function create(title, slot, times = false) {
  await click(`button[aria-label="添加${slot}任务"]`);
  await until(async () => !!await execute('return !!document.querySelector("input[aria-label=任务标题]")'), 'editor');
  await fill('input[aria-label="任务标题"]', title);
  await fill('textarea[aria-label="补充说明"]', `下一步：${title}`);
  if (times) {
    await execute('document.querySelector("button[aria-expanded]").click()');
    await fill('input[aria-label="开始时间"]', '14:00');
    await fill('input[aria-label="结束时间"]', '15:30');
  }
  await click('button[type="submit"]');
  await until(async () => !!await execute('return !!document.querySelector("article[data-task-title="+JSON.stringify(arguments[0])+"]")', title), 'created task');
  await until(async () => !await execute('return !!document.querySelector("input[aria-label=任务标题]")'), 'editor saved');
}
const doneButton = title => `article[data-task-title="${title}"] button[aria-pressed]`;
const isDone = title => execute('return document.querySelector(arguments[0])?.getAttribute("aria-pressed")==="true"', doneButton(title));
const log = message => console.log(`PASS ${message}`);
try {
  await start();
  await create('阅读一章 Rust 设计', '上午');
  await create('推进 LifeIsRPG 的 Today 交互', '下午', true);
  await create('整理今天的生活片段', '晚上');
  log('create three slots with descriptions and optional times through real commands');
  await execute('window.__sounds=0;window.__animations=0;window.__animationTimes=[];const start=AudioBufferSourceNode.prototype.start;AudioBufferSourceNode.prototype.start=function(...args){window.__sounds++;return start.apply(this,args)};const animate=Element.prototype.animate;Element.prototype.animate=function(...args){window.__animations++;window.__animationTimes.push(performance.now());return animate.apply(this,args)}');
  const originalOrder = await execute('return Array.from(document.querySelectorAll("article")).map(e=>e.dataset.planId)');
  await click(doneButton('阅读一章 Rust 设计'));
  await click(doneButton('推进 LifeIsRPG 的 Today 交互'));
  await until(async () => await isDone('阅读一章 Rust 设计') && await isDone('推进 LifeIsRPG 的 Today 交互'), 'two independent completions');
  assert.deepEqual(await execute('return Array.from(document.querySelectorAll("article")).map(e=>e.dataset.planId)'), originalOrder);
  await until(async () => await execute('return window.__sounds') >= 2, 'Web Audio playback');
  assert.equal(await execute('return window.__animations'), 2);
  const animationGap = await execute('return window.__animationTimes[1]-window.__animationTimes[0]');
  assert.ok(animationGap < 420, `Second task must complete during first animation: ${animationGap}ms`);
  console.log(`Two completion feedbacks started ${animationGap.toFixed(1)}ms apart (420ms animation).`);
  await click(doneButton('阅读一章 Rust 设计'));
  await until(async () => !await isDone('阅读一章 Rust 设计'), 'undo');
  await click(doneButton('阅读一章 Rust 设计'));
  await until(() => isDone('阅读一章 Rust 设计'), 'redo');
  assert.equal(await sql("SELECT count(*) FROM execution_records WHERE kind='completion' AND revoked_at IS NULL;"), '2');
  log('complete / undo / redo, original row order, Web Audio and 420ms local feedback');
  await click('label[class*="hideCompleted"] input');
  assert.equal(await execute('return document.querySelectorAll("article").length'), 1);
  await click('label[class*="hideCompleted"] input');
  await click('button[aria-label="查看「整理今天的生活片段」详情"]');
  await fill('input[aria-label="任务标题"]', '整理生活片段与照片');
  await click('button[type="submit"]');
  await until(async () => has('整理生活片段与照片'), 'edited title');
  await screenshot('today');
  await click('button[aria-label="查看「整理生活片段与照片」详情"]');
  await screenshot('today-details');
  await click('button[aria-label="关闭任务详情"]');
  log('hide completed, edit metadata and detail sidebar');
  await sql("CREATE TRIGGER qa_fail BEFORE UPDATE OF status ON quests WHEN NEW.status='completed' BEGIN SELECT RAISE(ABORT,'QA failure'); END;");
  const before = await execute('return {sounds:window.__sounds,animations:window.__animations}');
  await click(doneButton('整理生活片段与照片'));
  await until(() => has('本次操作未保存'), 'save error');
  assert.equal(await isDone('整理生活片段与照片'), false);
  assert.deepEqual(await execute('return {sounds:window.__sounds,animations:window.__animations}'), before);
  assert.equal(await sql("SELECT count(*) FROM execution_records WHERE kind='completion' AND revoked_at IS NULL;"), '2');
  await sql('DROP TRIGGER qa_fail;');
  await execute('document.querySelector("button[aria-label=关闭错误提示]").click()');
  log('injected SQLite failure leaves no half-write, completion animation or success sound');
  await execute('Array.from(document.querySelectorAll("button")).find(e=>e.textContent.includes("声音与动效")).click()');
  await click('input[aria-label="交互音效"]');
  await until(async () => await sql("SELECT json_extract(value,'$.soundEnabled') FROM settings WHERE key='preferences';") === '0', 'mute persisted');
  await click('input[aria-label="减少动效"]');
  await until(async () => await execute('return document.querySelector("[data-reduced-motion]").dataset.reducedMotion') === 'true', 'reduced motion');
  await click('button[aria-label="关闭对话框"]');
  const muted = await execute('return {sounds:window.__sounds,animations:window.__animations}');
  await click(doneButton('整理生活片段与照片'));
  await until(() => isDone('整理生活片段与照片'), 'muted completion');
  assert.deepEqual(await execute('return {sounds:window.__sounds,animations:window.__animations}'), muted);
  log('mute + reduce motion persist and suppress audio/movement while state remains visible');
  await stop();
  await start();
  assert.equal(await execute('return document.querySelectorAll("article button[aria-pressed=true]").length'), 3);
  assert.equal(await execute('return document.querySelector("[data-reduced-motion]").dataset.reducedMotion'), 'true');
  assert.equal(await sql('PRAGMA integrity_check;'), 'ok');
  assert.equal(await sql('PRAGMA foreign_key_check;'), '');
  log('restart restores tasks, times, completed states and preferences; database integrity passes');
  // Exercise keyboard and native close-request handling, without adding a test-only product command.
  await execute('window.dispatchEvent(new KeyboardEvent("keydown",{key:"n",metaKey:true,bubbles:true}))');
  await until(async () => !!await execute('return !!document.querySelector("input[aria-label=任务标题]")'), 'Cmd+N');
  await fill('input[aria-label="任务标题"]', '退出前保存的草稿');
  await execute('window.__TAURI_INTERNALS__.invoke("plugin:window|close",{label:"main"})');
  await until(() => has('退出前，保存这次修改'), 'native close guard');
  await screenshot('unsaved-close');
  await execute('Array.from(document.querySelectorAll("dialog button")).find(e=>e.textContent.includes("保存并退出")).click()');
  await until(async () => child.exitCode !== null, 'close quits process');
  assert.equal(await sql("SELECT count(*) FROM quests WHERE title='退出前保存的草稿';"), '1');
  log('Cmd+N opens form; native close offers save; save commits before process exits');
  console.log(`Desktop check complete. Isolated data: ${root}\nScreenshots: ${output}`);
} finally { await stop(); }
