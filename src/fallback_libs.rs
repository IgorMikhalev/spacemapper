// SPDX-License-Identifier: GPL-3.0-or-later
//! AppImage only: make sure the window system libraries can be found.
//!
//! The window code opens libX11, libxkbcommon and friends at runtime by name.
//! Some minimal systems lack a few of them, and the program would then quit
//! without a window. The AppImage carries copies in usr/lib/fallback. For
//! each library the system's copy is loaded first; the bundled copy is only
//! loaded when the system has none. Later lookups by name then find the
//! already loaded library.

use std::ffi::CString;
use std::path::Path;

/// In load order: dependencies before the libraries that need them.
/// packaging/build-packages.sh reads this list to bundle the files.
const LIBS: &[&str] = &[
    "libmd.so.0",
    "libbsd.so.0",
    "libXdmcp.so.6",
    "libXau.so.6",
    "libxcb.so.1",
    "libX11.so.6",
    "libXext.so.6",
    "libXrender.so.1",
    "libXfixes.so.3",
    "libXcursor.so.1",
    "libXi.so.6",
    "libX11-xcb.so.1",
    "libxcb-xkb.so.1",
    "libxkbcommon.so.0",
    "libxkbcommon-x11.so.0",
];

fn open(name: &str) -> bool {
    let Ok(c) = CString::new(name) else { return false };
    !unsafe { libc::dlopen(c.as_ptr(), libc::RTLD_NOW | libc::RTLD_GLOBAL) }.is_null()
}

pub fn load() {
    let Some(appdir) = std::env::var_os("APPDIR") else { return };
    let dir = Path::new(&appdir).join("usr/lib/fallback");
    if !dir.is_dir() {
        return;
    }
    for name in LIBS {
        if !open(name) {
            // ignore failures: the window code reports what is still missing
            open(&dir.join(name).to_string_lossy());
        }
    }
}
