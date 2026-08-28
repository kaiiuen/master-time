//! Native system-tray backend.
//!
//! Windows uses the Win32 shell notification API directly, avoiding a tray
//! crate and keeping the platform-neutral command model in `system_tray`.
//! Other targets deliberately remain unsupported and perform no side effects.

use crate::system_tray::{MENU_ITEMS, SystemTrayState, TrayAction, TrayCommand, TrayEvent};

/// Stable identifiers a native menu associates with [`TrayCommand`].
pub const MENU_COMMAND_IDS: [(u16, TrayCommand); 5] = [
    (100, TrayCommand::Show),
    (101, TrayCommand::Hide),
    (102, TrayCommand::StartPolling),
    (103, TrayCommand::StopPolling),
    (104, TrayCommand::Quit),
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrayBackendAvailability {
    WindowsBoundary,
    Unsupported,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrayBackendLifecycle {
    Created,
    Running,
    Stopped,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrayBackendError {
    UnsupportedPlatform,
    NotRunning,
    AlreadyStopped,
    UnknownMenuId(u16),
    WindowsApiFailure(u32),
}

impl TrayBackendError {
    pub const fn is_unsupported(self) -> bool {
        matches!(self, Self::UnsupportedPlatform)
    }
}

pub struct SystemTrayBackend {
    model: SystemTrayState,
    lifecycle: TrayBackendLifecycle,
    platform: platform::State,
}

impl SystemTrayBackend {
    pub fn new(model: SystemTrayState) -> Self {
        Self {
            model,
            lifecycle: TrayBackendLifecycle::Created,
            platform: platform::State::new(),
        }
    }

    pub const fn availability() -> TrayBackendAvailability {
        platform::availability()
    }

    pub const fn lifecycle(&self) -> TrayBackendLifecycle {
        self.lifecycle
    }

    pub const fn model(&self) -> &SystemTrayState {
        &self.model
    }

    /// Creates the message-only-by-visibility window, menu, and shell icon.
    pub fn initialize(&mut self) -> Result<(), TrayBackendError> {
        if self.lifecycle == TrayBackendLifecycle::Stopped {
            return Err(TrayBackendError::AlreadyStopped);
        }
        if self.lifecycle == TrayBackendLifecycle::Running {
            return Ok(());
        }
        platform::initialize(&mut self.platform)?;
        self.lifecycle = TrayBackendLifecycle::Running;
        Ok(())
    }

    /// Applies a command selected by a native callback or by an embedding UI.
    pub fn dispatch_menu_id(&mut self, menu_id: u16) -> Result<TrayAction, TrayBackendError> {
        if Self::availability() == TrayBackendAvailability::Unsupported {
            return Err(TrayBackendError::UnsupportedPlatform);
        }
        let command =
            command_for_menu_id(menu_id).ok_or(TrayBackendError::UnknownMenuId(menu_id))?;
        if self.lifecycle != TrayBackendLifecycle::Running {
            return Err(TrayBackendError::NotRunning);
        }
        Ok(self.model.handle_event(TrayEvent::Command(command)))
    }

    /// Drains the command most recently received by the Windows window proc.
    /// The host should call this while it pumps its normal Windows messages.
    pub fn poll_native_commands(&mut self) -> Result<Option<TrayAction>, TrayBackendError> {
        if Self::availability() == TrayBackendAvailability::Unsupported {
            return Err(TrayBackendError::UnsupportedPlatform);
        }
        if self.lifecycle != TrayBackendLifecycle::Running {
            return Err(TrayBackendError::NotRunning);
        }
        platform::take_command(&mut self.platform)
            .map(|id| self.dispatch_menu_id(id))
            .transpose()
    }

    /// Removes the shell icon and destroys all native resources.
    pub fn shutdown(&mut self) -> Result<(), TrayBackendError> {
        if self.lifecycle == TrayBackendLifecycle::Stopped {
            return Ok(());
        }
        platform::shutdown(&mut self.platform)?;
        self.lifecycle = TrayBackendLifecycle::Stopped;
        Ok(())
    }
}

impl Drop for SystemTrayBackend {
    fn drop(&mut self) {
        let _ = self.shutdown();
    }
}

pub fn command_for_menu_id(menu_id: u16) -> Option<TrayCommand> {
    MENU_COMMAND_IDS
        .iter()
        .find_map(|(id, command)| (*id == menu_id).then_some(*command))
}

pub const fn menu_items() -> &'static [crate::system_tray::TrayMenuItem; 5] {
    &MENU_ITEMS
}

#[cfg(windows)]
mod platform {
    use super::*;
    use std::{cell::Cell, ffi::c_void, mem, ptr};

    type Atom = u16;
    type Hinstance = *mut c_void;
    type Hicon = *mut c_void;
    type Hmenu = *mut c_void;
    type Hwnd = *mut c_void;
    type Lparam = isize;
    type Lresult = isize;
    type Wparam = usize;

    const CS_HREDRAW: u32 = 0x0002;
    const CS_VREDRAW: u32 = 0x0001;
    const CW_USEDEFAULT: i32 = i32::MIN;
    const GWLP_USERDATA: i32 = -21;
    const IDC_ARROW: usize = 32512;
    const IDI_APPLICATION: usize = 32512;
    const MF_STRING: u32 = 0;
    const TPM_BOTTOMALIGN: u32 = 0x0020;
    const TPM_LEFTALIGN: u32 = 0;
    const WM_COMMAND: u32 = 0x0111;
    const WM_DESTROY: u32 = 0x0002;
    const WM_LBUTTONUP: u32 = 0x0202;
    const WM_RBUTTONUP: u32 = 0x0205;

    const WM_APP: u32 = 0x8000;
    const NIM_ADD: u32 = 0;
    const NIM_DELETE: u32 = 2;
    const NIF_MESSAGE: u32 = 1;
    const NIF_ICON: u32 = 2;
    const NIF_TIP: u32 = 4;
    const TRAY_MESSAGE: u32 = WM_APP + 1;

    const CLASS_NAME: &[u16] = &[
        b'M' as u16,
        b'a' as u16,
        b's' as u16,
        b't' as u16,
        b'e' as u16,
        b'r' as u16,
        b'T' as u16,
        b'i' as u16,
        b'm' as u16,
        b'e' as u16,
        0,
    ];

    #[repr(C)]
    struct Point {
        x: i32,
        y: i32,
    }
    #[repr(C)]
    struct CreateStruct {
        lp_create_params: *mut c_void,
        _rest: [usize; 11],
    }
    #[repr(C)]
    struct WndClass {
        style: u32,
        wnd_proc: Option<unsafe extern "system" fn(Hwnd, u32, Wparam, Lparam) -> Lresult>,
        cls_extra: i32,
        wnd_extra: i32,
        instance: Hinstance,
        icon: Hicon,
        cursor: *mut c_void,
        background: *mut c_void,
        menu_name: *const u16,
        class_name: *const u16,
    }
    #[repr(C)]
    struct NotifyIconData {
        cb_size: u32,
        hwnd: Hwnd,
        id: u32,
        flags: u32,
        callback_message: u32,
        icon: Hicon,
        tip: [u16; 128],
        state: u32,
        state_mask: u32,
        info: [u16; 256],
        version: u32,
        info_title: [u16; 64],
        info_flags: u32,
        guid: [u8; 16],
        balloon_icon: Hicon,
    }

    #[link(name = "user32")]
    unsafe extern "system" {
        fn RegisterClassW(class: *const WndClass) -> Atom;
        fn UnregisterClassW(name: *const u16, instance: Hinstance) -> i32;
        fn CreateWindowExW(
            ex: u32,
            class: *const u16,
            name: *const u16,
            style: u32,
            x: i32,
            y: i32,
            w: i32,
            h: i32,
            parent: Hwnd,
            menu: Hmenu,
            instance: Hinstance,
            param: *mut c_void,
        ) -> Hwnd;
        fn DestroyWindow(hwnd: Hwnd) -> i32;
        fn DefWindowProcW(hwnd: Hwnd, msg: u32, wparam: Wparam, lparam: Lparam) -> Lresult;
        fn SetWindowLongPtrW(hwnd: Hwnd, index: i32, value: Lparam) -> Lparam;
        fn GetWindowLongPtrW(hwnd: Hwnd, index: i32) -> Lparam;
        fn CreatePopupMenu() -> Hmenu;
        fn DestroyMenu(menu: Hmenu) -> i32;
        fn AppendMenuW(menu: Hmenu, flags: u32, id: usize, text: *const u16) -> i32;
        fn GetCursorPos(point: *mut Point) -> i32;
        fn SetForegroundWindow(hwnd: Hwnd) -> i32;
        fn TrackPopupMenu(
            menu: Hmenu,
            flags: u32,
            x: i32,
            y: i32,
            reserved: i32,
            hwnd: Hwnd,
            rect: *const c_void,
        ) -> i32;
        fn LoadIconW(instance: Hinstance, name: *const u16) -> Hicon;
        fn LoadCursorW(instance: Hinstance, name: *const u16) -> *mut c_void;
        fn GetModuleHandleW(name: *const u16) -> Hinstance;
        fn GetLastError() -> u32;
    }
    #[link(name = "shell32")]
    unsafe extern "system" {
        fn Shell_NotifyIconW(message: u32, data: *mut NotifyIconData) -> i32;
    }

    pub struct State(Option<Box<NativeTray>>);
    struct NativeTray {
        hwnd: Hwnd,
        menu: Hmenu,
        class_registered: bool,
        icon_added: bool,
        pending: Cell<Option<u16>>,
    }

    pub const fn availability() -> TrayBackendAvailability {
        TrayBackendAvailability::WindowsBoundary
    }
    impl State {
        pub const fn new() -> Self {
            Self(None)
        }
    }

    fn wide(text: &str) -> Vec<u16> {
        text.encode_utf16().chain([0]).collect()
    }
    fn failure() -> TrayBackendError {
        TrayBackendError::WindowsApiFailure(unsafe { GetLastError() })
    }

    pub fn initialize(state: &mut State) -> Result<(), TrayBackendError> {
        let instance = unsafe { GetModuleHandleW(ptr::null()) };
        let class = WndClass {
            style: CS_HREDRAW | CS_VREDRAW,
            wnd_proc: Some(window_proc),
            cls_extra: 0,
            wnd_extra: 0,
            instance,
            icon: ptr::null_mut(),
            cursor: unsafe { LoadCursorW(ptr::null_mut(), IDC_ARROW as *const u16) },
            background: ptr::null_mut(),
            menu_name: ptr::null(),
            class_name: CLASS_NAME.as_ptr(),
        };
        let registered = unsafe { RegisterClassW(&class) } != 0;
        if !registered {
            return Err(failure());
        }
        let menu = unsafe { CreatePopupMenu() };
        if menu.is_null() {
            unsafe {
                UnregisterClassW(CLASS_NAME.as_ptr(), instance);
            }
            return Err(failure());
        }
        for (id, item) in MENU_COMMAND_IDS {
            let label = wide(item_label(item));
            if unsafe { AppendMenuW(menu, MF_STRING, id as usize, label.as_ptr()) } == 0 {
                unsafe {
                    DestroyMenu(menu);
                    UnregisterClassW(CLASS_NAME.as_ptr(), instance);
                }
                return Err(failure());
            }
        }
        let mut tray = Box::new(NativeTray {
            hwnd: ptr::null_mut(),
            menu,
            class_registered: true,
            icon_added: false,
            pending: Cell::new(None),
        });
        let hwnd = unsafe {
            CreateWindowExW(
                0,
                CLASS_NAME.as_ptr(),
                CLASS_NAME.as_ptr(),
                0,
                CW_USEDEFAULT,
                CW_USEDEFAULT,
                0,
                0,
                ptr::null_mut(),
                ptr::null_mut(),
                instance,
                (&mut *tray) as *mut NativeTray as *mut c_void,
            )
        };
        if hwnd.is_null() {
            return Err(failure());
        }
        tray.hwnd = hwnd;
        let mut data: NotifyIconData = unsafe { mem::zeroed() };
        data.cb_size = mem::size_of::<NotifyIconData>() as u32;
        data.hwnd = hwnd;
        data.id = 1;
        data.flags = NIF_MESSAGE | NIF_ICON | NIF_TIP;
        data.callback_message = TRAY_MESSAGE;
        data.icon = unsafe { LoadIconW(ptr::null_mut(), IDI_APPLICATION as *const u16) };
        let tip = wide("Master Time");
        data.tip[..tip.len()].copy_from_slice(&tip);
        if unsafe { Shell_NotifyIconW(NIM_ADD, &mut data) } == 0 {
            return Err(failure());
        }
        tray.icon_added = true;
        state.0 = Some(tray);
        Ok(())
    }

    fn item_label(command: TrayCommand) -> &'static str {
        MENU_ITEMS
            .iter()
            .find(|item| item.command == command)
            .map_or("Command", |item| item.label)
    }

    pub fn take_command(state: &mut State) -> Option<u16> {
        state.0.as_ref()?.pending.take()
    }
    impl Drop for NativeTray {
        fn drop(&mut self) {
            unsafe {
                if self.icon_added {
                    let mut data: NotifyIconData = mem::zeroed();
                    data.cb_size = mem::size_of::<NotifyIconData>() as u32;
                    data.hwnd = self.hwnd;
                    data.id = 1;
                    Shell_NotifyIconW(NIM_DELETE, &mut data);
                }
                if !self.hwnd.is_null() {
                    DestroyWindow(self.hwnd);
                }
                if self.class_registered {
                    let instance = GetModuleHandleW(ptr::null());
                    UnregisterClassW(CLASS_NAME.as_ptr(), instance);
                }
                if !self.menu.is_null() {
                    DestroyMenu(self.menu);
                }
            }
        }
    }

    pub fn shutdown(state: &mut State) -> Result<(), TrayBackendError> {
        state.0.take();
        Ok(())
    }

    unsafe extern "system" fn window_proc(
        hwnd: Hwnd,
        message: u32,
        wparam: Wparam,
        lparam: Lparam,
    ) -> Lresult {
        if message == 0x0081 {
            let create = lparam as *const CreateStruct;
            let tray = unsafe { (*create).lp_create_params as *mut NativeTray };
            unsafe {
                SetWindowLongPtrW(hwnd, GWLP_USERDATA, tray as Lparam);
            }
        }
        let tray = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut NativeTray };
        if !tray.is_null() {
            if message == WM_COMMAND {
                unsafe {
                    (*tray).pending.set(Some((wparam & 0xffff) as u16));
                }
                return 0;
            }
            if message == TRAY_MESSAGE
                && (lparam as u32 == WM_LBUTTONUP || lparam as u32 == WM_RBUTTONUP)
            {
                let mut point = Point { x: 0, y: 0 };
                unsafe {
                    GetCursorPos(&mut point);
                    SetForegroundWindow(hwnd);
                    TrackPopupMenu(
                        (*tray).menu,
                        TPM_LEFTALIGN | TPM_BOTTOMALIGN,
                        point.x,
                        point.y,
                        0,
                        hwnd,
                        ptr::null(),
                    );
                }
                return 0;
            }
        }
        if message == WM_DESTROY {
            return 0;
        }
        unsafe { DefWindowProcW(hwnd, message, wparam, lparam) }
    }
}

