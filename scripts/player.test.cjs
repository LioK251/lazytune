const {test}=require('node:test');
const assert=require('node:assert/strict');
const vm=require('node:vm');
const fs=require('node:fs');
const source=fs.readFileSync(require('node:path').join(__dirname,'../desktop/player.js'),'utf8');
function environment(origin='https://music.youtube.com',config){
  const reports=[],calls=[],events={};const video={readyState:4,duration:180,currentTime:5,paused:false,ended:false,volume:.5,muted:false,addEventListener(name,fn){(events[name]??=[]).push(fn);}};
  const initialResponse={videoDetails:{videoId:'dQw4w9WgXcQ',lengthSeconds:'180',thumbnail:{thumbnails:[]}}};
  const player={getVideoData:()=>({video_id:'dQw4w9WgXcQ',title:'A song',author:'Artist'}),getPlayerResponse:()=>initialResponse,getCurrentTime:()=>video.currentTime,getDuration:()=>video.duration,classList:{contains:()=>false},getPlayerState:()=>1,pauseVideo:()=>calls.push('pause'),playVideo:()=>calls.push('play'),seekTo:n=>{video.currentTime=n;calls.push(['seek',n]);},setVolume:n=>{video.volume=n/100;calls.push(['volume',n]);}};
  const app={handleNavigationEndpoint:e=>calls.push(e),networkManager:{fetch:async(endpoint,body)=>{calls.push({endpoint,body});return {contents:[],microformat:{microformatDataRenderer:{urlCanonical:'https://music.youtube.com/playlist?list=OLAKalbum'}}};}}};
  const toggle={audioOnlyPlaybackAvailable:true,selectedItemHasVideo:true,toggleDisabled:false,playbackMode:'ATV_PREFERRED',onSongAvToggleTap(){calls.push('audio');this.playbackMode='ATV_PREFERRED';},onVideoAvToggleTap(){calls.push('video');this.playbackMode='OMV_PREFERRED';}};
  const nodes={'#movie_player':player,'video':video,'#movie_player video':video,'ytmusic-app':app,'ytmusic-av-toggle':toggle,'#lazytune-video-style':{},'.next-button.ytmusic-player-bar':{click:()=>calls.push('next')},'.previous-button.ytmusic-player-bar':{click:()=>calls.push('previous')}};
  const documentEvents={},document={querySelector:s=>nodes[s]??null,documentElement:{dataset:{}},head:{append(){}},addEventListener(name,fn){(documentEvents[name]??=[]).push(fn);}};
  const window={ytcfg:{get:()=>true},__lazytuneConfig:config,__TAURI_INTERNALS__:{invoke:async(_,v)=>reports.push(JSON.parse(v.report))}};window.top=window;
  const handlers={},mediaSession={setActionHandler(name,fn){handlers[name]=fn;}};
  let tick,now=1000,id=0;const timers=new Map(),intervals=new Map();
  const context=vm.createContext({window,document,navigator:{mediaSession},URL,Uint8Array,location:{origin},performance:{now:()=>now},setTimeout(fn,ms){timers.set(++id,{fn,at:now+ms});return id;},setInterval(fn,ms){if(ms===1000)tick=fn;intervals.set(++id,{fn,ms});return id;},clearInterval(id){intervals.delete(id);},Response:class{async json(){return {playabilityStatus:{status:'OK'},adPlacements:[1]}}}});
  vm.runInContext(source,context);return {context,window,video,player,reports,calls,toggle,tick,nodes,handlers,intervals,fire(name){for(const fn of events[name]??[])fn();},advance(ms){now+=ms;},async elapse(ms){for(let left=ms;left>0;left-=100){now+=Math.min(100,left);for(const [id,t] of timers){if(t.at<=now){timers.delete(id);t.fn();}}for(let n=0;n<5;n++)await Promise.resolve();}},sample(){for(const t of intervals.values())if(t.ms===50)t.fn();}};
}
test('bridge is not installed on an unrelated origin',()=>assert.equal(environment('https://evil.test').window.__lazytune,undefined));
test('a confirmed cold cued song is primed before media readiness',async()=>{const e=environment();e.video.readyState=0;e.video.paused=true;e.player.getPlayerState=()=>5;await e.window.__lazytune.request(1,'command',{type:'play',value:'dQw4w9WgXcQ'});assert.equal(e.calls.filter(c=>c==='play').length,1);assert.equal(e.reports.at(-2).snapshot.play_requested,true);});
test('metadata gaps do not report a false pause or erase artwork',()=>{const e=environment();const response={videoDetails:{videoId:'dQw4w9WgXcQ',lengthSeconds:'180',thumbnail:{thumbnails:[{url:'https://i.ytimg.com/vi/dQw4w9WgXcQ/hqdefault.jpg'}]}}};e.player.getPlayerResponse=()=>response;e.window.__lazytune.report(true);e.player.getPlayerResponse=()=>({});e.window.__lazytune.report(true);const s=e.reports.at(-1).snapshot;assert.equal(s.playing,true);assert.equal(s.track.artwork,response.videoDetails.thumbnail.thumbnails[0].url);assert.equal(s.duration,180);});
test('cold navigation waits for hydration and respects a newer Pause',async()=>{const e=environment();const navigate=e.nodes['ytmusic-app'].handleNavigationEndpoint;delete e.nodes['ytmusic-app'].handleNavigationEndpoint;const pending=e.window.__lazytune.request(1,'command',{type:'play',value:'xsdTzjogYBM'});await e.elapse(100);await e.window.__lazytune.request(2,'command',{type:'pause'});e.nodes['ytmusic-app'].handleNavigationEndpoint=navigate;await e.elapse(100);await pending;assert.equal(e.calls.some(c=>c?.watchEndpoint),false);assert.equal(e.reports.find(r=>r.kind==='reply'&&r.id===1).error,undefined);});
test('an uncommanded pause resumes after a grace period without requiring video',async()=>{const e=environment();await e.window.__lazytune.request(1,'command',{type:'play',value:'dQw4w9WgXcQ'});e.window.__lazytune.report(true);e.video.paused=true;e.player.getPlayerState=()=>2;e.fire('pause');e.window.__lazytune.report(true);e.advance(999);e.window.__lazytune.report(true);assert.equal(e.calls.filter(c=>c==='play').length,1);e.advance(1);e.window.__lazytune.report(true);assert.equal(e.calls.filter(c=>c==='play').length,2);});
test('catalog requests use the site network manager without navigating playback',async()=>{const e=environment();await e.window.__lazytune.request(7,'catalog',{type:'search',value:'test'});assert.equal(e.calls[0].endpoint,'/search');assert.equal(e.calls[0].body.query,'test');assert.equal(e.reports[0].id,7);});
test('play and pause use the existing song without reloading it',async()=>{const e=environment();await e.window.__lazytune.request(1,'command',{type:'play',value:'dQw4w9WgXcQ'});assert.deepEqual(e.calls,['play']);await e.window.__lazytune.request(2,'command',{type:'toggle'});assert.ok(e.calls.includes('pause'));});
test('the first play starts once the requested song is ready, without replaying the old song',async()=>{
  const e=environment();e.video.readyState=0;e.player.getPlayerState=()=>-1;
  await e.window.__lazytune.request(1,'command',{type:'play',value:'xsdTzjogYBM'});
  assert.equal(e.calls.includes('play'),false);
  e.video.readyState=4;e.window.__lazytune.report(true);
  assert.equal(e.calls.includes('play'),false);
  e.player.getVideoData=()=>({video_id:'xsdTzjogYBM'});
  e.player.getPlayerResponse=()=>({videoDetails:{videoId:'xsdTzjogYBM',lengthSeconds:'103'}});
  e.window.__lazytune.report(true);e.window.__lazytune.report(true);
  assert.equal(e.calls.filter(c=>c==='play').length,1);
});
test('pausing a loading song cancels its pending play',async()=>{
  const e=environment();e.video.readyState=0;
  await e.window.__lazytune.request(1,'command',{type:'play',value:'dQw4w9WgXcQ'});
  const attempts=e.calls.filter(c=>c==='play').length;
  await e.window.__lazytune.request(2,'command',{type:'pause'});
  e.video.readyState=4;e.window.__lazytune.report(true);
  assert.equal(e.calls.filter(c=>c==='play').length,attempts);
});
test('one skip or collection click starts the newly loaded song',async()=>{
  for(const command of [{type:'next'},{type:'previous'},{type:'play_collection',value:'VLPLtest'}]){
    const e=environment();e.player.getPlayerState=()=>2;e.video.paused=true;
    await e.window.__lazytune.request(1,'command',command);
    assert.equal(e.calls.includes('play'),false,'navigation must not resume the old track');
    const response={videoDetails:{videoId:'xsdTzjogYBM',lengthSeconds:'103'}};
    e.player.getVideoData=()=>({video_id:'xsdTzjogYBM'});e.player.getPlayerResponse=()=>response;
    e.window.__lazytune.report(true);
    assert.equal(e.calls.filter(c=>c==='play').length,1);
    e.player.getPlayerState=()=>1;e.video.paused=false;e.window.__lazytune.report(true);
    e.advance(1000);e.window.__lazytune.report(true);
    assert.equal(e.calls.filter(c=>c==='play').length,1);
  }
});
test('Previous starts a song when YouTube restarts its existing timeline',async()=>{
  const e=environment();e.player.getPlayerState=()=>2;e.video.paused=true;e.video.currentTime=60;
  await e.window.__lazytune.request(1,'command',{type:'previous'});
  e.video.currentTime=0;e.window.__lazytune.report(true);
  assert.equal(e.calls.filter(c=>c==='play').length,1);
});
test('pending navigation rejects seeks to the previous song',async()=>{
  const e=environment();
  await e.window.__lazytune.request(1,'command',{type:'play',value:'xsdTzjogYBM'});
  await e.window.__lazytune.request(2,'command',{type:'seek',value:{position:0,track_id:'dQw4w9WgXcQ'}});
  assert.equal(e.video.currentTime,5);
  assert.match(e.reports.at(-1).error,/track changed/);
});
test('an audio counterpart keeps the intent to play after its ID changes',async()=>{
  const e=environment();e.player.getPlayerState=()=>2;e.video.paused=true;
  await e.window.__lazytune.request(1,'command',{type:'play',value:'xsdTzjogYBM'});
  e.toggle.playbackMode='OMV_PREFERRED';e.tick();
  assert.equal(e.calls.includes('audio'),false,'a pending selection must not switch the old song');
  e.player.getVideoData=()=>({video_id:'xsdTzjogYBM'});e.player.getPlayerResponse=()=>({videoDetails:{videoId:'xsdTzjogYBM',lengthSeconds:'103'}});e.tick();
  const response={videoDetails:{videoId:'abcdefghijk',lengthSeconds:'103'}};
  e.player.getVideoData=()=>({video_id:'abcdefghijk'});e.player.getPlayerResponse=()=>response;
  e.advance(600);
  e.window.__lazytune.report(true);
  assert.equal(e.calls.filter(c=>c==='play').length,1);
});
test('a late cue can retry playback, but pause and expired navigation cancel retries',async()=>{
  const e=environment();e.player.getPlayerState=()=>2;e.video.paused=true;
  await e.window.__lazytune.request(1,'command',{type:'toggle'});
  assert.equal(e.calls.filter(c=>c==='play').length,1);
  e.advance(600);e.window.__lazytune.report(true);
  assert.equal(e.calls.filter(c=>c==='play').length,2);
  await e.window.__lazytune.request(2,'command',{type:'pause'});
  e.advance(1000);e.window.__lazytune.report(true);
  assert.equal(e.calls.filter(c=>c==='play').length,2);
  e.video.readyState=0;
  await e.window.__lazytune.request(3,'command',{type:'resume'});
  const attempts=e.calls.filter(c=>c==='play').length;
  e.advance(20001);e.video.readyState=4;e.window.__lazytune.report(true);
  assert.equal(e.calls.filter(c=>c==='play').length,attempts);
});
test('ad filtering is reversible and preserves unrelated data',()=>{const e=environment();const payload='{"playabilityStatus":{"status":"OK"},"adPlacements":[1],"videoDetails":{"title":"Keep me"}}';const first=vm.runInContext(`JSON.parse(${JSON.stringify(payload)})`,e.context);assert.equal(first.adPlacements,undefined);assert.equal(first.videoDetails.title,'Keep me');e.window.__lazytune.configure({adblock:false});assert.equal(vm.runInContext(`JSON.parse(${JSON.stringify(payload)})`,e.context).adPlacements.length,1);});
test('snapshot normalizes invalid media timing',()=>{const e=environment();e.video.duration=Infinity;e.player.getPlayerResponse=()=>({videoDetails:{videoId:'dQw4w9WgXcQ',lengthSeconds:'invalid'}});e.window.__lazytune.report(true);assert.equal(e.reports[0].snapshot.duration,0);assert.equal(e.reports[0].snapshot.seekable,false);});

