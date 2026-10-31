const test = require('node:test');
const assert = require('node:assert/strict');
const vm = require('node:vm');
const fs = require('node:fs');
const path = require('node:path');

test('client reports failures, permits retry, and detects a restarted server', async () => {
  const elements = new Map();
  const element = selector => {
    if (!elements.has(selector)) elements.set(selector, {textContent:'',disabled:false,
      classList:{add(){},remove(){},toggle(){}},play:async()=>{},setAttribute(){}});
    return elements.get(selector);
  };
  let status = {backend:'rust',process_id:10,build_id:'test',version:'0.1.0',build_date:'2026-10-26',mode:'capture',fps:30,size:512,
    active_viewers:0,last_error:'Previous capture failure'};
  let offers=0;
  const peers=[];
  class Peer {
    constructor(){this.connectionState='new';this.iceGatheringState='complete';peers.push(this);}
    addTransceiver(){}
    async createOffer(){return {type:'offer',sdp:'test'};}
    async setLocalDescription(description){this.localDescription=description;}
    async setRemoteDescription(){this.connectionState='connected';this.onconnectionstatechange?.();}
    close(){this.closed=true;this.connectionState='closed';this.onconnectionstatechange?.();}
    async getStats(){return new Map();}
  }
  const context={
    document:{documentElement:{style:{setProperty(){}}},querySelector:element,querySelectorAll:()=>[],visibilityState:'visible',addEventListener(){}},
    window:{RTCPeerConnection:Peer,addEventListener(){}},RTCPeerConnection:Peer,navigator:{},
    AbortController,DOMException,MediaStream:class {},console,
    setInterval:()=>1,clearInterval(){},setTimeout:()=>1,clearTimeout(){},
    fetch:async(url)=>{
      if(url==='/offer'){offers++;status={...status,active_viewers:1,last_error:null};return {ok:true,json:async()=>({type:'answer',sdp:'test'})};}
      return {ok:true,json:async()=>({...status})};
    }
  };
  vm.createContext(context);
  const html=fs.readFileSync(path.join(__dirname,'..','web','index.html'),'utf8');
  vm.runInContext(html.match(/<script>([\s\S]*?)<\/script>/)[1],context);
  await new Promise(setImmediate);
  await element('#connect').onclick();
  assert.match(element('#build-info').textContent,/DCS Tablet v0\.1\.0 · Built 2026-10-26 UTC/);
  assert.equal(offers,1,'A retained error from the previous session must not prevent a new offer');
  assert.equal(peers[0].closed,undefined);

  status={...status,controls:{session:1,ready:false,bridge_connected:false}};
  await context.updateStatus();
  assert.equal(peers[0].closed,undefined,'A stale DCS control reply must not close video');
  assert.match(element('#control-status').textContent,/waiting for a reply/);
  status={...status,controls:{session:1,ready:false,bridge_connected:true}};
  await context.updateStatus();
  assert.match(element('#control-status').textContent,/Start a Hornet mission/);
  status={...status,controls:{session:1,ready:true,bridge_connected:true}};
  await context.updateStatus();
  assert.equal(element('#control-status').textContent,'Touch controls ready');

  status={...status,active_viewers:0,last_error:'Encoder worker failed'};
  await context.updateStatus();
  assert.equal(peers[0].closed,true);
  assert.match(element('#message').textContent,/Encoder worker failed/);
  await element('#connect').onclick();
  assert.equal(offers,2);
  assert.equal(peers[1].closed,undefined);

  status={...status,process_id:11};
  await context.updateStatus();
  assert.equal(peers[1].closed,true);
  assert.match(element('#message').textContent,/Streamer restarted/);
  assert.equal(element('#connect').disabled,false);
});