#[cfg(not(windows))]
mod platform {
    use super::*;
    pub struct State;
    impl State {
        pub const fn new() -> Self {
            Self
        }
    }
    pub const fn availability() -> TrayBackendAvailability {
        TrayBackendAvailability::Unsupported
    }
    pub fn initialize(_: &mut State) -> Result<(), TrayBackendError> {
        Err(TrayBackendError::UnsupportedPlatform)
    }
    pub fn take_command(_: &mut State) -> Option<u16> {
        None
    }
    pub fn shutdown(_: &mut State) -> Result<(), TrayBackendError> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_mapping_is_stable_and_rejects_unknown_ids() {
        assert_eq!(command_for_menu_id(100), Some(TrayCommand::Show));
        assert_eq!(command_for_menu_id(101), Some(TrayCommand::Hide));
        assert_eq!(command_for_menu_id(102), Some(TrayCommand::StartPolling));
        assert_eq!(command_for_menu_id(103), Some(TrayCommand::StopPolling));
        assert_eq!(command_for_menu_id(104), Some(TrayCommand::Quit));
        assert_eq!(command_for_menu_id(999), None);
    }

    #[cfg(not(windows))]
    #[test]
    fn unsupported_platform_is_side_effect_free() {
        let mut backend = SystemTrayBackend::new(SystemTrayState::new());
        assert_eq!(
            SystemTrayBackend::availability(),
            TrayBackendAvailability::Unsupported
        );
        assert_eq!(
            backend.initialize(),
            Err(TrayBackendError::UnsupportedPlatform)
        );
        assert_eq!(backend.lifecycle(), TrayBackendLifecycle::Created);
        assert_eq!(
            backend.dispatch_menu_id(100),
            Err(TrayBackendError::UnsupportedPlatform)
        );
        assert_eq!(
            backend.poll_native_commands(),
            Err(TrayBackendError::UnsupportedPlatform)
        );
        assert_eq!(backend.shutdown(), Ok(()));
        assert_eq!(backend.lifecycle(), TrayBackendLifecycle::Created);
        assert!(backend.model().window_visible());
    }
}