test('song duration stays fixed when YouTube changes its media timeline',async()=>{
  const e=environment();
  e.player.getPlayerResponse=()=>({videoDetails:{videoId:'dQw4w9WgXcQ',lengthSeconds:'103'}});
  e.player.getCurrentTime=()=>58.898;
  e.video.currentTime=363.934;e.video.duration=407.849;
  for(const duration of [49,102.813,0]){
    e.player.getDuration=()=>duration;
    e.window.__lazytune.report(true);
    const snapshot=e.reports.at(-1).snapshot;
    assert.equal(snapshot.duration,103);
    assert.equal(snapshot.position,58.898);
    assert.equal(snapshot.seekable,true);
  }
  await e.window.__lazytune.request(1,'command',{type:'seek',value:{position:999,track_id:'dQw4w9WgXcQ'}});
  assert.deepEqual(e.calls.at(-1),['seek',103]);
});

test('temporary missing player timing cannot leak the raw media clock or reset a song',()=>{
  const e=environment();
  e.player.getPlayerResponse=()=>({videoDetails:{videoId:'dQw4w9WgXcQ',lengthSeconds:'103'}});
  e.player.getCurrentTime=()=>58.898;
  e.window.__lazytune.report(true);
  e.player.getCurrentTime=()=>undefined;
  e.player.getPlayerResponse=()=>({videoDetails:{videoId:'dQw4w9WgXcQ'}});
  e.video.currentTime=363.934;e.video.duration=407.849;
  e.window.__lazytune.report(true);
  assert.equal(e.reports.at(-1).snapshot.position,58.898);
  assert.equal(e.reports.at(-1).snapshot.duration,103);
  e.player.getVideoData=()=>({video_id:'xsdTzjogYBM',title:'Next song'});
  e.window.__lazytune.report(true);
  assert.equal(e.reports.at(-1).snapshot.duration,0);
  assert.equal(e.reports.at(-1).snapshot.position,0);
});

