//! The system clipboard (§13.1/#353) — the shell's half of the help panel's
//! `copy [c]` control.
//!
//! The core owns the control's geometry and the token it names; the *write* is
//! platform, so it lives here (§12.1). The seam is one function: hand it a string and
//! a callback, and it says whether the string reached the clipboard.
//!
//! **It is reached through [`js_sys::Reflect`] rather than a typed binding.** The
//! whole question this module has to answer is "*is there a clipboard here at all?*",
//! and that is a property lookup: on an insecure origin — a `file://` page, plain
//! `http`, some framed contexts — `navigator.clipboard` is simply not there, and a
//! typed accessor would have nothing honest to return. Looking the property up lets
//! the absent case answer `false` on the spot instead of throwing.
//!
//! **Two ways it can fail, and only one of them is visible up front.** Absence is
//! synchronous; a *refusal* is not — `writeText` hands back a promise, and a frame
//! without clipboard permission (which is what the artifact build's `<iframe>` may
//! be) rejects it a microtask later. So the caller is told twice: once by the return
//! value, for "there is nothing to write to", and once through the callback, for what
//! the browser eventually said. Nothing here ever reports a copy it did not see
//! succeed.

use std::cell::RefCell;
use std::rc::Rc;

use js_sys::{Function, Promise, Reflect};
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;

/// Ask the browser to put `text` on the system clipboard, calling `done` with the
/// browser's answer once it settles.
///
/// Returns whether the write was even **started**: `false` means this page has no
/// clipboard to write to, and `done` is then never called at all. That split is
/// deliberate — a caller is usually inside its own `RefCell` borrow when it asks, so
/// calling `done` synchronously here would re-enter it. Everything the callback path
/// carries arrives in a microtask, after that borrow is gone.
pub(crate) fn write_text<F>(text: &str, done: F) -> bool
where
    F: FnOnce(bool) + 'static,
{
    let Some(promise) = write_promise(text) else {
        return false;
    };
    settle(&promise, done);
    true
}

/// Call `navigator.clipboard.writeText(text)` and hand back its promise, or `None`
/// when there is no such call to make — no window, no `clipboard` on the navigator
/// (the insecure-context case), or a `writeText` that threw on the spot.
fn write_promise(text: &str) -> Option<Promise> {
    let navigator = web_sys::window()?.navigator();
    let clipboard = Reflect::get(&navigator, &JsValue::from_str("clipboard")).ok()?;
    let write = Reflect::get(&clipboard, &JsValue::from_str("writeText"))
        .ok()?
        .dyn_into::<Function>()
        .ok()?;
    write
        .call1(&clipboard, &JsValue::from_str(text))
        .ok()?
        .dyn_into::<Promise>()
        .ok()
}

/// Wire `done` to both ends of `promise` — `true` on fulfil, `false` on reject.
///
/// The two handlers are `Closure::once_into_js`, so JS owns each one and frees it
/// after the single call a promise ever makes; nothing is leaked per press. They share
/// one `Option<F>` and take it, so `done` runs exactly once even if a host ever
/// settled a promise both ways.
fn settle<F>(promise: &Promise, done: F)
where
    F: FnOnce(bool) + 'static,
{
    let once = Rc::new(RefCell::new(Some(done)));
    let handler = |once: Rc<RefCell<Option<F>>>, ok: bool| {
        Closure::once_into_js(move |_: JsValue| {
            if let Some(done) = once.borrow_mut().take() {
                done(ok);
            }
        })
    };
    let fulfilled = handler(once.clone(), true);
    let rejected = handler(once, false);
    // `then` off the promise itself, for the same reason the write is: this module
    // talks to the platform through property lookups, and a host that somehow handed
    // back a thenless object should leave the caller with no acknowledgement rather
    // than a panic.
    let Some(then) = Reflect::get(promise, &JsValue::from_str("then"))
        .ok()
        .and_then(|f| f.dyn_into::<Function>().ok())
    else {
        return;
    };
    let _ = then.call2(promise, &fulfilled, &rejected);
}
