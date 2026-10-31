//! The terminal on Windows. The runtime is a GUI-subsystem executable there (`windows_subsystem`
//! in `main.rs`), so a player starting it from Explorer gets no console window. Such a process has
//! no console of its own either: started from a terminal, its output would go nowhere. [`attach`]
//! routes the standard output and error the parent left unset to the parent's console, and keeps
//! the redirected ones (pipes, files: `kagari debug --window`, `> log.txt`) as they are. Standard
//! input is never attached: the shell keeps reading it.
//!
//! The shell does not wait for a GUI program: in a terminal the prompt comes back at once and the
//! exit status is lost. Scripts that need the status wait explicitly (see `runtime/desktop.md`).

/// Attaches unset standard output and error to the parent's console, if it has one.
#[cfg(windows)]
pub fn attach() {
    use std::fs::OpenOptions;
    use std::os::windows::io::IntoRawHandle;

    use windows_sys::Win32::Foundation::{HANDLE, INVALID_HANDLE_VALUE};
    use windows_sys::Win32::System::Console::{
        ATTACH_PARENT_PROCESS, AttachConsole, GetStdHandle, STD_ERROR_HANDLE, STD_OUTPUT_HANDLE,
        SetStdHandle,
    };

    fn unset(handle: HANDLE) -> bool {
        handle.is_null() || handle == INVALID_HANDLE_VALUE
    }

    // SAFETY: `GetStdHandle` only reads the process's standard handles.
    let saved = [STD_OUTPUT_HANDLE, STD_ERROR_HANDLE].map(|id| (id, unsafe { GetStdHandle(id) }));
    if !saved.iter().any(|&(_, handle)| unset(handle)) {
        return;
    }
    // SAFETY: no pointers involved; fails when the parent has no console (started from Explorer).
    if unsafe { AttachConsole(ATTACH_PARENT_PROCESS) } == 0 {
        return;
    }
    let Ok(console) = OpenOptions::new().read(true).write(true).open("CONOUT$") else {
        return;
    };
    // Stays open for the rest of the process, as a standard handle.
    let console = console.into_raw_handle();
    for (id, handle) in saved {
        // Attaching may replace the standard handles: put back the ones the parent set.
        let handle = if unset(handle) { console } else { handle };
        // SAFETY: `handle` is a standard handle the process started with or the open console
        // output, never closed.
        unsafe { SetStdHandle(id, handle) };
    }
}

/// Other platforms start the runtime with the parent's standard streams.
#[cfg(not(windows))]
pub fn attach() {}