test('a confirmed track stays seekable while buffering or finishing an earlier seek',async()=>{const e=environment();e.player.getCurrentTime=()=>30;e.player.getDuration=()=>180;e.video.currentTime=5;e.video.seeking=true;e.player.getPlayerState=()=>3;e.window.__lazytune.report(true);assert.equal(e.reports.at(-1).snapshot.seekable,true);await e.window.__lazytune.request(1,'command',{type:'seek',value:{position:12,track_id:'dQw4w9WgXcQ'}});assert.equal(e.video.currentTime,12);e.video.readyState=0;e.window.__lazytune.report(true);assert.equal(e.reports.at(-1).snapshot.seekable,false);});
test('seek changes the media position, including returning to zero',async()=>{const e=environment();await e.window.__lazytune.request(1,'command',{type:'seek',value:{position:90,track_id:'dQw4w9WgXcQ'}});assert.equal(e.video.currentTime,90);await e.window.__lazytune.request(2,'command',{type:'seek',value:{position:0,track_id:'dQw4w9WgXcQ'}});assert.equal(e.video.currentTime,0);await e.window.__lazytune.request(3,'command',{type:'seek',value:{position:999,track_id:'dQw4w9WgXcQ'}});assert.equal(e.video.currentTime,180);});

test('leaving video mode restores audio and prevents automatic video selection',()=>{const e=environment();e.window.__lazytune.mode('video');assert.equal(e.toggle.playbackMode,'OMV_PREFERRED');e.window.__lazytune.mode('audio');assert.equal(e.toggle.playbackMode,'ATV_PREFERRED');e.toggle.playbackMode='OMV_PREFERRED';e.tick();assert.equal(e.toggle.playbackMode,'ATV_PREFERRED');assert.equal(e.calls.filter(c=>c==='video').length,1);});
test('playlist and album play keep the collection queue',async()=>{const e=environment();await e.window.__lazytune.request(1,'command',{type:'play_collection',value:'VLPLtest'});assert.equal(e.calls[0].watchPlaylistEndpoint.playlistId,'PLtest');await e.window.__lazytune.request(2,'command',{type:'play_collection',value:'MPREalbum'});assert.equal(e.calls[1].body.browseId,'MPREalbum');assert.equal(e.calls[2].watchPlaylistEndpoint.playlistId,'OLAKalbum');});
test('automatic music videos are opt-in and disabling the preference restores audio',()=>{const e=environment();e.tick();assert.equal(e.calls.includes('video'),false);e.window.__lazytune.configure({adblock:true,auto_video:true});assert.equal(e.toggle.playbackMode,'OMV_PREFERRED');e.window.__lazytune.configure({adblock:true,auto_video:false});assert.equal(e.toggle.playbackMode,'ATV_PREFERRED');});
test('saved volume is restored once, including zero, without overriding later input',async()=>{for(const volume of [.37,0]){const e=environment(undefined,{adblock:true,volume});e.tick();assert.equal(e.video.volume,volume);assert.equal(e.reports[0].snapshot.volume,volume);await e.window.__lazytune.request(1,'command',{type:'volume',value:.6});e.tick();assert.equal(e.video.volume,.6);}});
test('a delayed seek cannot affect the next song or its loading media',async()=>{const e=environment();e.player.getVideoData=()=>({video_id:'xsdTzjogYBM',title:'Next song'});await e.window.__lazytune.request(1,'command',{type:'seek',value:{position:0,track_id:'dQw4w9WgXcQ'}});assert.equal(e.video.currentTime,5);assert.match(e.reports.at(-1).error,/track changed/);e.window.__lazytune.report(true);assert.equal(e.reports.at(-1).snapshot.position,0);assert.equal(e.reports.at(-1).snapshot.seekable,false);e.player.getPlayerResponse=()=>({videoDetails:{videoId:'xsdTzjogYBM'}});e.video.readyState=0;await e.window.__lazytune.request(2,'command',{type:'seek',value:{position:0,track_id:'xsdTzjogYBM'}});assert.equal(e.video.currentTime,5);assert.match(e.reports.at(-1).error,/still loading/);});
test('follow and unfollow use the artist subscription channel',async()=>{const e=environment();await e.window.__lazytune.request(1,'command',{type:'follow',value:{channel_id:'UCofficial',follow:true}});assert.equal(e.calls[0].endpoint,'/subscription/subscribe');assert.equal(e.calls[0].body.channelIds[0],'UCofficial');await e.window.__lazytune.request(2,'command',{type:'follow',value:{channel_id:'UCofficial',follow:false}});assert.equal(e.calls[1].endpoint,'/subscription/unsubscribe');});

