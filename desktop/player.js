// Only this bundled adapter knows YouTube's private page API. Themes cannot supply scripts.
(() => {
  'use strict';
  if (location.origin !== 'https://music.youtube.com' || window.top !== window) return;
  let config = window.__lazytuneConfig ?? { adblock: true }, mode = 'audio', bound, lastEmit = 0, videoRequested = '';
  let pendingVolume=typeof config.volume==='number'?config.volume:null;
  let requestedPlay=null;
  let timeline={id:'',position:0,duration:0};
  const $ = s => document.querySelector(s);
  const api = () => $('#movie_player');
  const app = () => $('ytmusic-app');
  const queue = () => $('#queue');
  const invoke = (kind, data) => window.__TAURI_INTERNALS__?.invoke('player_report', {report: JSON.stringify({kind, ...data})}).catch(() => {});
  const finite = (n, fallback = 0) => Number.isFinite(n) && n >= 0 ? n : fallback;
  const str = v => String(v ?? '').slice(0, 1000);
  const image = v => (v?.thumbnails ?? []).at(-1)?.url ?? '';
  const track = r => ({ id: str(r?.videoId), title: str(r?.title?.runs?.map(r=>r.text).join('') ?? r?.title?.simpleText), artist: str(r?.longBylineText?.runs?.map(r=>r.text).join('')), artwork: image(r?.thumbnail), album:'',duration:0 });
  // Navigation can only cue a song; keep the user's play intent until its media starts.
  function expectPlay(id='',restart=false) {
    const p=api();
    return requestedPlay={id,restart,response:p?.getPlayerResponse?.(),position:finite(p?.getCurrentTime?.()),until:performance.now()+20000,attempt:-Infinity};
  }
  const selected = (p,id,response=p?.getPlayerResponse?.()) => !requestedPlay||(requestedPlay.id?id===requestedPlay.id:response!==requestedPlay.response||requestedPlay.restart&&finite(p?.getCurrentTime?.())<requestedPlay.position-1);
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
    const consistent=!!d.video_id&&d.video_id===response.videoDetails?.videoId;
    if(requestedPlay&&now>=requestedPlay.until)requestedPlay=null;
    if(requestedPlay&&consistent&&selected(p,d.video_id,response)&&v?.readyState>0&&p?.playVideo) {
      if(Number.isFinite(requestedPlay.attempt)&&p.getPlayerState?.()===1&&!v.paused&&!v.ended)requestedPlay=null;
      else if(now-requestedPlay.attempt>=500) {
        requestedPlay.attempt=now;
        p.playVideo();
      }
    }
    if(timeline.id!==(d.video_id??''))timeline={id:d.video_id??'',position:0,duration:0};
    if(consistent) {
      timeline.duration=songDuration(p,d.video_id);
      timeline.position=timeline.duration>0?Math.min(timeline.duration,finite(p?.getCurrentTime?.(),timeline.position)):0;
    }
    const {position,duration}=timeline;
    const seekable=duration>0&&canSeek(p,v,d.video_id);
    let items=[];try { items=queue()?.queue?.getItems?.() ?? []; } catch {}
    const tracks=items.slice(0,500).map(i=>track(i.playlistPanelVideoRenderer ?? i.playlistPanelVideoWrapperRenderer?.primaryRenderer?.playlistPanelVideoRenderer)).filter(t=>/^[\w-]{11}$/.test(t.id));
    const snapshot={track:{id:str(d.video_id),title:str(d.title),artist:str(d.author),artist_id:str(response.videoDetails?.channelId),album:'',artwork:image(response.videoDetails?.thumbnail),duration},playing:consistent&&!!v&&!v.paused&&!v.ended,position,duration,seekable,volume:pendingVolume??finite(v?.volume,1),muted:!!v?.muted,liked:$('#like-button-renderer')?.getAttribute('like-status')==='LIKE',ad:p?.classList.contains('ad-showing')??false,ready:!!app()?.networkManager,signed_in:!!window.ytcfg?.get?.('LOGGED_IN'),queue:tracks,error:v?.error?'The media engine could not play this track.':null};
    invoke('snapshot',{snapshot});
  }
  const must = (v,message) => { if (!v) throw Error(message); return v; };
  async function command(c) {
    const p=api(),bar=$('ytmusic-player-bar'),q=queue();
    switch(c.type) {
      case 'play': {
        const navigate=must(app()?.handleNavigationEndpoint,'YouTube Music is still starting');
        expectPlay(c.value);
        if(p?.getVideoData?.()?.video_id!==c.value||p?.getPlayerResponse?.()?.videoDetails?.videoId!==c.value) {
          try {navigate.call(app(),{watchEndpoint:{videoId:c.value}});}catch(e){requestedPlay=null;throw e;}
        }
        break;
      }
      case 'play_collection': {
        const wanted=expectPlay();
        try {
          let playlist=c.value.slice(2);
          if(c.value.startsWith('MPRE')) {
            const response=await must(app()?.networkManager,'YouTube Music is still starting').fetch('/browse',{browseId:c.value});
            const url=new URL(response?.microformat?.microformatDataRenderer?.urlCanonical??'',location.origin);
            playlist=url.origin===location.origin&&url.pathname==='/playlist'?url.searchParams.get('list'):null;
          }
          if(!/^[\w-]{1,256}$/.test(playlist??''))throw Error('This collection is unavailable');
          if(requestedPlay!==wanted)break;
          must(app()?.handleNavigationEndpoint,'YouTube Music is still starting').call(app(),{watchPlaylistEndpoint:{playlistId:playlist}});
        }catch(e){if(requestedPlay===wanted)requestedPlay=null;throw e;}
        break;
      }
      case 'toggle':
        must(p,'Player is not ready');
        if(p.getPlayerState()===1){requestedPlay=null;p.pauseVideo();}else if(!requestedPlay)expectPlay(p.getVideoData?.()?.video_id??'');break;
      case 'pause': requestedPlay=null;p?.pauseVideo();break;
      case 'resume': if(!requestedPlay)expectPlay(p?.getVideoData?.()?.video_id??'');break;
      case 'next':
      case 'previous': {
        const button=must($(`.${c.type}-button.ytmusic-player-bar`),'Track navigation is unavailable');
        expectPlay('',c.type==='previous');button.click();break;
      }
      case 'seek': {
        const media=must($('#movie_player video'),'Player is not ready');
        const {position,track_id}=c.value;
        if(!canSeek(p,media,track_id))throw Error('The track changed or is still loading. Try seeking again.');
        must(p?.seekTo,'Seeking is unavailable').call(p,Math.min(position,songDuration(p,track_id)),true);break;
      }
      case 'follow': {
        must(window.ytcfg?.get?.('LOGGED_IN'),'Sign in to follow artists');
        const response=await must(app()?.networkManager,'YouTube Music is still starting').fetch(c.value.follow?'/subscription/subscribe':'/subscription/unsubscribe',{channelIds:[c.value.channel_id]});
        if(response?.error)throw Error(response.error.message??'Could not update artist following');break;
      }
      case 'volume': pendingVolume=null;p?.setVolume(c.value*100);break;
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
    const network=must(app()?.networkManager,'YouTube Music is still loading. Try again shortly.');
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
  window.__lazytune = {
    async request(id,kind,value) {try {const result=kind==='catalog'?await catalog(value):await command(value);invoke('reply',{id,result:result??null});}catch(e){invoke('reply',{id,error:str(e.message)});}},
    configure(value) {config=value;videoRequested='';attach();},
    mode(value) {mode=value;if(value==='audio')videoRequested='';document.documentElement.dataset.lazytuneMode=value;attach();},
    report
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
    const v=$('#movie_player video');if(v && bound!==v){bound=v;for(const e of ['playing','pause','seeked','loadedmetadata','canplay','volumechange','ended','error'])v.addEventListener(e,()=>report(true));v.addEventListener('timeupdate',()=>report());}
    const p=api();if(config.adblock&&p?.classList.contains('ad-showing')) {const button=p.querySelector('.ytp-ad-skip-button,.ytp-ad-skip-button-modern,.ytp-skip-ad-button');if(button)button.click();else if(v&&Number.isFinite(v.duration)&&v.duration>0)v.currentTime=v.duration;}
    const toggle=$('ytmusic-av-toggle');
    const switchMode=method=>{
      if(requestedPlay||v&&!v.paused&&!v.ended)expectPlay();
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
      if(fallback){const url=image(p?.getPlayerResponse?.()?.videoDetails?.thumbnail);const img=fallback.querySelector('img');if(img.getAttribute('src')!==url)img.src=url;img.alt='Album artwork';fallback.querySelector('span').textContent=toggle?.selectedItemHasVideo?'Loading video…':'This track has no video.';}
    }
    report();
  }
  // This low-frequency watchdog also recovers after YouTube replaces its player node.
  setInterval(attach,1000);
  document.addEventListener('DOMContentLoaded',attach,{once:true});
  document.addEventListener('yt-navigate-finish',()=>report(true));
})();
