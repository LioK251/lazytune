// Only this bundled adapter knows YouTube's private page API. Themes cannot supply scripts.
(() => {
  'use strict';
  if (location.origin !== 'https://music.youtube.com' || window.top !== window) return;
  let config = window.__lazytuneConfig ?? { adblock: true }, mode = 'audio', bound, lastEmit = 0, videoRequested = '';
  let pendingVolume=typeof config.volume==='number'?config.volume:null;
  let requestedPlay=null,desiredPlay=false,generation=0,playError=null,pausedAt=null,recoveryId='',recoveryAttempts=0,lastRecovery=-Infinity,endedId='',seekUntil=0;
  let timeline={id:'',position:0,duration:0},metadata={},pendingRestore=null;
  const diagnostics=[];
  const $ = s => document.querySelector(s);
  const api = () => $('#movie_player');
  const app = () => $('ytmusic-app');
  const queue = () => $('#queue');
  const invoke = (kind, data) => window.__TAURI_INTERNALS__?.invoke('player_report', {report: JSON.stringify({kind, ...data})}).catch(() => {});
  const finite = (n, fallback = 0) => Number.isFinite(n) && n >= 0 ? n : fallback;
  const str = v => String(v ?? '').slice(0, 1000);
  const image = v => (v?.thumbnails ?? []).at(-1)?.url ?? '';
  const track = r => ({ id: str(r?.videoId), title: str(r?.title?.runs?.map(r=>r.text).join('') ?? r?.title?.simpleText), artist: str(r?.longBylineText?.runs?.map(r=>r.text).join('')), artwork: image(r?.thumbnail), album:'',duration:0 });
  function trace(reason) {
    const p=api(),v=$('#movie_player video');
    diagnostics.push({at:Math.round(performance.now()),reason,id:p?.getVideoData?.()?.video_id??'',state:p?.getPlayerState?.(),paused:v?.paused,ended:v?.ended,ready:v?.readyState,desired:desiredPlay});
    if(diagnostics.length>32)diagnostics.shift();
  }
  function stopIntent(error=null) {desiredPlay=false;requestedPlay=null;pendingRestore=null;generation++;pausedAt=null;playError=error;trace(error?'failed':'pause');}
  // Navigation can cue media without buffering it; play intent outlives the first successful start.
  function expectPlay(id='',restart=false,explicit=true) {
    const p=api();
    if(explicit){generation++;recoveryAttempts=0;lastRecovery=-Infinity;endedId='';pendingRestore=pendingRestore&&id===pendingRestore.track_id?{...pendingRestore,generation}:null;}
    desiredPlay=true;playError=null;pausedAt=null;
    return requestedPlay={id,restart,from:p?.getVideoData?.()?.video_id??'',position:finite(p?.getCurrentTime?.()),until:performance.now()+20000,attempt:-Infinity,generation,issued:true,explicit};
  }
  const current = wanted => wanted.generation===generation&&mode!=='login';
  const selected = (p,id) => !requestedPlay||requestedPlay.issued&&(requestedPlay.id?id===requestedPlay.id:id!==requestedPlay.from||requestedPlay.same&&id===requestedPlay.from||requestedPlay.restart&&finite(p?.getCurrentTime?.())<requestedPlay.position-1);
  async function ready(test,message,wanted) {
    const until=performance.now()+10000;
    for(;;){
      if(wanted&&!current(wanted))return null;
      const result=test();if(result)return result;
      if(performance.now()>=until)throw Error(message);
      await new Promise(resolve=>setTimeout(resolve,100));
    }
  }
  // YouTube reuses an offset media timeline; only its song clock and metadata are track-relative.
  const songDuration = (p,id) => timeline.id===id&&timeline.duration>0?timeline.duration:finite(Number(p?.getPlayerResponse?.()?.videoDetails?.lengthSeconds));
  const canSeek = (p,v,id) => !!id&&p?.getVideoData?.()?.video_id===id&&p?.getPlayerResponse?.()?.videoDetails?.videoId===id&&selected(p,id)&&v?.readyState>0&&songDuration(p,id)>0&&!p?.classList.contains('ad-showing');
  function report(force = false) {
    if(pendingVolume!==null) {
      const media=$('#movie_player video'),player=api();
      if(media) {
        media.volume=pendingVolume;
        if(player?.setVolume) {try {player.setVolume(pendingVolume*100);pendingVolume=null;}catch {}}
      }
    }
    const now = performance.now(); if (!force && now - lastEmit < 750) return; lastEmit=now;
    const p=api(),v=$('#movie_player video'),d=p?.getVideoData?.() ?? {}, response=p?.getPlayerResponse?.() ?? {};
    const id=d.video_id||timeline.id,consistent=!!d.video_id&&d.video_id===response.videoDetails?.videoId;
    if(timeline.id!==id){timeline={id,position:0,duration:0};metadata={};}
    if(consistent) {
      timeline.duration=songDuration(p,d.video_id);
      timeline.position=timeline.duration>0?Math.min(timeline.duration,finite(p?.getCurrentTime?.(),timeline.position)):0;
      metadata={title:d.title||metadata.title,artist:d.author||metadata.artist,artist_id:str(response.videoDetails?.channelId)||metadata.artist_id,artwork:image(response.videoDetails?.thumbnail)||metadata.artwork};
    }
    if(pendingRestore&&current(pendingRestore)&&consistent&&id===pendingRestore.track_id){
      if(!desiredPlay&&v&&!v.paused)p?.pauseVideo?.();
      if(canSeek(p,v,id)&&p?.seekTo){const restored=Math.min(pendingRestore.position,songDuration(p,id));pendingRestore=null;seekUntil=now+1000;try{p.seekTo(restored,true);timeline.position=restored;}catch{stopIntent('Could not restore this track. Try playing it again.');}}
    }
    const {position,duration}=timeline;
    const state=p?.getPlayerState?.(),ad=p?.classList.contains('ad-showing')??false,playing=!!v&&!v.paused&&!v.ended;
    if(endedId===id&&consistent&&state===1&&playing&&duration>0&&position<duration-1){endedId='';recoveryAttempts=0;lastRecovery=-Infinity;}
    if(v?.ended||state===0&&duration>0&&position>=duration-1){endedId=id;pausedAt=null;if(!requestedPlay?.explicit)requestedPlay=null;}
    if(v?.error&&desiredPlay)stopIntent('The media engine could not play this track.');
    if(consistent&&recoveryId!==id){
      recoveryId=id;recoveryAttempts=0;lastRecovery=-Infinity;pausedAt=null;
      if(desiredPlay&&!requestedPlay&&id!==endedId)expectPlay(id,false,false);
    }
    if(requestedPlay&&now>=requestedPlay.until)stopIntent('This track could not start. Try playing it again.');
    if(requestedPlay&&consistent&&selected(p,id)&&p?.playVideo&&mode!=='login'&&!ad&&!v?.error&&state!==3&&(requestedPlay.explicit||!v?.ended&&id!==endedId)) {
      if(Number.isFinite(requestedPlay.attempt)&&state===1&&playing){requestedPlay=null;pausedAt=null;}
      else if(now-requestedPlay.attempt>=500){
        requestedPlay.attempt=now;
        try{p.playVideo();}catch{stopIntent('This track could not start. Try playing it again.');}
      }
    }
    const recover=desiredPlay&&!requestedPlay&&consistent&&v?.paused&&!v.ended&&!v.error&&!v.seeking&&now>=seekUntil&&mode!=='login'&&!ad&&state!==0&&state!==3&&(v.readyState>=2||v.readyState===0&&(state===-1||state===5))&&duration>0&&position<duration-1&&id!==endedId&&(!response.playabilityStatus?.status||response.playabilityStatus.status==='OK');
    if(recover){
      pausedAt??=now;
      if(now-pausedAt>=1000&&now-lastRecovery>=1000){
        if(recoveryAttempts>=3)stopIntent('Playback stopped. Try playing this track again.');
        else{recoveryAttempts++;lastRecovery=now;trace('recover');try{p?.playVideo?.();}catch{stopIntent('Playback stopped. Try playing this track again.');}}
      }
    }else pausedAt=null;
    const seekable=duration>0&&canSeek(p,v,d.video_id);
    let items=[];try { items=queue()?.queue?.getItems?.() ?? []; } catch {}
    const tracks=items.slice(0,500).map(i=>track(i.playlistPanelVideoRenderer ?? i.playlistPanelVideoWrapperRenderer?.primaryRenderer?.playlistPanelVideoRenderer)).filter(t=>/^[\w-]{11}$/.test(t.id));
    const snapshot={track:{id:str(id),title:str(d.title||metadata.title),artist:str(d.author||metadata.artist),artist_id:metadata.artist_id??'',album:'',artwork:metadata.artwork??'',duration},playing:!!v&&!v.paused&&!v.ended,play_requested:desiredPlay&&id!==endedId,position,duration,seekable,volume:pendingVolume??Math.min(1,finite(p?.getVolume?.()/100,finite(v?.volume,1))),muted:!!v?.muted,liked:$('#like-button-renderer')?.getAttribute('like-status')==='LIKE',ad,ready:!!app()?.networkManager,signed_in:!!window.ytcfg?.get?.('LOGGED_IN'),queue:tracks,error:playError};
    invoke('snapshot',{snapshot});
  }
  const must = (v,message) => { if (!v) throw Error(message); return v; };
  async function command(c) {
    const p=api(),bar=$('ytmusic-player-bar'),q=queue();
    switch(c.type) {
      case 'play': {
        const wanted=expectPlay(c.value);
        if(p?.getVideoData?.()?.video_id!==c.value||p?.getPlayerResponse?.()?.videoDetails?.videoId!==c.value) {
          wanted.issued=false;
          try {const navigate=await ready(()=>app()?.handleNavigationEndpoint,'YouTube Music did not become ready. Try again.',wanted);if(!navigate)break;wanted.issued=true;navigate.call(app(),{watchEndpoint:{videoId:c.value}});}catch(e){if(current(wanted))stopIntent();throw e;}
        }
        break;
      }
      case 'play_collection': {
        const wanted=expectPlay('',true);wanted.issued=false;
        try {
          let playlist=c.value.slice(2);
          if(c.value.startsWith('MPRE')) {
            const network=await ready(()=>app()?.networkManager,'YouTube Music did not become ready. Try again.',wanted);if(!network)break;
            const response=await network.fetch('/browse',{browseId:c.value});
            const url=new URL(response?.microformat?.microformatDataRenderer?.urlCanonical??'',location.origin);
            playlist=url.origin===location.origin&&url.pathname==='/playlist'?url.searchParams.get('list'):null;
          }
          if(!/^[\w-]{1,256}$/.test(playlist??''))throw Error('This collection is unavailable');
          if(!current(wanted))break;
          const navigate=await ready(()=>app()?.handleNavigationEndpoint,'YouTube Music did not become ready. Try again.',wanted);if(!navigate)break;
          wanted.issued=true;navigate.call(app(),{watchPlaylistEndpoint:{playlistId:playlist}});
        }catch(e){if(current(wanted))stopIntent();throw e;}
        break;
      }
      case 'toggle':
        if(desiredPlay||p?.getPlayerState?.()===1){stopIntent();p?.pauseVideo?.();}else expectPlay(p?.getVideoData?.()?.video_id??'');break;
      case 'pause': stopIntent();p?.pauseVideo?.();break;
      case 'resume': if(!requestedPlay)expectPlay(p?.getVideoData?.()?.video_id??'');break;
      case 'next':
      case 'previous': {
        const wanted=expectPlay('',c.type==='previous');wanted.issued=false;
        try{
          const action=await ready(()=>{const bar=$('ytmusic-player-bar'),method=c.type==='next'?'onNextButtonClick':'onPreviousButtonClick';if(typeof bar?.[method]==='function')return ()=>bar[method]();const button=$(`.${c.type}-button.ytmusic-player-bar`);return button?()=>button.click():null;},'Track navigation is unavailable',wanted);
          if(!action)break;wanted.issued=true;action();
        }catch(e){if(current(wanted))stopIntent();throw e;}break;
      }
      case 'seek': {
        const media=must($('#movie_player video'),'Player is not ready');
        const {position,track_id}=c.value;
        if(!canSeek(p,media,track_id))throw Error('The track changed or is still loading. Try seeking again.');
        seekUntil=performance.now()+1000;pausedAt=null;
        must(p?.seekTo,'Seeking is unavailable').call(p,Math.min(position,songDuration(p,track_id)),true);break;
      }
      case 'follow': {
        must(window.ytcfg?.get?.('LOGGED_IN'),'Sign in to follow artists');
        const response=await must(app()?.networkManager,'YouTube Music is still starting').fetch(c.value.follow?'/subscription/subscribe':'/subscription/unsubscribe',{channelIds:[c.value.channel_id]});
        if(response?.error)throw Error(response.error.message??'Could not update artist following');break;
      }
      case 'volume': pendingVolume=c.value;break;
      case 'mute': p?.isMuted()?p.unMute():p?.mute();break;
      case 'like': must($('#like-button-renderer')?.updateLikeStatus,'Sign in to like tracks').call($('#like-button-renderer'),c.value?'LIKE':'INDIFFERENT');break;
      case 'shuffle': must(bar?.queue,'Queue is unavailable').shuffle();break;
      case 'repeat': must(bar?.onRepeatButtonClick,'Repeat is unavailable').call(bar);break;
      case 'remove': must(q?.dispatch,'Queue is unavailable').call(q,{type:'REMOVE_ITEM',payload:c.value});break;
      case 'move': must(q?.dispatch,'Queue is unavailable').call(q,{type:'MOVE_ITEM',payload:{fromIndex:c.value.from,toIndex:c.value.to}});break;
      case 'enqueue': {
        const store=must(q?.queue?.store?.store,'Start a track before adding to its queue'),state=store.getState().queue;
        const result=await app().networkManager.fetch('/music/get_queue',{queueContextParams:state.queueContextParams,queueInsertPosition:'INSERT_AT_END',videoIds:[c.value]});
        const items=result?.queueDatas?.map(i=>i.content).filter(Boolean);if(!items?.length)throw Error('Could not add the track');
        q.dispatch({type:'ADD_ITEMS',payload:{nextQueueItemId:state.nextQueueItemId,index:state.items.length,items,shuffleEnabled:false,shouldAssignIds:true}});break;
      }
      default: throw Error('Unsupported player command');
    }
    report(true);
  }
  async function catalog(request) {
    const network=await ready(()=>app()?.networkManager,'YouTube Music did not become ready. Try again.');
    let endpoint='/browse',body={};
    switch(request.type) {
      case 'home':body={browseId:'FEmusic_home'};break;
      case 'library':body={browseId:'FEmusic_liked_playlists'};break;
      case 'liked':body={browseId:'VLLM'};break;
      case 'browse':body={browseId:request.value};break;
      case 'search':endpoint='/search';body={query:request.value};break;
      case 'continue':endpoint=request.value.search?'/search':'/browse';body={continuation:request.value.token};break;
      default:throw Error('Unsupported catalog request');
    }
    return await network.fetch(endpoint,body);
  }
  async function restore(value){
    if(!value||!/^[\w-]{11}$/.test(value.track_id)||!Number.isFinite(value.position)||value.position<0||value.position>604800||typeof value.playing!=='boolean')throw Error('Invalid playback restore');
    stopIntent();endedId='';if(!value.playing)api()?.pauseVideo?.();
    const wanted=value.playing?expectPlay(value.track_id):{generation};
    pendingRestore={track_id:value.track_id,position:value.position,generation};
    try{
      if(api()?.getVideoData?.()?.video_id!==value.track_id||api()?.getPlayerResponse?.()?.videoDetails?.videoId!==value.track_id){
        if(value.playing)wanted.issued=false;
        const navigate=await ready(()=>app()?.handleNavigationEndpoint,'Could not restore this track. Try playing it again.',wanted);if(!navigate)return;
        if(value.playing)wanted.issued=true;
        navigate.call(app(),{watchEndpoint:{videoId:value.track_id}});
      }
      const loaded=await ready(()=>api()?.getVideoData?.()?.video_id===value.track_id&&api()?.getPlayerResponse?.()?.videoDetails?.videoId===value.track_id,'Could not restore this track. Try playing it again.',wanted);
      if(loaded)report(true);
    }catch(e){if(current(wanted))stopIntent();throw e;}
  }
  const session=globalThis.navigator?.mediaSession;
  if(session?.setActionHandler){
    const setAction=session.setActionHandler;
    session.setActionHandler=function(action,handler){
      const wrapped=typeof handler==='function'?function(...args){if(action==='pause'||action==='stop')stopIntent();else if(action==='play')expectPlay(api()?.getVideoData?.()?.video_id??'');return handler.apply(this,args);}:handler;
      return setAction.call(this,action,wrapped);
    };
  }
  let audioContext=null,activeAudio=null,sampling=null,analyserStarting=false,visualizerFailed=false,lastSpectrum=-Infinity;
  const audioNodes=new WeakMap();
  function stopSampling(){if(sampling!==null){clearInterval(sampling);sampling=null;}}
  function releaseCapture(){if(activeAudio&&!activeAudio.node.routed){activeAudio.node.source.disconnect();for(const t of activeAudio.node.stream.getTracks())t.stop();audioNodes.delete(activeAudio.v);activeAudio=null;}}
  function visualizerError(){stopSampling();releaseCapture();if(!visualizerFailed){visualizerFailed=true;invoke('visualizer_error',{error:'Audio visualizer is unavailable in this browser.'});}}
  function sampleAudio(v,node){
    if(!config.visualizer||mode==='login'||$('#movie_player video')!==v||v.paused||v.ended||v.error||api()?.classList.contains('ad-showing')){stopSampling();return;}
    const now=performance.now();if(now-lastSpectrum<50)return;lastSpectrum=now;
    try{
      node.analyser.getByteFrequencyData(node.data);
      const bands=[];
      for(let i=0;i<32;i++){
        const from=Math.floor(Math.expm1(i/32*Math.log(node.data.length+1))),to=Math.min(node.data.length,Math.max(from+1,Math.floor(Math.expm1((i+1)/32*Math.log(node.data.length+1)))));
        let sum=0;for(let j=from;j<to;j++)sum+=node.data[j];bands.push(Math.round(sum/(to-from)));
      }
      invoke('spectrum',{bands});
    }catch{visualizerError();}
  }
  async function analyse(v){
    analyserStarting=true;
    try{
      const Context=window.AudioContext??window.webkitAudioContext;
      if(!Context)throw Error('Web Audio unavailable');
      audioContext??=new Context();
      if(config.capture_only&&(typeof v.captureStream!=='function'||typeof audioContext.createMediaStreamSource!=='function'))throw Error('Audio capture unavailable');
      await Promise.race([audioContext.resume(),new Promise((_,reject)=>setTimeout(()=>reject(Error('Audio context blocked')),2000))]);
      if(audioContext.state!=='running')throw Error('Audio context blocked');
      if(!config.visualizer||mode==='login'||$('#movie_player video')!==v||v.paused||v.ended)return;
      let node=audioNodes.get(v);
      const key=v.currentSrc||v.src||'';
      if(node&&!node.routed&&(node.key!==key||!node.stream.getAudioTracks().some(t=>t.readyState==='live'))){node.source.disconnect();for(const t of node.stream.getTracks())t.stop();audioNodes.delete(v);node=null;}
      if(!node){
        const analyser=audioContext.createAnalyser();analyser.fftSize=2048;analyser.smoothingTimeConstant=.8;
        node={analyser,data:new Uint8Array(analyser.frequencyBinCount),key,routed:false};
        if(typeof v.captureStream==='function'&&audioContext.createMediaStreamSource){
          node.stream=v.captureStream();if(!node.stream.getAudioTracks().length)throw Error('No audio track');
          node.source=audioContext.createMediaStreamSource(node.stream);node.source.connect(analyser);
        }else{
          // Binding a non-CORS media URL silences it; only route media whose origin is accessible.
          if(!key||new URL(key,location.origin).origin!==location.origin&&!v.crossOrigin)throw Error('Media is not accessible');
          analyser.connect(audioContext.destination);
          node.source=audioContext.createMediaElementSource(v);node.routed=true;
          audioNodes.set(v,node);
          activeAudio={v,node};
          try{node.source.connect(analyser);}catch(e){node.source.connect(audioContext.destination);throw e;}
        }
        audioNodes.set(v,node);
      }
      node.key=key;
      activeAudio={v,node};
      stopSampling();sampling=setInterval(()=>sampleAudio(v,node),50);sampleAudio(v,node);
    }catch{if(config.visualizer)visualizerError();}
    finally{analyserStarting=false;}
  }
  function syncAnalyser(){
    const v=$('#movie_player video');
    // A routed element must retain its pass-through even when the effect is disabled.
    if(audioContext?.state==='suspended'&&activeAudio?.node.routed&&!activeAudio.v.paused)audioContext.resume().catch(()=>{});
    if(!config.visualizer||mode==='login'||!v||v.paused||v.ended||v.readyState<2||v.error||api()?.classList.contains('ad-showing')){stopSampling();releaseCapture();return;}
    if(activeAudio&&activeAudio.v!==v&&!activeAudio.node.routed)releaseCapture();
    if(visualizerFailed||analyserStarting)return;
    if(activeAudio?.v===v&&activeAudio.node.key===(v.currentSrc||v.src||'')&&audioContext?.state==='running'&&(activeAudio.node.routed||activeAudio.node.stream.getAudioTracks().some(t=>t.readyState==='live'))){
      if(sampling===null){sampling=setInterval(()=>sampleAudio(v,activeAudio.node),50);sampleAudio(v,activeAudio.node);}
    }else void analyse(v);
  }
  window.__lazytune = {
    async request(id,kind,value) {try {const result=kind==='restore'?await restore(value):kind==='catalog'?await catalog(value):await command(value);invoke('reply',{id,result:result??null});}catch(e){invoke('reply',{id,error:str(e.message)});}},
    configure(value) {const wasEnabled=!!config.visualizer,wasVideo=config.auto_video;config={...config,...value};if(typeof value.volume==='number'&&Number.isFinite(value.volume)&&value.volume>=0&&value.volume<=1)pendingVolume=value.volume;if(!wasEnabled&&config.visualizer)visualizerFailed=false;if(wasVideo!==config.auto_video)videoRequested='';attach();},
    mode(value) {mode=value;if(value==='login')stopIntent();if(value==='audio')videoRequested='';document.documentElement.dataset.lazytuneMode=value;attach();},
    report,diagnostics:()=>diagnostics.slice()
  };
  // Adapt only known player response fields; never install downloaded scriptlets.
  function prune(value) {
    if (!config.adblock || !value || typeof value!=='object') return value;
    if ('playabilityStatus' in value || 'videoDetails' in value) for(const key of ['adPlacements','playerAds','adSlots','adBreakHeartbeatParams']) delete value[key];
    if(value.playerResponse && typeof value.playerResponse==='object')prune(value.playerResponse);
    return value;
  }
  const parse=JSON.parse;JSON.parse=function(...args){return prune(parse.apply(this,args));};
  const json=Response.prototype.json;Response.prototype.json=async function(){return prune(await json.call(this));};
  function attach() {
    if (!document.documentElement) return;
    document.documentElement.dataset.lazytuneMode=mode;
    if(!$('#lazytune-video-style') && document.head) {
      const style=document.createElement('style');style.id='lazytune-video-style';
      style.textContent=`
        html[data-lazytune-mode="video"],html[data-lazytune-mode="video"] body{margin:0!important;padding:0!important;overflow:hidden!important;background:#111!important}
        html[data-lazytune-mode="video"] body *{visibility:hidden!important}
        html[data-lazytune-mode="video"] :is(ytmusic-app,ytmusic-app-layout,#player-page,#player-page .content,#main-panel,ytmusic-player,#song-video,.player-wrapper){display:block!important;position:static!important;transform:none!important;will-change:auto!important;contain:none!important;width:100%!important;height:100%!important;margin:0!important;padding:0!important;max-width:none!important;max-height:none!important}
        html[data-lazytune-mode="video"] #movie_player{display:block!important;position:fixed!important;inset:0!important;transform:none!important;width:100vw!important;height:100vh!important;visibility:visible!important;background:#111!important;z-index:9999}
        html[data-lazytune-mode="video"] #movie_player .html5-video-container,html[data-lazytune-mode="video"] #movie_player video{display:block!important;visibility:visible!important;position:absolute!important;inset:0!important;width:100%!important;height:100%!important;object-fit:cover!important;object-position:center!important;transform:none!important}
        html[data-lazytune-mode="video"] #movie_player .ytp-caption-window-container,html[data-lazytune-mode="video"] #movie_player .ytp-caption-window-container *{visibility:visible!important}
        #lazytune-audio-cover{display:none}
        html[data-lazytune-mode="video"][data-lazytune-media="audio"] #lazytune-audio-cover{display:flex!important;position:fixed!important;inset:0!important;z-index:10000;align-items:center;justify-content:center;flex-direction:column;background:#171717;gap:20px;color:#aaa;font:14px system-ui;visibility:visible!important}
        html[data-lazytune-mode="video"] #lazytune-audio-cover *{visibility:visible!important}
        #lazytune-audio-cover img{max-height:70vh;max-width:70vw;object-fit:contain;border-radius:6px}
      `;
      document.head.append(style);
    }
    const v=$('#movie_player video');if(v && bound!==v){bound=v;for(const e of ['playing','pause','seeked','loadedmetadata','canplay','volumechange','ended','error'])v.addEventListener(e,()=>{if($('#movie_player video')!==v)return;trace(e);if(e==='ended'){endedId=timeline.id;requestedPlay=null;pausedAt=null;}report(true);syncAnalyser();});v.addEventListener('timeupdate',()=>report());}
    const p=api();if(config.adblock&&p?.classList.contains('ad-showing')) {const button=p.querySelector('.ytp-ad-skip-button,.ytp-ad-skip-button-modern,.ytp-skip-ad-button');if(button)button.click();else if(v&&Number.isFinite(v.duration)&&v.duration>0)v.currentTime=v.duration;}
    const toggle=$('ytmusic-av-toggle');
    const switchMode=method=>{
      if(mode==='login'||requestedPlay&&!selected(p,p?.getVideoData?.()?.video_id))return;
      if(desiredPlay||v&&!v.paused&&!v.ended){const wanted=expectPlay('',false,false);wanted.same=true;wanted.attempt=performance.now();}
      toggle[method]?.();
    };
    const wantVideo=mode==='video'||!!config.auto_video;
    if(!wantVideo&&toggle?.audioOnlyPlaybackAvailable&&!toggle.toggleDisabled&&toggle.playbackMode==='OMV_PREFERRED')switchMode('onSongAvToggleTap');
    if(wantVideo) {
      const id=p?.getVideoData?.()?.video_id??'';
      if(id&&videoRequested!==id&&toggle?.selectedItemHasVideo&&!toggle.toggleDisabled&&toggle.playbackMode!=='OMV_PREFERRED') {videoRequested=id;switchMode('onVideoAvToggleTap');}
    }
    if(mode==='video') {
      document.documentElement.dataset.lazytuneMedia=v?.videoWidth>0?'video':'audio';
      let fallback=$('#lazytune-audio-cover');
      if(!fallback&&document.body){fallback=document.createElement('div');fallback.id='lazytune-audio-cover';fallback.append(document.createElement('img'),document.createElement('span'));document.body.append(fallback);}
      if(fallback){const url=image(p?.getPlayerResponse?.()?.videoDetails?.thumbnail);const img=fallback.querySelector('img');if(img.getAttribute('src')!==url)img.src=url;img.alt='Album artwork';fallback.querySelector('span').textContent=toggle?.selectedItemHasVideo?'':'This track has no video.';}
    }
    report();
    syncAnalyser();
  }
  // This low-frequency watchdog also recovers after YouTube replaces its player node.
  setInterval(attach,1000);
  document.addEventListener('DOMContentLoaded',attach,{once:true});
  document.addEventListener('yt-navigate-finish',()=>report(true));
})();