test('hydrated player-bar navigation works without its old DOM buttons',async()=>{
  const e=environment();delete e.nodes['.next-button.ytmusic-player-bar'];
  const pending=e.window.__lazytune.request(1,'command',{type:'next'});await e.elapse(100);
  e.nodes['ytmusic-player-bar']={onNextButtonClick(){assert.equal(this,e.nodes['ytmusic-player-bar']);e.calls.push('native next');}};
  await e.elapse(100);await pending;assert.ok(e.calls.includes('native next'));assert.equal(e.calls.includes('play'),false);
});
test('hydration expires with one actionable error and clears play intent',async()=>{
  const e=environment();delete e.nodes['ytmusic-app'].handleNavigationEndpoint;
  const pending=e.window.__lazytune.request(1,'command',{type:'play',value:'xsdTzjogYBM'});await e.elapse(10000);await pending;
  assert.match(e.reports.at(-1).error,/did not become ready/);e.window.__lazytune.report(true);assert.equal(e.reports.at(-1).snapshot.play_requested,false);
});
test('a later track selection supersedes a waiting navigation',async()=>{
  const e=environment(),navigate=e.nodes['ytmusic-app'].handleNavigationEndpoint;delete e.nodes['ytmusic-app'].handleNavigationEndpoint;
  const first=e.window.__lazytune.request(1,'command',{type:'play',value:'xsdTzjogYBM'}),second=e.window.__lazytune.request(2,'command',{type:'play',value:'abcdefghijk'});
  e.nodes['ytmusic-app'].handleNavigationEndpoint=navigate;await e.elapse(100);await Promise.all([first,second]);
  assert.deepEqual(e.calls.filter(c=>c?.watchEndpoint).map(c=>c.watchEndpoint.videoId),['abcdefghijk']);
});
test('changing response objects without changing the track cannot resume the old song',async()=>{
  const e=environment();e.video.paused=true;e.player.getPlayerState=()=>2;await e.window.__lazytune.request(1,'command',{type:'next'});
  e.player.getPlayerResponse=()=>({videoDetails:{videoId:'dQw4w9WgXcQ',lengthSeconds:'180'}});e.advance(1000);e.window.__lazytune.report(true);assert.equal(e.calls.includes('play'),false);
});
test('recovery is capped at three attempts per track and deliberate Play replenishes it',async()=>{
  const e=environment();await e.window.__lazytune.request(1,'command',{type:'play',value:'dQw4w9WgXcQ'});e.window.__lazytune.report(true);
  e.video.paused=true;e.player.getPlayerState=()=>2;e.window.__lazytune.report(true);
  for(let n=0;n<4;n++){e.advance(1000);e.window.__lazytune.report(true);}
  assert.equal(e.calls.filter(c=>c==='play').length,4);assert.equal(e.reports.at(-1).snapshot.play_requested,false);assert.match(e.reports.at(-1).snapshot.error,/Playback stopped/);
  e.advance(5000);e.window.__lazytune.report(true);assert.equal(e.calls.filter(c=>c==='play').length,4);
  await e.window.__lazytune.request(2,'command',{type:'play',value:'dQw4w9WgXcQ'});assert.equal(e.calls.filter(c=>c==='play').length,5);
});
test('native MediaSession Pause and Stop cancel recovery and preserve callback context',async()=>{
  for(const action of ['pause','stop']){
    const e=environment();await e.window.__lazytune.request(1,'command',{type:'play',value:'dQw4w9WgXcQ'});e.window.__lazytune.report(true);const receiver={};
    e.context.navigator.mediaSession.setActionHandler(action,function(value){assert.equal(this,receiver);assert.equal(value,7);e.video.paused=true;e.player.getPlayerState=()=>2;return 'handled';});
    assert.equal(e.handlers[action].call(receiver,7),'handled');e.window.__lazytune.report(true);e.advance(5000);e.window.__lazytune.report(true);
    assert.equal(e.calls.filter(c=>c==='play').length,1);assert.equal(e.reports.at(-1).snapshot.play_requested,false);
    e.context.navigator.mediaSession.setActionHandler(action,null);assert.equal(e.handlers[action],null);
  }
});
test('pending Play can be canceled with Toggle even before media readiness',async()=>{
  const e=environment();e.video.readyState=0;e.video.paused=true;e.player.getPlayerState=()=>5;
  await e.window.__lazytune.request(1,'command',{type:'play',value:'dQw4w9WgXcQ'});await e.window.__lazytune.request(2,'command',{type:'toggle'});
  e.video.readyState=4;e.advance(2000);e.window.__lazytune.report(true);assert.equal(e.calls.filter(c=>c==='play').length,1);assert.equal(e.reports.at(-1).snapshot.play_requested,false);
});
test('recovery never restarts ended, errored, buffering, seeking, ad or login media',async()=>{
  for(const reason of ['ended','error','buffering','seeking','ad','login']){
    const e=environment();await e.window.__lazytune.request(1,'command',{type:'play',value:'dQw4w9WgXcQ'});e.window.__lazytune.report(true);e.video.paused=true;e.player.getPlayerState=()=>2;
    if(reason==='ended'){e.video.ended=true;e.fire('ended');}if(reason==='error')e.video.error={code:3};if(reason==='buffering')e.player.getPlayerState=()=>3;if(reason==='seeking')e.video.seeking=true;if(reason==='ad')e.player.classList.contains=()=>true;if(reason==='login')e.window.__lazytune.mode('login');
    e.window.__lazytune.report(true);e.advance(5000);e.window.__lazytune.report(true);assert.equal(e.calls.filter(c=>c==='play').length,1,reason);
  }
});
test('an ended song remains excluded after its media element is replaced',async()=>{
  const e=environment();await e.window.__lazytune.request(1,'command',{type:'play',value:'dQw4w9WgXcQ'});e.tick();e.window.__lazytune.report(true);e.video.ended=true;e.video.paused=true;e.fire('ended');
  e.nodes['#movie_player video']={...e.video,ended:false,currentTime:0};e.player.getPlayerState=()=>2;e.tick();e.advance(5000);e.window.__lazytune.report(true);assert.equal(e.calls.filter(c=>c==='play').length,1);
});
test('a next song cues without inheriting the ended track or raw media clock',async()=>{
  const e=environment();await e.window.__lazytune.request(1,'command',{type:'play',value:'dQw4w9WgXcQ'});e.window.__lazytune.report(true);e.video.ended=true;e.fire('ended');
  e.video.ended=false;e.video.paused=true;e.video.readyState=0;e.player.getPlayerState=()=>5;e.player.getVideoData=()=>({video_id:'xsdTzjogYBM'});e.player.getPlayerResponse=()=>({videoDetails:{videoId:'xsdTzjogYBM',lengthSeconds:'103'}});e.player.getCurrentTime=()=>0;e.video.currentTime=999;e.window.__lazytune.report(true);
  assert.equal(e.calls.filter(c=>c==='play').length,2);assert.equal(e.reports.at(-1).snapshot.position,0);assert.equal(e.reports.at(-1).snapshot.duration,103);
});
test('a native Repeat One restart restores intent and its pause recovery budget',async()=>{
  const e=environment();await e.window.__lazytune.request(1,'command',{type:'play',value:'dQw4w9WgXcQ'});e.tick();e.window.__lazytune.report(true);
  e.video.currentTime=180;e.video.paused=true;e.video.ended=true;e.player.getPlayerState=()=>0;e.fire('ended');
  e.video.currentTime=0;e.video.paused=false;e.video.ended=false;e.player.getPlayerState=()=>1;e.fire('playing');
  assert.equal(e.reports.at(-1).snapshot.play_requested,true);
  e.video.currentTime=5;e.video.paused=true;e.player.getPlayerState=()=>2;e.fire('pause');e.advance(1000);e.window.__lazytune.report(true);assert.equal(e.calls.filter(c=>c==='play').length,2);
});

