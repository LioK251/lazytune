const {test}=require('node:test');
const assert=require('node:assert/strict');
const vm=require('node:vm');
const fs=require('node:fs');
const source=fs.readFileSync(require('node:path').join(__dirname,'../desktop/player.js'),'utf8');
function environment(origin='https://music.youtube.com',config){
  const reports=[],calls=[];const video={readyState:4,duration:180,currentTime:5,paused:false,ended:false,volume:.5,muted:false,addEventListener(){}};
  const initialResponse={videoDetails:{videoId:'dQw4w9WgXcQ',lengthSeconds:'180',thumbnail:{thumbnails:[]}}};
  const player={getVideoData:()=>({video_id:'dQw4w9WgXcQ',title:'A song',author:'Artist'}),getPlayerResponse:()=>initialResponse,getCurrentTime:()=>video.currentTime,getDuration:()=>video.duration,classList:{contains:()=>false},getPlayerState:()=>1,pauseVideo:()=>calls.push('pause'),playVideo:()=>calls.push('play'),seekTo:n=>{video.currentTime=n;calls.push(['seek',n]);},setVolume:n=>{video.volume=n/100;calls.push(['volume',n]);}};
  const app={handleNavigationEndpoint:e=>calls.push(e),networkManager:{fetch:async(endpoint,body)=>{calls.push({endpoint,body});return {contents:[],microformat:{microformatDataRenderer:{urlCanonical:'https://music.youtube.com/playlist?list=OLAKalbum'}}};}}};
  const toggle={audioOnlyPlaybackAvailable:true,selectedItemHasVideo:true,toggleDisabled:false,playbackMode:'ATV_PREFERRED',onSongAvToggleTap(){calls.push('audio');this.playbackMode='ATV_PREFERRED';},onVideoAvToggleTap(){calls.push('video');this.playbackMode='OMV_PREFERRED';}};
  const document={querySelector:s=>({'#movie_player':player,'video':video,'#movie_player video':video,'ytmusic-app':app,'ytmusic-av-toggle':toggle,'#lazytune-video-style':{},'.next-button.ytmusic-player-bar':{click:()=>calls.push('next')},'.previous-button.ytmusic-player-bar':{click:()=>calls.push('previous')}}[s]??null),documentElement:{dataset:{}},head:{append(){}},addEventListener(){}};
  const window={ytcfg:{get:()=>true},__lazytuneConfig:config,__TAURI_INTERNALS__:{invoke:async(_,v)=>reports.push(JSON.parse(v.report))}};window.top=window;
  let tick,now=1000;
  const context=vm.createContext({window,document,URL,location:{origin},performance:{now:()=>now},setInterval(fn){tick=fn;},Response:class{async json(){return {playabilityStatus:{status:'OK'},adPlacements:[1]}}}});
  vm.runInContext(source,context);return {context,window,video,player,reports,calls,toggle,tick,advance(ms){now+=ms;}};
}
test('bridge is not installed on an unrelated origin',()=>assert.equal(environment('https://evil.test').window.__lazytune,undefined));
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
  await e.window.__lazytune.request(2,'command',{type:'pause'});
  e.video.readyState=4;e.window.__lazytune.report(true);
  assert.equal(e.calls.includes('play'),false);
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
  const response={videoDetails:{videoId:'abcdefghijk',lengthSeconds:'103'}};
  e.player.getVideoData=()=>({video_id:'abcdefghijk'});e.player.getPlayerResponse=()=>response;
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
  e.advance(20001);e.video.readyState=4;e.window.__lazytune.report(true);
  assert.equal(e.calls.filter(c=>c==='play').length,2);
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
