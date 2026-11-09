use super::*;

#[component]
pub fn PlaybackSlider(ui: Ui, seek: bool) -> impl IntoView {
    let preview = RwSignal::new(None::<f64>);
    let editing = RwSignal::new(false);
    let revision = RwSignal::new(0u64);
    let target = RwSignal::new(None::<(u64, f64)>);
    let sending = RwSignal::new(false);
    let reported = Memo::new(move |_| {
        ui.playback
            .with(|p| if seek { p.position } else { p.volume })
    });
    let maximum = Memo::new(move |_| {
        if seek {
            ui.playback.with(|p| p.duration).max(1.0)
        } else {
            1.0
        }
    });
    let value = Memo::new(move |_| preview.get().unwrap_or_else(|| reported.get()));

    // Command replies can arrive before the media engine reports the new value.
    Effect::new(move || {
        let actual = reported.get();
        if !editing.get()
            && !sending.get()
            && preview
                .get()
                .is_some_and(|v| (actual - v).abs() < if seek { 1.0 } else { 0.005 })
        {
            preview.set(None);
        }
    });
    if seek {
        let track_id = Memo::new(move |_| ui.track.with(|t| t.id.clone()));
        Effect::new(move || {
            track_id.get();
            revision.update(|r| *r += 1);
            preview.set(None);
            editing.set(false);
            target.set(None);
        });
    }
    let send = move |v| {
        target.set(Some((revision.get_untracked(), v)));
        if sending.get_untracked() {
            return;
        }
        sending.set(true);
        spawn_local(async move {
            while let Some((sent_revision, v)) = target.try_update(Option::take).flatten() {
                let command = if seek {
                    PlayerCommand::Seek {
                        position: v,
                        track_id: ui.track.with_untracked(|t| t.id.clone()),
                    }
                } else {
                    PlayerCommand::Volume(v)
                };
                match call::<()>("player_command", json!({"command":command})).await {
                    Ok(()) => set_timeout(
                        move || {
                            if revision.get_untracked() == sent_revision && !editing.get_untracked()
                            {
                                preview.set(None);
                            }
                        },
                        Duration::from_secs(2),
                    ),
                    Err(e) => {
                        ui.notice.set(e);
                        if revision.get_untracked() == sent_revision {
                            preview.set(None);
                            editing.set(false);
                        }
                    }
                }
            }
            sending.set(false);
        });
    };
    view! {
        <input class=if seek {"seek"} else {"volume"} type="range"
            aria-label=if seek {"Seek"} else {"Volume"} min="0"
            max=move||maximum.get().to_string() step=if seek {"0.1"} else {"0.01"}
            prop:value=move||value.get().to_string()
            disabled=move||seek&&!ui.playback.with(|p|p.seekable)
            aria-valuetext=move||if seek {time(value.get())} else {format!("{}%", (value.get()*100.0).round())}
            on:pointerdown=move|_|{
                revision.update(|r|*r+=1);
                editing.set(true);
                preview.set(Some(value.get_untracked()));
            }
            on:input=move|e|{if let Ok(v)=event_target_value(&e).parse(){
                revision.update(|r|*r+=1);
                editing.set(true);
                preview.set(Some(v));
                if !seek {send(v);}
            }}
            on:change=move|e|{if let Ok(v)=event_target_value(&e).parse(){
                preview.set(Some(v));
                send(v);
                editing.set(false);
            }}
            on:pointerup=move|_|editing.set(false)
            on:pointercancel=move|_|{
                revision.update(|r|*r+=1);
                editing.set(false);
                preview.set(None);
            }
        />
    }
}