function audioEnvironment({capture=true,blocked=false,connectError=false}={}){
  const e=environment(),operations=[];let sources=0,reads=0;const destination={name:'destination'};
  class Context{
    state='suspended';destination=destination;
    async resume(){operations.push('resume');if(!blocked)this.state='running';}
    createAnalyser(){return {name:'analyser',frequencyBinCount:1024,connect(target){operations.push(['analyser',target.name]);},getByteFrequencyData(data){reads++;data.fill(192);}};}
    createMediaStreamSource(){operations.push('capture source');return {connect(){operations.push('capture connect');},disconnect(){operations.push('capture disconnect');}};}
    createMediaElementSource(){sources++;assert.equal(this.state,'running');return {connect(target){operations.push(['element',target.name]);if(connectError&&target.name==='analyser')throw Error('connect failed');}};}
  }
  e.window.AudioContext=Context;e.video.currentSrc='blob:https://music.youtube.com/audio';
  if(capture)e.video.captureStream=()=>{const track={readyState:'live',stop(){this.readyState='ended';operations.push('capture stopped');}};return {getAudioTracks:()=>[track],getTracks:()=>[track]};};
  return {...e,operations,get sources(){return sources;},get reads(){return reads;}};
}
test('real analyser emits 32 audio bands at at most 20 Hz without rerouting capture audio',async()=>{
  const e=audioEnvironment();e.window.__lazytune.configure({visualizer:true});await e.elapse(100);const spectra=e.reports.filter(r=>r.kind==='spectrum');
  assert.equal(spectra.length,1);assert.equal(spectra[0].bands.length,32);assert.ok(spectra[0].bands.every(n=>n===192));assert.equal(e.sources,0);
  e.sample();e.advance(49);e.sample();assert.equal(e.reports.filter(r=>r.kind==='spectrum').length,1);e.advance(1);e.sample();assert.equal(e.reports.filter(r=>r.kind==='spectrum').length,2);
  e.video.paused=true;e.fire('pause');const reads=e.reads;e.advance(100);e.sample();assert.equal(e.reads,reads);assert.ok(e.operations.includes('capture stopped'));
});
test('disabling the visualizer keeps the element pass-through and reuses its single binding',async()=>{
  const e=audioEnvironment({capture:false});e.window.__lazytune.configure({visualizer:true});await e.elapse(100);assert.equal(e.sources,1);
  assert.ok(e.operations.some(v=>Array.isArray(v)&&v[0]==='analyser'&&v[1]==='destination'));
  e.window.__lazytune.configure({visualizer:false});const reads=e.reads;e.advance(100);e.sample();assert.equal(e.reads,reads);assert.equal(e.video.paused,false);
  e.video.currentSrc='blob:https://music.youtube.com/next';e.window.__lazytune.configure({visualizer:true});await e.elapse(100);e.tick();await e.elapse(100);assert.equal(e.sources,1);assert.equal(e.reports.some(r=>r.kind==='visualizer_error'),false);
});
test('blocked Web Audio never binds media and reports one error per enable',async()=>{
  const e=audioEnvironment({capture:false,blocked:true});e.window.__lazytune.configure({visualizer:true});await e.elapse(100);e.tick();await e.elapse(100);
  assert.equal(e.sources,0);assert.equal(e.video.paused,false);assert.equal(e.reports.filter(r=>r.kind==='visualizer_error').length,1);
  e.window.__lazytune.configure({visualizer:false});e.window.__lazytune.configure({visualizer:true});await e.elapse(100);assert.equal(e.reports.filter(r=>r.kind==='visualizer_error').length,2);
});
test('cross-origin non-CORS audio is never rerouted by the visualizer',async()=>{
  const e=audioEnvironment({capture:false});e.video.currentSrc='https://other.test/audio';e.window.__lazytune.configure({visualizer:true});await e.elapse(100);
  assert.equal(e.sources,0);assert.equal(e.video.paused,false);assert.equal(e.reports.filter(r=>r.kind==='visualizer_error').length,1);
});
test('capture-only engines reject unsupported analysis before binding or affecting audio',async()=>{
  const e=audioEnvironment({capture:false});e.window.__lazytune.configure({visualizer:true,capture_only:true});await e.elapse(100);e.tick();await e.elapse(100);
  assert.equal(e.sources,0);assert.equal(e.video.paused,false);assert.equal(e.reports.filter(r=>r.kind==='visualizer_error').length,1);
});
test('a failed analyser connection preserves direct audio output',async()=>{
  const e=audioEnvironment({capture:false,connectError:true});e.window.__lazytune.configure({visualizer:true});await e.elapse(100);assert.equal(e.sources,1);
  assert.ok(e.operations.some(v=>Array.isArray(v)&&v[0]==='element'&&v[1]==='destination'));assert.equal(e.video.paused,false);assert.equal(e.reports.filter(r=>r.kind==='visualizer_error').length,1);
});
test('partial runtime configuration preserves saved ad and video preferences',()=>{
  const e=environment(undefined,{adblock:false,auto_video:true});e.window.__lazytune.configure({visualizer:false});assert.equal(e.toggle.playbackMode,'OMV_PREFERRED');
  const payload=vm.runInContext('JSON.parse(\'{"videoDetails":{},"adPlacements":[1]}\')',e.context);assert.equal(payload.adPlacements.length,1);
});
test('account restore seeks a confirmed song and resumes only requested playback',async()=>{
  for(const playing of [false,true]){
    const e=environment();e.video.paused=true;e.player.getPlayerState=()=>2;
    await e.window.__lazytune.request(1,'restore',{track_id:'dQw4w9WgXcQ',position:59,playing});
    assert.deepEqual(e.calls.find(c=>Array.isArray(c)&&c[0]==='seek'),['seek',59]);assert.equal(e.calls.includes('play'),playing);assert.equal(e.reports.at(-1).kind,'reply');
  }
});
test('paused cold restore defers its seek until a later deliberate Resume',async()=>{
  const e=environment();e.video.readyState=0;e.video.paused=true;e.player.getPlayerState=()=>5;
  await e.window.__lazytune.request(1,'restore',{track_id:'dQw4w9WgXcQ',position:59,playing:false});assert.equal(e.calls.includes('play'),false);
  await e.window.__lazytune.request(2,'command',{type:'resume'});assert.equal(e.calls.filter(c=>c==='play').length,1);
  e.video.readyState=4;e.window.__lazytune.report(true);assert.deepEqual(e.calls.find(c=>Array.isArray(c)&&c[0]==='seek'),['seek',59]);
});
test('transport changes cancel account restore while it waits for hydration',async()=>{
  const e=environment();delete e.nodes['ytmusic-app'].handleNavigationEndpoint;
  const pending=e.window.__lazytune.request(1,'restore',{track_id:'xsdTzjogYBM',position:59,playing:true});await e.elapse(100);
  await e.window.__lazytune.request(2,'command',{type:'pause'});await e.elapse(100);await pending;assert.equal(e.calls.some(c=>c?.watchEndpoint),false);assert.equal(e.calls.includes('play'),false);
});
test('account restore validates track, timing and intent before navigating',async()=>{
  for(const value of [{track_id:'invalid',position:0,playing:true},{track_id:'dQw4w9WgXcQ',position:Infinity,playing:true},{track_id:'dQw4w9WgXcQ',position:0,playing:'true'}]){
    const e=environment();await e.window.__lazytune.request(1,'restore',value);assert.match(e.reports.at(-1).error,/Invalid playback restore/);assert.equal(e.calls.length,0);
  }
});
