// Each invocation uses a fresh native profile. Run song, footer, playlist and album separately.
use serde_json::{json, Value};
use std::{
    env, fs,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use tao::{
    event::{Event, WindowEvent},
    event_loop::{ControlFlow, EventLoopBuilder},
    window::WindowBuilder,
};
use wry::{
    dpi::{LogicalPosition, LogicalSize},
    Rect, WebContext, WebViewBuilder,
};
#[cfg(target_os = "macos")]
use wry::{BackgroundThrottlingPolicy, WebViewBuilderExtDarwin};

const BRIDGE: &str = r#"
if(location.origin==='https://music.youtube.com'&&window.top===window){
 window.__nativeTest={snapshot:null,videoCalls:0};
 window.__TAURI_INTERNALS__={invoke:async(_,args)=>{
  const r=JSON.parse(args.report);
  if(r.kind==='snapshot')__nativeTest.snapshot=r.snapshot;
  if(r.kind==='reply'){
   let ids=[];
   if(r.id===9){const walk=(v,d=0)=>{if(!v||typeof v!=='object'||d>24)return;for(const [k,x]of Object.entries(v)){if(k==='browseId'&&typeof x==='string'&&(x.startsWith('VLPL')||x.startsWith('MPRE'))&&!ids.includes(x))ids.push(x);if(typeof x==='object')walk(x,d+1);}};walk(r.result);}
   window.ipc.postMessage(JSON.stringify({event:'reply',id:r.id,failed:!!r.error,ids}));
  }
 }};
 const hooked=new WeakSet();
 setInterval(()=>{
  const p=document.querySelector('#movie_player'),v=p?.querySelector('video'),r=p?.getPlayerResponse?.(),app=document.querySelector('ytmusic-app'),av=document.querySelector('ytmusic-av-toggle');
  if(av&&typeof av.onVideoAvToggleTap==='function'&&!hooked.has(av)){hooked.add(av);const video=av.onVideoAvToggleTap;av.onVideoAvToggleTap=function(...args){__nativeTest.videoCalls++;return video.apply(this,args);};}
  const src=v?.currentSrc??'';let source='empty';try{source=src.startsWith('blob:')?'blob':new URL(src).origin;}catch{}
  window.ipc.postMessage(JSON.stringify({event:'sample',adapter:!!window.__lazytune,ready:!!app?.networkManager&&typeof app?.handleNavigationEndpoint==='function',id:p?.getVideoData?.()?.video_id??'',responseId:r?.videoDetails?.videoId??'',playability:r?.playabilityStatus?.status??'',state:p?.getPlayerState?.()??null,paused:v?.paused??null,mediaReady:v?.readyState??0,time:p?.getCurrentTime?.()??0,bufferedEnd:v?.buffered?.length?v.buffered.end(v.buffered.length-1):0,source,visibility:document.visibilityState,videoCalls:__nativeTest.videoCalls,error:!!__nativeTest.snapshot?.error}));
 },100);
}
"#;

#[derive(Debug)]
enum NativeEvent {
    Message(String),
    PageLoad,
}

fn request(id: u8, kind: &str, value: Value) -> String {
    format!("window.__lazytune?.request({id},{},{value})", json!(kind))
}

fn fail(stage: &str, latest: &Value) -> ! {
    println!("{}", json!({"event":"failed","stage":stage,"last":latest}));
    std::process::exit(2);
}

fn main() {
    #[cfg(target_os = "macos")]
    {
        let version = std::process::Command::new("/usr/bin/sw_vers")
            .arg("-productVersion")
            .output()
            .unwrap();
        let major = String::from_utf8(version.stdout)
            .unwrap()
            .split('.')
            .next()
            .unwrap()
            .parse::<u32>()
            .unwrap();
        assert!(major >= 14, "a unique WK test store requires macOS 14+");
    }
    let scenario = env::args().nth(1).unwrap_or_else(|| "song".into());
    assert!(
        matches!(
            scenario.as_str(),
            "song" | "footer" | "playlist" | "album" | "hidden"
        ),
        "expected song, footer, playlist, album or hidden"
    );
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos()
        ^ u128::from(std::process::id());
    let identifier = nonce.to_le_bytes();
    assert_ne!(identifier, *b"lazytune-player1");
    let hex = identifier
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<String>();
    let profile = format!(
        "{}-{}-{}-{}-{}",
        &hex[..8],
        &hex[8..12],
        &hex[12..16],
        &hex[16..20],
        &hex[20..]
    );
    let directory = env::temp_dir().join(format!("lazytune-native-player-test-{nonce:032x}"));
    fs::create_dir(&directory).unwrap();
    let mut context = WebContext::new(Some(directory));
    let event_loop = EventLoopBuilder::<NativeEvent>::with_user_event().build();
    let proxy = event_loop.create_proxy();
    let load_proxy = proxy.clone();
    let window = WindowBuilder::new()
        .with_title("lazytune isolated native check")
        .with_inner_size(tao::dpi::LogicalSize::new(1280.0, 820.0))
        .build(&event_loop)
        .unwrap();
    let script = format!(
        "window.__lazytuneConfig={{adblock:true,auto_video:false,volume:0.01}};\n{BRIDGE}\n{}",
        include_str!("../desktop/player.js")
    );
    let builder = WebViewBuilder::new_with_web_context(&mut context)
        .with_focused(false)
        .with_bounds(Rect {
            position: LogicalPosition::new(-10000.0, -10000.0).into(),
            size: LogicalSize::new(960.0, 600.0).into(),
        })
        .with_initialization_script(script)
        .with_ipc_handler(move |r| {
            let _ = proxy.send_event(NativeEvent::Message(r.body().clone()));
        })
        .with_on_page_load_handler(move |_, _| {
            let _ = load_proxy.send_event(NativeEvent::PageLoad);
        })
        .with_url("https://music.youtube.com/");
    #[cfg(target_os = "macos")]
    let builder = builder
        // Never use the default WK store or lazytune's installed account profile.
        .with_data_store_identifier(identifier)
        .with_user_agent("Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/18.0 Safari/605.1.15")
        .with_background_throttling(BackgroundThrottlingPolicy::Disabled);
    let webview = builder.build_as_child(&window).unwrap();
    webview
        .set_visible(!matches!(scenario.as_str(), "hidden" | "footer"))
        .unwrap();
    println!(
        "{}",
        json!({"event":"native-config","scenario":scenario,"platform":env::consts::OS,"profile":profile,"bounds":[-10000,-10000,960,600],"focused":false,"autoVideo":false})
    );
    let mut phase = "startup";
    let mut phase_at = Instant::now();
    let mut latest = Value::Null;
    let mut pause_reply = false;
    let mut last_time = 0.0;
    let mut last_log = Instant::now();
    event_loop.run(move |event, _, flow| {
        let _keep_window = &window;
        let _keep_context = &context;
        *flow = ControlFlow::WaitUntil(Instant::now() + Duration::from_millis(100));
        match event {
            Event::WindowEvent { event: WindowEvent::CloseRequested, .. } => fail("interrupted", &latest),
            Event::UserEvent(NativeEvent::PageLoad) => webview.evaluate_script("window.__lazytune?.configure({adblock:true,auto_video:false,volume:0.01});window.__lazytune?.mode('audio')").unwrap(),
            Event::UserEvent(NativeEvent::Message(body)) => {
                if let Ok(value) = serde_json::from_str::<Value>(&body) {
                    if value["event"] == "sample" {
                        latest = value;
                        assert_eq!(latest["videoCalls"], 0, "native cold start called the video toggle");
                        if last_log.elapsed() >= Duration::from_secs(2) {
                            println!("{}", json!({"event":"progress","phase":phase,"sample":latest}));
                            last_log = Instant::now();
                        }
                    } else if value["failed"] == true {
                        fail("adapter reply", &latest);
                    } else if value["id"] == 8 {
                        pause_reply = true;
                        latest = Value::Null;
                    } else if value["id"] == 9 {
                        let prefix = if scenario == "album" { "MPRE" } else { "VLPL" };
                        let id = value["ids"].as_array().and_then(|ids| ids.iter().filter_map(Value::as_str).find(|id| id.starts_with(prefix))).unwrap_or_else(|| fail("collection unavailable", &latest));
                        println!("{}", json!({"event":"play-collection","id":id}));
                        webview.evaluate_script(&request(1, "command", json!({"type":"play_collection","value":id}))).unwrap();
                        phase = "playing";
                        phase_at = Instant::now();
                    }
                }
            }
            _ => {}
        }
        let selected = latest["id"].as_str().is_some_and(|id| !id.is_empty()) && latest["id"] == latest["responseId"] && (matches!(scenario.as_str(), "playlist" | "album") || latest["id"] == "_5nSnBpjTFY");
        match phase {
            "startup" if latest["adapter"] == true && latest["ready"] == true => {
                if scenario == "footer" {
                    webview.evaluate_script("document.querySelector('ytmusic-app').handleNavigationEndpoint({watchEndpoint:{videoId:'_5nSnBpjTFY'}})").unwrap();
                    phase = "selecting";
                } else if matches!(scenario.as_str(), "playlist" | "album") {
                    let query = if scenario == "album" { "Kind of Blue" } else { "lofi playlist" };
                    webview.evaluate_script(&request(9, "catalog", json!({"type":"search","value":query}))).unwrap();
                    phase = "catalog";
                } else {
                    webview.evaluate_script(&request(1, "command", json!({"type":"play","value":"_5nSnBpjTFY"}))).unwrap();
                    phase = "playing";
                }
                phase_at = Instant::now();
            }
            "selecting" if selected && latest["id"] == "_5nSnBpjTFY" => {
                webview.evaluate_script(&request(8, "command", json!({"type":"pause"}))).unwrap();
                phase = "pausing";
                phase_at = Instant::now();
            }
            "pausing" if pause_reply && selected && latest["paused"] == true => {
                if latest["time"] != 0 || latest["bufferedEnd"] != 0 || latest["mediaReady"] != 0 {
                    println!("{}", json!({"event":"footer-preparation-invalid","sample":latest}));
                    std::process::exit(3);
                }
                println!("{}", json!({"event":"footer-prepared","sample":latest}));
                webview.set_visible(true).unwrap();
                webview.evaluate_script(&request(1, "command", json!({"type":"toggle"}))).unwrap();
                phase = "playing";
                phase_at = Instant::now();
            }
            "playing" | "parked-after-hidden" => {
                let time = latest["time"].as_f64().unwrap_or(0.0);
                let passed = selected && latest["paused"] == false && latest["mediaReady"].as_u64().unwrap_or(0) >= 2 && time > 1.5 && time > last_time + 0.1 && (phase == "parked-after-hidden" || latest["error"] == false);
                if passed {
                    println!("{}", json!({"event":"native-cold-start-passed","scenario":scenario,"afterPark":phase=="parked-after-hidden"||scenario!="hidden","noVideoCalls":true,"sample":latest}));
                    std::process::exit(if phase == "parked-after-hidden" { 1 } else { 0 });
                }
                last_time = time;
                if phase_at.elapsed() > Duration::from_secs(25) && scenario == "hidden" && phase == "playing" {
                    println!("{}", json!({"event":"hidden-cold-start-failed","sample":latest}));
                    webview.set_visible(true).unwrap();
                    phase = "parked-after-hidden";
                    phase_at = Instant::now();
                }
            }
            _ => {}
        }
        if phase_at.elapsed() > Duration::from_secs(35) {
            fail(phase, &latest);
        }
    });
}
