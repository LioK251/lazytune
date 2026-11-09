use crate::app::{self, AppState};
use lazytune_model::{PlaybackSnapshot, PlayerCommand};
use std::{cell::Cell, rc::Rc, sync::LazyLock};
use tauri::{AppHandle, Emitter, Manager};
use windows::{
    core::w,
    Win32::{
        Foundation::{HWND, LPARAM, LRESULT, WPARAM},
        System::Com::{CoCreateInstance, CLSCTX_INPROC_SERVER},
        UI::{
            Shell::{
                DefSubclassProc, ITaskbarList3, RemoveWindowSubclass, SetWindowSubclass,
                TaskbarList, THBF_DISABLED, THBF_ENABLED, THBN_CLICKED, THB_FLAGS, THB_ICON,
                THB_TOOLTIP, THUMBBUTTON,
            },
            WindowsAndMessaging::{
                CreateIconFromResourceEx, DestroyIcon, PostMessageW, RegisterWindowMessageW, HICON,
                LR_DEFAULTCOLOR, WM_COMMAND, WM_NCDESTROY,
            },
        },
    },
};

const FIRST_BUTTON: u32 = 0xA110;
const SUBCLASS_ID: usize = 0x4C545442;
static UPDATE_MESSAGE: LazyLock<u32> =
    LazyLock::new(|| unsafe { RegisterWindowMessageW(w!("lazytune.taskbar.update")) });
const ICONS: [&[u8]; 6] = [
    include_bytes!("../icons/taskbar/like.png"),
    include_bytes!("../icons/taskbar/liked.png"),
    include_bytes!("../icons/taskbar/previous.png"),
    include_bytes!("../icons/taskbar/play.png"),
    include_bytes!("../icons/taskbar/pause.png"),
    include_bytes!("../icons/taskbar/next.png"),
];

