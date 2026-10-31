const test=require('node:test');
const assert=require('node:assert/strict');
const net=require('node:net');
const path=require('node:path');
const {spawn}=require('node:child_process');
const {once}=require('node:events');
const executable=process.env.DCS_TABLET_EXE||path.join(__dirname,'..','target','release','dcs-tablet.exe');
const port=Number(process.env.DCS_TABLET_HTTP_TEST_PORT||18194),base=`http://127.0.0.1:${port}`;
const delay=ms=>new Promise(resolve=>setTimeout(resolve,ms));

function connection(request,localAddress='127.0.0.1'){
 const socket=net.connect({host:'127.0.0.1',port,localAddress});
 let bytes='';socket.on('data',b=>bytes+=b);socket.on('error',()=>{});
 const connected=once(socket,'connect').then(()=>socket.write(request));
 const closed=new Promise(resolve=>socket.once('close',()=>resolve(bytes)));
 return {socket,connected,closed};
}
async function bounded(promise,ms=5000){
 let timer;
 try{return await Promise.race([promise,new Promise((_,reject)=>{timer=setTimeout(()=>reject(new Error(`Operation exceeded ${ms} ms`)),ms);})]);}
 finally{clearTimeout(timer);}
}
async function health(pid){
 const start=Date.now();const response=await fetch(base+'/health',{signal:AbortSignal.timeout(1000)});
 assert.equal(response.status,200);assert.equal((await response.json()).process_id,pid);
 return Date.now()-start;
}

test('HTTP ingress bounds slow clients and preserves health/shutdown', {timeout:30000},async t=>{
 const child=spawn(executable,['--demo','--port',String(port)],{stdio:['ignore','pipe','pipe']});
 let output='';child.stdout.on('data',b=>output+=b);child.stderr.on('data',b=>output+=b);
 const exited=once(child,'exit');const live=new Set();
 const open=(request,ip)=>{const c=connection(request,ip);live.add(c.socket);c.socket.once('close',()=>live.delete(c.socket));return c;};
 try{
  let status;
  for(let n=0;n<100;n++){
   if(child.exitCode!==null)throw new Error(output);
   try{status=await(await fetch(base+'/status',{signal:AbortSignal.timeout(500)})).json();break;}catch{await delay(50);}
  }
  assert.equal(status?.process_id,child.pid,'Own test server must be listening');
  const pid=child.pid;
  await t.test('four incomplete bodies cannot occupy application workers',async()=>{
   const slow=Array.from({length:4},()=>open(`POST /offer HTTP/1.1\r\nHost: localhost\r\nContent-Length: 64000\r\n\r\n{`));
   await Promise.all(slow.map(c=>c.connected));await delay(100);
   for(let n=0;n<3;n++){await health(pid);await delay(50);}
   const replies=await bounded(Promise.all(slow.map(c=>c.closed)),4200);
   for(const reply of replies)assert.match(reply,/^HTTP\/1\.1 408 /);
   await health(pid);
  });
  await t.test('incomplete headers expire without blocking health',async()=>{
   const slow=Array.from({length:4},()=>open('GET /health HTTP/1.1\r\nHost:'));
   await Promise.all(slow.map(c=>c.connected));await delay(100);await health(pid);
   for(const reply of await bounded(Promise.all(slow.map(c=>c.closed)),3000))assert.match(reply,/^HTTP\/1\.1 408 /);
  });
  await t.test('trickling bytes does not extend the absolute body deadline',async()=>{
   const slow=open('POST /offer HTTP/1.1\r\nHost: localhost\r\nContent-Length: 64000\r\n\r\n{');await slow.connected;
   const start=Date.now();const trickle=setInterval(()=>{if(!slow.socket.destroyed)slow.socket.write(' ');},100);
   try{
    const reply=await bounded(slow.closed,4200),elapsed=Date.now()-start;
    // A trickle can arrive while Windows closes the timed-out socket, discarding
    // the response with a reset. Still require the absolute deadline (not an
    // early rejection), and check that application health remains available.
    if(reply!=='')assert.match(reply,/^HTTP\/1\.1 408 /);
    assert.ok(elapsed>=2500 && elapsed<4200,`Body deadline: ${elapsed} ms`);
    await health(pid);
   }finally{clearInterval(trickle);}
  });
  await t.test('oversized and ambiguous requests are rejected before body reads',async()=>{
   const cases=[
    ['POST /offer HTTP/1.1\r\nHost: localhost\r\nContent-Length: 65537\r\n\r\n',413],
    ['POST /controls HTTP/1.1\r\nHost: localhost\r\nContent-Length: 513\r\n\r\n',413],
    ['POST /offer HTTP/1.1\r\nHost: localhost\r\nTransfer-Encoding: chunked\r\n\r\n',400],
    ['POST /offer HTTP/1.1\r\nHost: localhost\r\nContent-Length: 2\r\nContent-Length: 2\r\n\r\n',400],
    ['GET /health HTTP/1.1\r\nHost: localhost\r\nX-Large: '+ 'x'.repeat(9000),431,true],
   ];
   for(const [request,code,mayReset]of cases){
    const c=open(request);await c.connected;const reply=await bounded(c.closed,1500);
    // Windows can reset a connection closed with unread oversized input before
    // the 431 response arrives. A prompt close also enforces the size limit.
    if(!(mayReset && reply===''))assert.match(reply,new RegExp(`^HTTP/1.1 ${code} `));
   }
   const response=await fetch(base+'/offer',{method:'POST',body:'{}'});assert.equal(response.status,400);await health(pid);
  });
  await t.test('per-address connection cap rejects excess sockets',async()=>{
   // Separate loopback address leaves the supervisor's source address available.
   const slow=Array.from({length:8},()=>open('GET / HTTP/1.1\r\nHost:','127.0.0.2'));
   await Promise.all(slow.map(c=>c.connected));await delay(100);
   const excess=open('GET / HTTP/1.1\r\nHost:','127.0.0.2');await excess.connected;
   assert.equal(await bounded(excess.closed,1000),'');await health(pid);
   slow.forEach(c=>c.socket.destroy());await Promise.all(slow.map(c=>c.closed));await delay(50);
  });
  await t.test('shutdown closes incomplete requests promptly',async()=>{
   const slow=Array.from({length:4},()=>open('POST /offer HTTP/1.1\r\nHost: localhost\r\nContent-Length: 64000\r\n\r\n{'));
   await Promise.all(slow.map(c=>c.connected));await delay(100);
   const start=Date.now();const response=await fetch(base+'/shutdown',{method:'POST',signal:AbortSignal.timeout(1500)});
   assert.equal(response.status,200);await bounded(exited,1500);assert.ok(Date.now()-start<1500);
   await bounded(Promise.all(slow.map(c=>c.closed)),1000);
  });
 }catch(error){console.error(output);throw error;}
 finally{
  live.forEach(s=>s.destroy());
  if(child.exitCode===null){try{await fetch(base+'/shutdown',{method:'POST',signal:AbortSignal.timeout(1000)});}catch{}await Promise.race([exited,delay(1500)]);}
  if(child.exitCode===null){child.kill();await exited;}
 }
});
