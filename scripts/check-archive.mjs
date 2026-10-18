import assert from 'node:assert/strict';
import { mkdtemp, copyFile, readFile, writeFile, rename, chmod, stat } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { spawnSync } from 'node:child_process';

export async function checkArchive({ root, click, fill, execute, until, sql, has, screenshot, clickText, saveForm, stop, start, log }) {
  const fixtures=await mkdtemp(join(tmpdir(),'lifeisrpg-files-'));
  const first=join(fixtures,'first.png'),second=join(fixtures,'second.png'),moved=join(fixtures,'moved-original.png'),zip=join(fixtures,'complete.zip');
  await copyFile(resolve('src-tauri/icons/icon.png'),first);await copyFile(first,second);
  const choice=(purpose,paths)=>writeFile(join(root,'dialog.json'),JSON.stringify({purpose,paths}));
  const exists=selector=>execute('return !!document.querySelector(arguments[0])',selector);
  const pointer=async()=>JSON.parse(await readFile(join(root,'active.json'),'utf8')).datasetId;
  const card=title=>`article[data-event-title="${title}"]`;
  const [yesterday,today]=await execute('const fmt=d=>`${d.getFullYear()}-${String(d.getMonth()+1).padStart(2,"0")}-${String(d.getDate()).padStart(2,"0")}`;return [-1,0].map(i=>{const d=new Date();d.setDate(d.getDate()+i);return fmt(d)})');
  async function select(label,text) {await execute('const e=document.querySelector(arguments[0]);const o=Array.from(e.options).find(o=>o.textContent.includes(arguments[1]));if(!o)throw new Error("option missing");e.value=o.value;e.dispatchEvent(new Event("change",{bubbles:true}))',`select[aria-label="${label}"]`,text);}
  async function open(title) {await click(`button[aria-label="编辑生活事件：${title}"]`);await until(()=>exists('input[aria-label="事件标题"]'),'event editor loaded');}
  async function imagesReady(scope='main') {await until(()=>execute('const images=Array.from(document.querySelectorAll(arguments[0]+" img"));return images.length>0&&images.every(i=>i.complete&&i.naturalWidth>0)',scope),'private asset images decoded by WKWebView');}
  async function probe(url) {await execute('window.__imageProbe=null;const image=new Image();image.onload=()=>window.__imageProbe=true;image.onerror=()=>window.__imageProbe=false;image.src=arguments[0]+"?probe="+crypto.randomUUID()',url);await until(async()=>await execute('return window.__imageProbe')!==null,'asset scope probe');return execute('return window.__imageProbe');}
  async function remove(title) {await open(title);await clickText('删除生活事件');await clickText('确认删除事件','dialog button');await until(async()=>!await exists('aside form'),'event deleted');await until(async()=>!await exists(card(title)),'deleted event leaves timeline');}
  await clickText('Archive生活记录');
  await clickText('＋ 生活事件');
  await fill('input[aria-label="事件标题"]','第一份成果记录');
  await fill('input[aria-label="事件日期"]',yesterday);
  await fill('textarea[aria-label="事件备注"]','保留真实成果、原图与目标关联');
  await fill('input[aria-label="事件标签"]','学习，日常');
  await click('input[aria-label="标记为里程碑"]');
  await select('事件关联目标','准备日常可用版本');
  await select('事件关联任务','跨日技术文档');
  await choice('images',[first,second]);await clickText('选择图片');
  await until(async()=>await execute('return document.querySelectorAll("[data-editor-image]").length')===2,'two staged images');
  await imagesReady('aside');
  assert.equal(await sql('SELECT count(*) FROM life_events;'),'0');assert.equal(await sql('SELECT count(*) FROM assets;'),'0');
  await sql("CREATE TRIGGER qa_image_failure BEFORE INSERT ON life_event_assets BEGIN SELECT RAISE(ABORT,'QA event image failure'); END;");
  await click('button[type="submit"]');await until(()=>has('本次操作未保存'),'event failure is visible');assert.equal(await sql('SELECT count(*) FROM life_events;'),'0');assert.equal(await sql('SELECT count(*) FROM assets;'),'0');
  await sql('DROP TRIGGER qa_image_failure;');await saveForm();await until(()=>exists(card('第一份成果记录')),'event committed with images');await rename(first,moved);await imagesReady();
  assert.equal(await sql('SELECT count(*) FROM assets;'),'2');
  const originalUrl=await execute('return document.querySelector(arguments[0]+" img").src',card('第一份成果记录'));
  const outsideUrl=await execute('return window.__TAURI_INTERNALS__.convertFileSrc(arguments[0])',moved);
  assert.equal(await probe(outsideUrl),false,'original selected files are not granted webview filesystem access');
  await click(`${card('第一份成果记录')} button[aria-label="查看生活记录图片"]`);
  await until(()=>exists('dialog img'),'full image preview');await click('button[aria-label="关闭对话框"]');
  await screenshot('phase-5-archive');
  await open('第一份成果记录');await click('input[aria-label="标记为里程碑"]');await saveForm();await click('input[aria-label="仅里程碑"]');await until(async()=>!await exists(card('第一份成果记录')),'unmarked event stays but leaves milestone filter');assert.equal(await sql('SELECT count(*) FROM life_events;'),'1');await click('input[aria-label="仅里程碑"]');
  await clickText('＋ 生活事件');await fill('input[aria-label="事件标题"]','同图的第二次记录');await choice('images',[moved]);await clickText('选择图片');await until(async()=>await execute('return document.querySelectorAll("[data-editor-image]").length')===1,'existing asset selected');await saveForm();await until(()=>exists(card('同图的第二次记录')),'second event');assert.equal(await sql('SELECT count(*) FROM assets;'),'2');
  const sharedId=await sql("SELECT asset_id FROM life_event_assets WHERE life_event_id=(SELECT id FROM life_events WHERE title='同图的第二次记录');");assert.equal(await sql("SELECT count(*) FROM life_event_assets WHERE asset_id='"+sharedId+"';"),'2');
  log('multi-image event save is atomic, originals can move, milestone toggles preserve events, duplicate import reuses referenced resources');

  await clickText('Calendar日历与计划');await fill('input[aria-label="日历日期"]',yesterday);await until(()=>has('第一份成果记录'),'event in actual-date Calendar');
  await clickText('Review每日回顾');await fill('input[aria-label="回顾日期"]',yesterday);await until(()=>has('第一份成果记录'),'event in Daily Review');
  await clickText('Quests目标与任务');await click('button[data-goal-name="准备日常可用版本"]');await until(()=>exists('input[aria-label="目标名称"]'),'completed Goal detail');await clickText('记录为里程碑');await until(()=>exists('input[aria-label="事件标题"]'),'Goal milestone event');assert.equal(await execute('return document.querySelector("input[aria-label=标记为里程碑]").checked'),true);await fill('input[aria-label="事件标题"]','目标成果留痕');await saveForm();
  await clickText('Settings设置与数据');await until(()=>has('完整备份与恢复'),'Settings data');await choice('export',[zip]);await clickText('导出完整备份');await until(()=>has('完整备份已导出'),'export complete');
  const exported=spawnSync('/usr/bin/unzip',['-p',zip,'manifest.json'],{encoding:'utf8'});assert.equal(exported.status,0);const manifest=JSON.parse(exported.stdout);assert.equal(manifest.files.filter(f=>f.path.startsWith('images/')).length,2);assert.ok(manifest.files.some(f=>f.path==='lifeisrpg.sqlite3'));
  const beforeRecords=await sql('SELECT count(*) FROM execution_records;');const beforeQuests=await sql('SELECT count(*) FROM quests;');
  await click('input[aria-label="交互音效"]');await until(async()=>await sql("SELECT json_extract(value,'$.soundEnabled') FROM settings WHERE key='preferences';")==='1','post-export preference change');
  await clickText('Archive生活记录');await remove('第一份成果记录');await imagesReady();assert.equal(await sql('SELECT count(*) FROM assets;'),'1');assert.equal(await sql("SELECT count(*) FROM life_event_assets WHERE asset_id='"+sharedId+"';"),'1');
  await clickText('＋ 生活事件');await fill('input[aria-label="事件标题"]','备份之后的临时事件');await saveForm();await until(()=>exists(card('备份之后的临时事件')),'post-export event');
  const old=await pointer();const oldPath=join(root,'datasets',old);const oldImage=join(oldPath,await sql("SELECT relative_path FROM assets WHERE id='"+sharedId+"';"));
  const oldUrl=await execute('return document.querySelector(arguments[0]+" img").src',card('同图的第二次记录'));
  await clickText('Settings设置与数据');await choice('restore',[zip]);await clickText('选择备份并校验');await until(()=>has('确认完整替换恢复'),'restore inspect preview');assert.equal(await execute('return Array.from(document.querySelectorAll("dialog button")).find(b=>b.textContent==="确认替换并恢复").disabled'),true);assert.equal(await pointer(),old);await screenshot('phase-5-restore-preview');await clickText('取消，保留当前数据','dialog button');await until(async()=>!await exists('dialog'),'restore cancelled');assert.equal(await pointer(),old);assert.equal(await sql("SELECT count(*) FROM life_events WHERE title='备份之后的临时事件';"),'1');
  const bad=join(fixtures,'broken.zip');await writeFile(bad,'not a ZIP');await choice('restore',[bad]);await clickText('选择备份并校验');await until(()=>has('备份 ZIP 无法读取或写入'),'bad backup rejected');assert.equal(await pointer(),old);
  await choice('restore',[zip]);await clickText('选择备份并校验');await until(()=>has('确认完整替换恢复'),'valid restore preview');
  await chmod(oldPath,0o555);await chmod(join(oldPath,'images'),0o555);
  await click('input[aria-label="确认完整替换数据"]');await clickText('确认替换并恢复','dialog button');await until(()=>has('数据已恢复') .then(v=>v||has('完整数据与图片已恢复')),'restore result notice');
  await until(async()=>await pointer()!==old,'atomic active dataset switch');await until(()=>exists('input[aria-label="查看日期"]'),'frontend resets to Today');
  assert.equal(await sql('SELECT count(*) FROM execution_records;'),beforeRecords);assert.equal(await sql('SELECT count(*) FROM quests;'),beforeQuests);assert.equal(await sql("SELECT count(*) FROM life_events WHERE title='备份之后的临时事件';"),'0');assert.equal(await sql("SELECT json_extract(value,'$.soundEnabled') FROM settings WHERE key='preferences';"),'0');
  assert.ok((await stat(oldImage)).isFile(),'cleanup failure preserves old files for retry');assert.equal(await probe(oldUrl),false,'old image scope is revoked even while old file remains');
  await execute('window.__staleResult=null;window.__TAURI_INTERNALS__.invoke("save_review",{request:{datasetId:arguments[0],date:arguments[1],action:"reviewed"}}).then(()=>window.__staleResult="allowed").catch(e=>window.__staleResult=e.code)',old,today);await until(async()=>await execute('return window.__staleResult')!==null,'old session request rejected');assert.equal(await execute('return window.__staleResult'),'CONFLICT');
  await chmod(oldPath,0o755);await chmod(join(oldPath,'images'),0o755);await clickText('Settings设置与数据');await clickText('重试资源清理');await until(()=>has('无引用资源已清理'),'old dataset cleanup retry');assert.equal(await stat(oldPath).then(()=>true).catch(()=>false),false);assert.ok((await stat(zip)).isFile(),'exported backup is not deleted by restore');
  await clickText('Archive生活记录');await until(()=>exists(card('第一份成果记录')),'restored event returns');await imagesReady();assert.equal(await sql('SELECT count(*) FROM assets;'),'2');await screenshot('phase-5-restored');
  log('complete ZIP includes images; cancelled/bad restores preserve current state; confirmed restore replaces data and preferences, revokes old assets and rejects stale writes');
  await stop();await start();await clickText('Archive生活记录');await until(()=>exists(card('第一份成果记录')),'restored events survive restart');await imagesReady();assert.equal(await sql('PRAGMA integrity_check;'),'ok');assert.equal(await sql('PRAGMA foreign_key_check;'),'');
  await remove('第一份成果记录');await imagesReady();assert.equal(await sql('SELECT count(*) FROM assets;'),'1');await remove('同图的第二次记录');assert.equal(await sql('SELECT count(*) FROM assets;'),'0');assert.equal(await sql('SELECT count(*) FROM life_events;'),'1');
  log('restored images survive restart; deleting shared references cleans only the final unused resource');
  await clickText('Today今日行动');
}
