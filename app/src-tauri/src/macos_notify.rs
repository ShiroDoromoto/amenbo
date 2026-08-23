//! Deliver macOS notifications through UNUserNotificationCenter, the current API. The stock
//! `tauri-plugin-notification` goes through notify-rust and mac-notification-sys to the
//! **deprecated `NSUserNotification`** (deprecated in macOS 10.14), and on current macOS that old
//! API shows no permission dialog (the plugin's `request_permission` is a stub that always returns
//! Granted), never registers the app under System Settings > Notifications, and has delivery from an
//! unregistered app silently dropped: no toast ever appears. `UNUserNotificationCenter` is the
//! supported API — it registers the app and raises the one-time permission prompt, and it asks
//! nothing of the signature beyond there being one (a locally signed dev build works). It does
//! require the app's bundle ID for `currentNotificationCenter`, so we must be running as a `.app`.
//! What the signature does decide is how long a granted permission LASTS: macOS keys the grant to
//! the app's Designated Requirement, which is why the distributed build is signed with a Developer
//! ID whose DR pins to the team rather than to one certificate — see `scripts/codesign-release-mac.sh`.

#![cfg(target_os = "macos")]

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::OnceLock;

use block2::RcBlock;
use objc2::rc::Retained;
use objc2::runtime::{Bool, ProtocolObject};
use objc2::{define_class, msg_send, AnyThread};
use objc2_foundation::{NSError, NSObject, NSObjectProtocol, NSString};
use objc2_user_notifications::{
    UNAuthorizationOptions, UNMutableNotificationContent, UNNotification,
    UNNotificationPresentationOptions, UNNotificationRequest, UNNotificationResponse,
    UNNotificationSound, UNUserNotificationCenter, UNUserNotificationCenterDelegate,
};
use tauri::{Emitter, Manager};

/// The AppHandle used, when a notification is clicked, to raise the window and ask the front end to
/// navigate to the inbox (set once, in `init`). The delegate runs outside Tauri, in the objc
/// runtime, so it carries no instance state and reads the handle from here.
static APP: OnceLock<tauri::AppHandle> = OnceLock::new();

/// The event emitted to the front end on a click; the webview takes it and opens the inbox.
const ACTIVATED_EVENT: &str = "notification-activated";

/// What a click on the toast does: raise the board and ask the front end to go to the inbox. An
/// arrival notification is an aggregate count (`notifyArrival(n)`) and names no single task, so the
/// inbox is as specific as the destination can get — and the inbox is the board's (`crate::windows`).
fn on_activated() {
    let Some(app) = APP.get() else { return };
    // The click has the OS activate the app, but restoring from minimized/hidden and coming to the
    // front are spelled out here.
    if let Some(win) = app.get_webview_window(crate::windows::BOARD) {
        let _ = win.unminimize();
        let _ = win.show();
        let _ = win.set_focus();
    }
    // Opening the inbox is the front end's navigation job, so ask for it with an event rather than
    // resolving it outside Tauri.
    if let Err(e) = app.emit(ACTIVATED_EVENT, ()) {
        log::warn!("failed to emit {ACTIVATED_EVENT}: {e}");
    }
}

define_class!(
    // SAFETY:
    // - The superclass NSObject imposes no constraints on subclassing.
    // - We implement no Drop, so no generated dealloc is needed.
    #[unsafe(super(NSObject))]
    #[name = "AmenboUNDelegate"]
    struct Delegate;

    unsafe impl NSObjectProtocol for Delegate {}

    unsafe impl UNUserNotificationCenterDelegate for Delegate {
        // Present banner, list and sound even while the app is frontmost (the default suppresses them).
        #[unsafe(method(userNotificationCenter:willPresentNotification:withCompletionHandler:))]
        fn will_present(
            &self,
            _center: &UNUserNotificationCenter,
            _notification: &UNNotification,
            handler: &block2::DynBlock<dyn Fn(UNNotificationPresentationOptions)>,
        ) {
            let opts = UNNotificationPresentationOptions::Banner
                | UNNotificationPresentationOptions::List
                | UNNotificationPresentationOptions::Sound;
            handler.call((opts,));
        }

        // The response to a click on the toast (the default action): raise the window, ask for the
        // inbox, and always call the completion handler (skip it and the OS calls us unresponsive).
        #[unsafe(method(userNotificationCenter:didReceiveNotificationResponse:withCompletionHandler:))]
        fn did_receive(
            &self,
            _center: &UNUserNotificationCenter,
            _response: &UNNotificationResponse,
            handler: &block2::DynBlock<dyn Fn()>,
        ) {
            on_activated();
            handler.call(());
        }
    }
);

