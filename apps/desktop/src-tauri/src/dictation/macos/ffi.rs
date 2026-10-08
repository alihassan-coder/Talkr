//! The C APIs dictation needs from Core Foundation, Core Graphics (event taps and synthetic
//! keys), the Accessibility API (HIServices) and Carbon, declared by hand: they are small, stable
//! C interfaces, and declaring exactly what is used keeps the build free of large binding crates.

#![allow(non_upper_case_globals)]

use std::ffi::{c_ulong, c_void};
use std::ptr::NonNull;

pub type CFTypeRef = *const c_void;
pub type CFStringRef = *const c_void;
pub type CFIndex = isize;
pub type CFTypeID = c_ulong;
pub type CGEventRef = *mut c_void;
pub type CFMachPortRef = *mut c_void;
pub type CFRunLoopRef = *mut c_void;
pub type CFRunLoopSourceRef = *mut c_void;
pub type AXError = i32;

pub const AX_SUCCESS: AXError = 0;
const UTF8: u32 = 0x0800_0100;

#[repr(C)]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CFRange {
    pub location: CFIndex,
    pub length: CFIndex,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct CGPoint {
    pub x: f64,
    pub y: f64,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct CGSize {
    pub width: f64,
    pub height: f64,
}


pub type TapCallback = unsafe extern "C" fn(proxy: *mut c_void, kind: u32, event: CGEventRef, info: *mut c_void) -> CGEventRef;

// CGEventTapLocation, CGEventTapPlacement, CGEventTapOptions
pub const HID_EVENT_TAP: u32 = 0;
pub const SESSION_EVENT_TAP: u32 = 1;
pub const HEAD_INSERT: u32 = 0;
pub const TAP_DEFAULT: u32 = 0;
// CGEventType
pub const KEY_DOWN: u32 = 10;
pub const KEY_UP: u32 = 11;
pub const FLAGS_CHANGED: u32 = 12;
pub const TAP_DISABLED_BY_TIMEOUT: u32 = 0xFFFF_FFFE;
pub const TAP_DISABLED_BY_USER_INPUT: u32 = 0xFFFF_FFFF;
// CGEventField
pub const FIELD_KEYCODE: u32 = 9;
pub const FIELD_SOURCE_USER_DATA: u32 = 42;
// CGEventSourceStateID
pub const STATE_PRIVATE: i32 = -1;
pub const STATE_COMBINED_SESSION: i32 = 0;
// AXValueType
pub const AX_VALUE_CGPOINT: u32 = 1;
pub const AX_VALUE_CGSIZE: u32 = 2;
pub const AX_VALUE_CFRANGE: u32 = 4;

#[link(name = "CoreFoundation", kind = "framework")]
extern "C" {
    pub static kCFRunLoopCommonModes: CFStringRef;
    pub static kCFRunLoopDefaultMode: CFStringRef;
    pub static kCFBooleanTrue: CFTypeRef;
    /// `CFDictionaryKeyCallBacks` and `CFDictionaryValueCallBacks`: a version and function
    /// pointers, only ever passed by address.
    pub static kCFTypeDictionaryKeyCallBacks: [usize; 6];
    pub static kCFTypeDictionaryValueCallBacks: [usize; 5];

    pub fn CFRelease(cf: CFTypeRef);
    pub fn CFRetain(cf: CFTypeRef) -> CFTypeRef;
    pub fn CFGetTypeID(cf: CFTypeRef) -> CFTypeID;
    pub fn CFStringGetTypeID() -> CFTypeID;
    pub fn CFNumberGetTypeID() -> CFTypeID;
    pub fn CFNumberGetValue(number: CFTypeRef, kind: CFIndex, value: *mut c_void) -> u8;
    pub fn CFStringCreateWithBytes(
        alloc: CFTypeRef,
        bytes: *const u8,
        len: CFIndex,
        encoding: u32,
        external: u8,
    ) -> CFStringRef;
    pub fn CFStringGetLength(string: CFStringRef) -> CFIndex;
    pub fn CFStringGetCharacters(string: CFStringRef, range: CFRange, buffer: *mut u16);
    pub fn CFDictionaryCreate(
        alloc: CFTypeRef,
        keys: *const CFTypeRef,
        values: *const CFTypeRef,
        count: CFIndex,
        key_callbacks: *const c_void,
        value_callbacks: *const c_void,
    ) -> CFTypeRef;
    pub fn CFMachPortCreateRunLoopSource(alloc: CFTypeRef, port: CFMachPortRef, order: CFIndex) -> CFRunLoopSourceRef;
    pub fn CFMachPortInvalidate(port: CFMachPortRef);
    pub fn CFRunLoopGetCurrent() -> CFRunLoopRef;
    pub fn CFRunLoopAddSource(run_loop: CFRunLoopRef, source: CFRunLoopSourceRef, mode: CFStringRef);
    pub fn CFRunLoopRemoveSource(run_loop: CFRunLoopRef, source: CFRunLoopSourceRef, mode: CFStringRef);
    pub fn CFRunLoopRunInMode(mode: CFStringRef, seconds: f64, return_after_source_handled: u8) -> i32;
    pub fn CFRunLoopStop(run_loop: CFRunLoopRef);
}

#[link(name = "CoreGraphics", kind = "framework")]
extern "C" {
    pub fn CGEventTapCreate(
        tap: u32,
        place: u32,
        options: u32,
        events_of_interest: u64,
        callback: TapCallback,
        info: *mut c_void,
    ) -> CFMachPortRef;
    pub fn CGEventTapEnable(tap: CFMachPortRef, enable: bool);
    pub fn CGEventTapIsEnabled(tap: CFMachPortRef) -> bool;
    pub fn CGEventGetIntegerValueField(event: CGEventRef, field: u32) -> i64;
    pub fn CGEventSetIntegerValueField(event: CGEventRef, field: u32, value: i64);
    pub fn CGEventGetFlags(event: CGEventRef) -> u64;
    pub fn CGEventSetFlags(event: CGEventRef, flags: u64);
    pub fn CGEventKeyboardGetUnicodeString(event: CGEventRef, max: c_ulong, actual: *mut c_ulong, buffer: *mut u16);
    pub fn CGEventKeyboardSetUnicodeString(event: CGEventRef, length: c_ulong, string: *const u16);
    pub fn CGEventCreateKeyboardEvent(source: CFTypeRef, key: u16, down: bool) -> CGEventRef;
    pub fn CGEventPost(tap: u32, event: CGEventRef);
    pub fn CGEventSourceCreate(state: i32) -> CFTypeRef;
    pub fn CGEventSourceKeyState(state: i32, key: u16) -> bool;
    pub fn CGEventSourceFlagsState(state: i32) -> u64;
}

#[link(name = "ApplicationServices", kind = "framework")]
extern "C" {
    pub static kAXTrustedCheckOptionPrompt: CFStringRef;

    pub fn AXIsProcessTrusted() -> u8;
    pub fn AXIsProcessTrustedWithOptions(options: CFTypeRef) -> u8;
    pub fn AXUIElementCreateSystemWide() -> CFTypeRef;
    pub fn AXUIElementCreateApplication(pid: i32) -> CFTypeRef;
    pub fn AXUIElementCopyAttributeValue(element: CFTypeRef, attribute: CFStringRef, value: *mut CFTypeRef) -> AXError;
    pub fn AXUIElementCopyParameterizedAttributeValue(
        element: CFTypeRef,
        attribute: CFStringRef,
        parameter: CFTypeRef,
        value: *mut CFTypeRef,
    ) -> AXError;
    pub fn AXUIElementIsAttributeSettable(element: CFTypeRef, attribute: CFStringRef, settable: *mut u8) -> AXError;
    pub fn AXUIElementSetAttributeValue(element: CFTypeRef, attribute: CFStringRef, value: CFTypeRef) -> AXError;
    pub fn AXUIElementSetMessagingTimeout(element: CFTypeRef, seconds: f32) -> AXError;
    #[cfg(test)]
    pub fn AXUIElementPerformAction(element: CFTypeRef, action: CFStringRef) -> AXError;
    pub fn AXUIElementGetPid(element: CFTypeRef, pid: *mut i32) -> AXError;
    pub fn AXValueCreate(kind: u32, value: *const c_void) -> CFTypeRef;
    pub fn AXValueGetValue(value: CFTypeRef, kind: u32, out: *mut c_void) -> u8;
    pub fn AXValueGetTypeID() -> CFTypeID;
}

#[link(name = "Carbon", kind = "framework")]
extern "C" {
    pub fn IsSecureEventInputEnabled() -> u8;
}

/// An owned Core Foundation object, released when dropped.
#[derive(Debug)]
pub struct Cf(NonNull<c_void>);

// SAFETY: Core Foundation reference counting is thread-safe, and every object this wraps
// (strings, numbers, AXUIElements, AXValues) is immutable or documented as usable from any thread.
unsafe impl Send for Cf {}

impl Cf {
    /// Take ownership of an object from a Create or Copy function (`None` for NULL).
    ///
    /// # Safety
    /// `raw` must be NULL or a valid CF object the caller owns one reference to.
    pub unsafe fn from_owned(raw: CFTypeRef) -> Option<Cf> {
        NonNull::new(raw.cast_mut()).map(Cf)
    }

    pub fn as_ptr(&self) -> CFTypeRef {
        self.0.as_ptr()
    }

    fn is(&self, type_id: CFTypeID) -> bool {
        // SAFETY: a valid CF object.
        unsafe { CFGetTypeID(self.as_ptr()) == type_id }
    }

    /// The object as a string, if it is one.
    pub fn to_utf16(&self) -> Option<Vec<u16>> {
        // SAFETY: plain type query.
        if !self.is(unsafe { CFStringGetTypeID() }) {
            return None;
        }
        // SAFETY: a CFString; the buffer holds exactly the requested range.
        unsafe {
            let len = CFStringGetLength(self.as_ptr());
            let mut buf = vec![0u16; len.max(0) as usize];
            CFStringGetCharacters(self.as_ptr(), CFRange { location: 0, length: len }, buf.as_mut_ptr());
            Some(buf)
        }
    }

    pub fn to_string_lossy(&self) -> Option<String> {
        self.to_utf16().map(|units| String::from_utf16_lossy(&units))
    }

    pub fn to_i64(&self) -> Option<i64> {
        const SINT64: CFIndex = 4;
        let mut out = 0i64;
        // SAFETY: type checked; the out pointer is an i64 as kCFNumberSInt64Type asks.
        let ok = unsafe { self.is(CFNumberGetTypeID()) && CFNumberGetValue(self.as_ptr(), SINT64, (&raw mut out).cast()) != 0 };
        ok.then_some(out)
    }

    /// The object as an AXValue of `kind`, read into a `T` of the matching layout.
    pub fn to_ax_value<T: Default>(&self, kind: u32) -> Option<T> {
        let mut out = T::default();
        // SAFETY: type checked; callers pair each kind with its C struct (CFRange, CGPoint, CGSize).
        let ok = unsafe { self.is(AXValueGetTypeID()) && AXValueGetValue(self.as_ptr(), kind, (&raw mut out).cast()) != 0 };
        ok.then_some(out)
    }
}

impl Drop for Cf {
    fn drop(&mut self) {
        // SAFETY: we own one reference.
        unsafe { CFRelease(self.as_ptr()) }
    }
}

/// A CFString from Rust text.
pub fn cf_string(text: &str) -> Option<Cf> {
    // SAFETY: the bytes are valid UTF-8 of the given length; the result is owned.
    unsafe { Cf::from_owned(CFStringCreateWithBytes(std::ptr::null(), text.as_ptr(), text.len() as CFIndex, UTF8, 0)) }
}

/// Whether macOS lets Talkr use the Accessibility API, listen to the keyboard and post keys.
pub fn trusted() -> bool {
    // SAFETY: plain query.
    unsafe { AXIsProcessTrusted() != 0 }
}

/// Ask macOS for Accessibility access: it shows its prompt (once per app; afterwards only System
/// Settings can grant it). Returns whether access is already granted.
pub fn prompt_for_trust() -> bool {
    // SAFETY: a one-entry dictionary of CF constants; it is released after the call.
    unsafe {
        let keys = [kAXTrustedCheckOptionPrompt];
        let values = [kCFBooleanTrue];
        let options = Cf::from_owned(CFDictionaryCreate(
            std::ptr::null(),
            keys.as_ptr(),
            values.as_ptr(),
            1,
            (&raw const kCFTypeDictionaryKeyCallBacks).cast(),
            (&raw const kCFTypeDictionaryValueCallBacks).cast(),
        ));
        AXIsProcessTrustedWithOptions(options.as_ref().map_or(std::ptr::null(), Cf::as_ptr)) != 0
    }
}

/// Secure keyboard entry is on (a password field has focus, or Terminal's Secure Keyboard Entry):
/// macOS hides key presses from every listener until it is off.
pub fn secure_input() -> bool {
    // SAFETY: plain query.
    unsafe { IsSecureEventInputEnabled() != 0 }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strings_round_trip_through_core_foundation() {
        for text in ["", "plain", "héllo wörld", "emoji 😀 and 中文"] {
            let cf = cf_string(text).expect("CFString");
            assert_eq!(cf.to_string_lossy().as_deref(), Some(text));
            assert_eq!(cf.to_utf16().map(|u| u.len()), Some(text.encode_utf16().count()));
            assert_eq!(cf.to_i64(), None, "a string is not a number");
        }
    }

    #[test]
    fn ax_values_keep_their_layout() {
        let range = CFRange { location: 12, length: 3 };
        // SAFETY: AXValueCreate copies the struct; the result is owned.
        let value = unsafe { Cf::from_owned(AXValueCreate(AX_VALUE_CFRANGE, (&raw const range).cast())) }.unwrap();
        assert_eq!(value.to_ax_value::<CFRange>(AX_VALUE_CFRANGE), Some(range));
        assert_eq!(value.to_ax_value::<CGPoint>(AX_VALUE_CGPOINT), None, "the kind must match");
        assert_eq!(value.to_string_lossy(), None);

        let point = CGPoint { x: 10.5, y: -4.0 };
        // SAFETY: as above.
        let value = unsafe { Cf::from_owned(AXValueCreate(AX_VALUE_CGPOINT, (&raw const point).cast())) }.unwrap();
        assert_eq!(value.to_ax_value::<CGPoint>(AX_VALUE_CGPOINT), Some(point));

        let size = CGSize { width: 640.0, height: 480.0 };
        // SAFETY: as above.
        let value = unsafe { Cf::from_owned(AXValueCreate(AX_VALUE_CGSIZE, (&raw const size).cast())) }.unwrap();
        assert_eq!(value.to_ax_value::<CGSize>(AX_VALUE_CGSIZE), Some(size));
    }

    #[test]
    fn system_queries_answer() {
        // Whatever this machine allows, the queries answer without blocking.
        let (trusted, secure) = (trusted(), secure_input());
        if std::env::var_os("CI").is_some() {
            // Straight to stderr, past the test harness's capture: the log then shows which
            // permission-dependent tests really ran.
            use std::io::Write;
            let _ = writeln!(std::io::stderr(), "macOS test runner: Accessibility trusted = {trusted}, secure input = {secure}");
        }
    }
}