struct Icon(HICON);
impl Icon {
    fn load(data: &[u8]) -> windows::core::Result<Self> {
        unsafe { CreateIconFromResourceEx(data, true, 0x00030000, 0, 0, LR_DEFAULTCOLOR).map(Self) }
    }
}
impl Drop for Icon {
    fn drop(&mut self) {
        let _ = unsafe { DestroyIcon(self.0) };
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct State {
    available: bool,
    playing: bool,
    liked: bool,
    signed_in: bool,
}
impl From<&PlaybackSnapshot> for State {
    fn from(p: &PlaybackSnapshot) -> Self {
        Self {
            available: p.ready && !p.track.id.is_empty(),
            playing: p.playing,
            liked: p.liked,
            signed_in: p.signed_in,
        }
    }
}

struct Toolbar {
    app: AppHandle,
    hwnd: HWND,
    shell: ITaskbarList3,
    icons: Vec<Icon>,
    created_message: u32,
    added: Cell<bool>,
    last: Cell<Option<State>>,
}
impl Toolbar {
    fn refresh(&self) {
        let state = State::from(&*self.app.state::<AppState>().playback.lock().unwrap());
        if self.last.get() == Some(state) {
            return;
        }
        let buttons = buttons(&self.icons, state);
        let result = unsafe {
            if self.added.get() {
                self.shell.ThumbBarUpdateButtons(self.hwnd, &buttons)
            } else {
                self.shell.ThumbBarAddButtons(self.hwnd, &buttons)
            }
        };
        if result.is_ok() {
            #[cfg(debug_assertions)]
            if !self.added.get() {
                eprintln!("lazytune: taskbar controls ready");
            }
            self.added.set(true);
            self.last.set(Some(state));
        }
    }
}

fn buttons(icons: &[Icon], state: State) -> [THUMBBUTTON; 4] {
    let like_tip = if !state.signed_in {
        "Sign in to like tracks"
    } else if state.liked {
        "Unlike"
    } else {
        "Like"
    };
    [
        (
            FIRST_BUTTON,
            usize::from(state.liked),
            like_tip,
            state.signed_in,
        ),
        (FIRST_BUTTON + 1, 2, "Previous", true),
        (
            FIRST_BUTTON + 2,
            if state.playing { 4 } else { 3 },
            if state.playing { "Pause" } else { "Play" },
            true,
        ),
        (FIRST_BUTTON + 3, 5, "Next", true),
    ]
    .map(|(id, icon, tip, enabled)| {
        let mut button = THUMBBUTTON {
            dwMask: THB_FLAGS | THB_ICON | THB_TOOLTIP,
            iId: id,
            hIcon: icons[icon].0,
            dwFlags: if state.available && enabled {
                THBF_ENABLED
            } else {
                THBF_DISABLED
            },
            ..Default::default()
        };
        for (target, value) in button.szTip.iter_mut().zip(tip.encode_utf16()) {
            *target = value;
        }
        button
    })
}

fn command(wparam: WPARAM, state: State) -> Option<PlayerCommand> {
    if (wparam.0 >> 16) & 0xffff != THBN_CLICKED as usize || !state.available {
        return None;
    }
    match (wparam.0 & 0xffff) as u32 {
        id if id == FIRST_BUTTON && state.signed_in => Some(PlayerCommand::Like(!state.liked)),
        id if id == FIRST_BUTTON + 1 => Some(PlayerCommand::Previous),
        id if id == FIRST_BUTTON + 2 => Some(PlayerCommand::Toggle),
        id if id == FIRST_BUTTON + 3 => Some(PlayerCommand::Next),
        _ => None,
    }
}

// The subclass owns one reference; COM and icon handles remain on Tauri's UI thread.
unsafe extern "system" fn window_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    _id: usize,
    data: usize,
) -> LRESULT {
    // Shell calls can pump messages and destroy the window before this callback returns.
    let toolbar = unsafe {
        Rc::increment_strong_count(data as *const Toolbar);
        Rc::from_raw(data as *const Toolbar)
    };
    if message == WM_NCDESTROY {
        unsafe {
            let _ = RemoveWindowSubclass(hwnd, Some(window_proc), SUBCLASS_ID);
            let result = DefSubclassProc(hwnd, message, wparam, lparam);
            drop(Rc::from_raw(data as *const Toolbar));
            return result;
        }
    }
    if message == toolbar.created_message {
        toolbar.added.set(false);
        toolbar.last.set(None);
        toolbar.refresh();
    } else if message == *UPDATE_MESSAGE {
        toolbar.refresh();
        return LRESULT(0);
    } else if message == WM_COMMAND {
        let state = State::from(&*toolbar.app.state::<AppState>().playback.lock().unwrap());
        if let Some(command) = command(wparam, state) {
            let app = toolbar.app.clone();
            tauri::async_runtime::spawn(async move {
                if let Err(error) = app::control(&app, command).await {
                    let _ = app.emit_to("main", "notice", error);
                }
            });
            return LRESULT(0);
        }
    }
    unsafe { DefSubclassProc(hwnd, message, wparam, lparam) }
}

pub fn setup(app: &AppHandle) -> Result<(), Box<dyn std::error::Error>> {
    let window = app.get_window("main").ok_or("Main window unavailable")?;
    let hwnd = window.hwnd()?;
    let created_message = unsafe { RegisterWindowMessageW(w!("TaskbarButtonCreated")) };
    if created_message == 0 || *UPDATE_MESSAGE == 0 {
        return Err(windows::core::Error::from_thread().into());
    }
    let shell: ITaskbarList3 =
        unsafe { CoCreateInstance(&TaskbarList, None, CLSCTX_INPROC_SERVER)? };
    unsafe { shell.HrInit()? };
    let toolbar = Rc::new(Toolbar {
        app: app.clone(),
        hwnd,
        shell,
        icons: ICONS
            .into_iter()
            .map(Icon::load)
            .collect::<Result<_, _>>()?,
        created_message,
        added: Cell::new(false),
        last: Cell::new(None),
    });
    unsafe {
        SetWindowSubclass(
            hwnd,
            Some(window_proc),
            SUBCLASS_ID,
            &*toolbar as *const Toolbar as usize,
        )
        .ok()?;
    }
    let _ = Rc::into_raw(toolbar);
    update(app);
    Ok(())
}

pub fn update(app: &AppHandle) {
    if let Some(window) = app.get_window("main") {
        if let Ok(hwnd) = window.hwnd() {
            let _ = unsafe { PostMessageW(Some(hwnd), *UPDATE_MESSAGE, WPARAM(0), LPARAM(0)) };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn toolbar_icons_states_and_commands() {
        let icons: Vec<_> = ICONS
            .into_iter()
            .map(|data| Icon::load(data).unwrap())
            .collect();
        let mut state = State {
            available: true,
            playing: true,
            liked: false,
            signed_in: true,
        };
        let click =
            |index| WPARAM(((THBN_CLICKED as usize) << 16) | (FIRST_BUTTON as usize + index));
        let active = buttons(&icons, state);
        assert_eq!(active[2].hIcon, icons[4].0);
        assert!(active.iter().all(|b| b.dwFlags == THBF_ENABLED));
        assert!(matches!(
            command(click(0), state),
            Some(PlayerCommand::Like(true))
        ));
        assert!(matches!(
            command(click(1), state),
            Some(PlayerCommand::Previous)
        ));
        assert!(matches!(
            command(click(2), state),
            Some(PlayerCommand::Toggle)
        ));
        assert!(matches!(
            command(click(3), state),
            Some(PlayerCommand::Next)
        ));
        assert!(command(WPARAM(FIRST_BUTTON as usize), state).is_none());
        state.playing = false;
        state.liked = true;
        let paused = buttons(&icons, state);
        assert_eq!(paused[0].hIcon, icons[1].0);
        assert_eq!(paused[2].hIcon, icons[3].0);
        assert!(matches!(
            command(click(0), state),
            Some(PlayerCommand::Like(false))
        ));
        state.signed_in = false;
        assert_eq!(buttons(&icons, state)[0].dwFlags, THBF_DISABLED);
        assert!(command(click(0), state).is_none());
        state.available = false;
        assert!(buttons(&icons, state)
            .iter()
            .all(|b| b.dwFlags == THBF_DISABLED));
        assert!(command(click(2), state).is_none());
    }
}