impl Delegate {
    fn new() -> Retained<Self> {
        unsafe { msg_send![Self::alloc(), init] }
    }
}

/// The running number for notification IDs: a repeated ID replaces the earlier toast, so every
/// arrival gets a fresh one. No `Date`, no randomness.
static SEQ: AtomicU64 = AtomicU64::new(0);

/// Are we running inside a `.app` bundle? `UNUserNotificationCenter` needs the app's bundle ID (see
/// module docs); an unbundled binary — `tauri dev`, or a bare `cargo run` — has no bundle proxy, so
/// `currentNotificationCenter` throws an NSException (`bundleProxyForCurrentProcess is nil`) and
/// aborts the process. Detect the bundle by the executable's location so both entry points below can
/// no-op out of a dev run rather than crash it; a real install always runs from `.app/Contents/MacOS/`.
fn is_bundled() -> bool {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.to_str().map(|s| s.contains(".app/Contents/MacOS/")))
        .unwrap_or(false)
}

/// Once at startup: install the delegate, ask for permission, and register the app.
///
/// Call this from the main thread (from Tauri's `setup` closure). `setDelegate:` keeps only a weak
/// reference, so the delegate is `forget`ed and lives for the life of the app.
pub fn init(app: tauri::AppHandle) {
    // Unbundled (dev) run: `currentNotificationCenter` would abort the process, so skip OS
    // notifications entirely. `send` is guarded the same way.
    if !is_bundled() {
        log::info!("macos notifications disabled: not running from a .app bundle");
        return;
    }

    // Hold on to the AppHandle so the click response (`on_activated`) can reach the front end (once;
    // later calls are ignored).
    let _ = APP.set(app);

    let center = UNUserNotificationCenter::currentNotificationCenter();

    let delegate = Delegate::new();
    let proto = ProtocolObject::from_ref(&*delegate);
    center.setDelegate(Some(proto));
    std::mem::forget(delegate); // A singleton for the life of the app (the delegate is held weakly).

    let opts = UNAuthorizationOptions::Alert
        | UNAuthorizationOptions::Sound
        | UNAuthorizationOptions::Badge;
    let handler = RcBlock::new(move |granted: Bool, err: *mut NSError| {
        if !granted.as_bool() {
            log::info!("os notification authorization not granted");
        }
        if !err.is_null() {
            // The NSError is owned by the autorelease pool; we only borrow it.
            let msg = unsafe { &*err }.localizedDescription();
            log::warn!("os notification authorization error: {msg}");
        }
    });
    center.requestAuthorizationWithOptions_completionHandler(opts, &handler);
}

/// Raise one OS notification with the given title and body. Without permission the OS drops it
/// silently (the sound comes from a separate path).
pub fn send(title: &str, body: &str) {
    // Unbundled (dev) run: no bundle proxy, so `currentNotificationCenter` would abort. `init` already
    // logged that notifications are off; here we just drop the toast.
    if !is_bundled() {
        return;
    }

    let center = UNUserNotificationCenter::currentNotificationCenter();

    let content = UNMutableNotificationContent::new();
    content.setTitle(&NSString::from_str(title));
    content.setBody(&NSString::from_str(body));
    content.setSound(Some(&UNNotificationSound::defaultSound()));

    let n = SEQ.fetch_add(1, Ordering::Relaxed);
    let ident = format!("amenbo-arrival-{n}");
    let request = UNNotificationRequest::requestWithIdentifier_content_trigger(
        &NSString::from_str(&ident),
        &content,
        None,
    );

    // Scheduling is asynchronous and this is the only channel it answers on: the call returns before
    // the OS has decided anything, so a request it then refuses — permission revoked since `init`
    // asked, a content the OS rejects, a delivery quota — leaves no trace unless the handler takes it.
    // Passing `None` here made "the toast never appeared" indistinguishable from "we never asked".
    // The identifier goes in the line so one silent arrival can be told from another.
    let handler = RcBlock::new(move |err: *mut NSError| {
        if !err.is_null() {
            // The NSError is owned by the autorelease pool; we only borrow it.
            let msg = unsafe { &*err }.localizedDescription();
            log::warn!("os notification {ident} was not scheduled: {msg}");
        }
    });
    center.addNotificationRequest_withCompletionHandler(&request, Some(&handler));
}
