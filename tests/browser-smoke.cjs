// Run against a real local executable. Playwright is a development-only dependency.
const assert = require('node:assert/strict');
const {spawn} = require('node:child_process');
const path = require('node:path');
const fs = require('node:fs');
const {chromium, webkit} = require(process.env.DCS_TABLET_PLAYWRIGHT || process.argv[2] || 'playwright');
const root=path.resolve(__dirname,'..');
const executable=process.env.DCS_TABLET_EXE || process.argv[3] || path.join(root,'target','release','dcs-tablet.exe');
const version=fs.readFileSync(path.join(root,'Cargo.toml'),'utf8').match(/version\s*=\s*"([^"]+)"/)[1];
const port=Number(process.env.DCS_TABLET_TEST_PORT || 18092),base=`http://127.0.0.1:${port}`;
const delay=ms=>new Promise(resolve=>setTimeout(resolve,ms));
const stream=spawn(executable,['--demo','--port',String(port)],{stdio:['ignore','pipe','pipe']});
let output='';stream.stdout.on('data',b=>output+=b);stream.stderr.on('data',b=>output+=b);
let browser;
(async()=>{
 try {
  let status;
  for(let n=0;n<100;n++){if(stream.exitCode!==null)throw new Error(output);try{const response=await fetch(base+'/status');status=await response.json();break;}catch{await delay(100);}}
  assert.ok(status,'Service became healthy');assert.equal(status.version,version);assert.equal(status.codec,'Media Foundation H.264');assert.match(status.build_date,/^\d{4}-\d{2}-\d{2}$/);assert.equal(status.test_hooks,false);
  assert.equal((await fetch(base+'/offer',{method:'POST',headers:{Origin:'http://unrelated.example'},body:'{}'})).status,403);
  assert.equal((await fetch(base+'/offer',{method:'POST',body:'{}'})).status,400);
  browser=await chromium.launch({headless:true,channel:process.env.DCS_TABLET_BROWSER_CHANNEL||undefined});
  const page=await browser.newPage({viewport:{width:1180,height:820},hasTouch:true});
  const errors=[];page.on('pageerror',error=>errors.push(error.message));
  await page.goto(base);
  assert.ok(await page.evaluate(()=>RTCRtpReceiver.getCapabilities('video').codecs.some(codec=>codec.mimeType.toLowerCase()==='video/h264')),'Browser lacks H.264; use DCS_TABLET_BROWSER_CHANNEL=chrome or msedge.');await page.waitForFunction(()=>document.querySelector('#build-info').textContent.includes('Built'));
  assert.equal(await page.locator('#build-info').innerText(),`DCS Tablet v${version} · Built ${status.build_date} UTC`);
  assert.equal(await page.locator('.bezel').count(),40);
  await page.locator('#connect').click();
  await page.evaluate(()=>{window.__testPeer=pc;window.__testIce=[];for(const event of ['connectionstatechange','iceconnectionstatechange'])pc?.addEventListener(event,()=>window.__testIce.push({event,state:window.__testPeer.connectionState,ice:window.__testPeer.iceConnectionState}));});
  await page.waitForSelector('#stage.playing',{timeout:25000});
  await page.waitForFunction(()=>document.querySelector('#source-video').videoWidth===1024);
  await delay(1500);
  const colors=await page.locator('.screen canvas').evaluateAll(canvases=>canvases.map(c=>[...c.getContext('2d').getImageData(64,64,1,1).data]));
  assert.ok(colors[0][1]>100 && colors[0][0]<50,JSON.stringify(colors));assert.ok(colors[1][2]>100 && colors[1][1]<100,JSON.stringify(colors));
  const health=await (await fetch(base+'/status')).json();assert.equal(health.active_viewers,1);
  assert.equal((await fetch(base+'/offer',{method:'POST',body:'{}'})).status,409);
  for(const view of ['left','both','right','both']){
    if(view==='both')await page.locator('button[data-view="both"]').click();else await page.locator(`.screen[data-zoom="${view}"]`).click();
    await page.waitForFunction(v=>document.querySelector('#stage').dataset.view===v,view);
    assert.equal(await page.locator('.panel:visible').count(),view==='both'?2:1);
  }
  await page.locator('#fullscreen').click();assert.ok(await page.locator('body').evaluate(b=>b.classList.contains('immersive')));
  await page.locator('#exit-focus').click();
  await page.setViewportSize({width:820,height:1180});await page.locator('#rotate-notice').waitFor({state:'visible'});
  assert.match(await page.locator('#rotate-notice').innerText(),/Rotate your tablet to landscape/);
  await page.setViewportSize({width:1180,height:820});await page.locator('#rotate-notice').waitFor({state:'hidden'});
  fs.mkdirSync(path.join(root,'test-results'),{recursive:true});await page.screenshot({path:path.join(root,'test-results','landscape.png')});
  await page.locator('#disconnect').click();
  for(let n=0;n<100;n++){status=await(await fetch(base+'/status')).json();if(!status.active_viewers)break;await delay(100);}
  assert.equal(status.active_viewers,0);
  const metrics=await(await fetch(base+'/metrics')).json();await delay(500);
  const idle=await(await fetch(base+'/metrics')).json();assert.equal(idle.captured_frames,metrics.captured_frames);
  // A second connection creates a fresh Media Foundation worker and must decode
  // immediately using the new encoder's initial parameter sets and keyframe.
  await page.locator('#connect').click();
  await page.waitForSelector('#stage.playing',{timeout:25000});
  await page.waitForFunction(()=>document.querySelector('#source-video').videoWidth===1024 && document.querySelector('#source-video').readyState>=2);
  await delay(500);
  assert.equal((await(await fetch(base+'/status')).json()).active_viewers,1);
  await page.locator('#disconnect').click();
  for(let n=0;n<100;n++){status=await(await fetch(base+'/status')).json();if(!status.active_viewers)break;await delay(100);}
  assert.equal(status.active_viewers,0);
  assert.deepEqual(errors,[]);
  await browser.close();browser=null;
  // WebKit checks layout and metadata. Safari decoding requires a physical iPad.
  browser=await webkit.launch({headless:true});
  const safari=await browser.newPage({viewport:{width:1180,height:820},hasTouch:true});await safari.goto(base);
  await safari.waitForFunction(()=>document.querySelector('#build-info').textContent.includes('Built'));
  assert.equal(await safari.locator('.panel:visible').count(),2);
  await safari.setViewportSize({width:820,height:1180});await safari.locator('#rotate-notice').waitFor({state:'visible'});
  console.log('PASS: HTTP validation, metadata, real H.264 decoding, panel colors, zoom, fullscreen, landscape/portrait prompt, viewer ownership, idle cleanup, WebKit layout.');
 } catch(error) {
  fs.mkdirSync(path.join(root,'test-results'),{recursive:true});
  for(const page of browser?.contexts().flatMap(context=>context.pages()) || []) {
   try{
    console.error('BROWSER DIAGNOSTICS',await page.evaluate(()=>({url:location.href,message:document.querySelector('#message')?.textContent,connection:window.__testPeer?.connectionState,ice:window.__testPeer?.iceConnectionState,timeline:window.__testIce})));
    await page.screenshot({path:path.join(root,'test-results','browser-failure.png')});
   }catch{}
  }
  try{console.error('SERVER STATUS',await(await fetch(base+'/status')).text());}catch{}
  throw error;
 } finally {
  await browser?.close();
  try{await fetch(base+'/shutdown',{method:'POST'});}catch{}
  for(let n=0;n<40&&stream.exitCode===null;n++)await delay(100);
  if(stream.exitCode===null)stream.kill();
 }
})().catch(error=>{console.error(error);console.error(output);process.exitCode=1;});
