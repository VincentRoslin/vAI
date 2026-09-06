//! Windows Job Object wrapper — every spawned child (the `llama-server` model
//! server, a stdio worker) is assigned to a job with
//! `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`, so it cannot outlive this process even
//! on a hard crash (ADR-0013, orphan-cleanup gate).
//!
//! On non-Windows the type is a no-op so the crate still builds.

#[cfg(windows)]
pub use windows_impl::JobObject;

#[cfg(not(windows))]
pub use stub_impl::JobObject;

/// Stop Windows popping a console window for a spawned console app
/// (`llama-server`, the Python workers, `nvidia-smi`). No-op on other platforms.
/// Call before `spawn()` / `output()`.
#[cfg(windows)]
pub fn hide_console(cmd: &mut tokio::process::Command) {
    // CREATE_NO_WINDOW
    cmd.creation_flags(0x0800_0000);
}

/// See [`hide_console`]. Non-Windows: nothing to suppress.
#[cfg(not(windows))]
#[allow(clippy::missing_const_for_fn)]
pub fn hide_console(_cmd: &mut tokio::process::Command) {}

/// [`hide_console`] for a blocking [`std::process::Command`].
#[cfg(windows)]
pub fn hide_console_std(cmd: &mut std::process::Command) {
    use std::os::windows::process::CommandExt;
    cmd.creation_flags(0x0800_0000);
}

/// See [`hide_console_std`].
#[cfg(not(windows))]
#[allow(clippy::missing_const_for_fn)]
pub fn hide_console_std(_cmd: &mut std::process::Command) {}

#[cfg(windows)]
mod windows_impl {
    use std::io;

    use windows::Win32::Foundation::{CloseHandle, HANDLE};
    use windows::Win32::System::JobObjects::{
        AssignProcessToJobObject, CreateJobObjectW, JobObjectExtendedLimitInformation,
        SetInformationJobObject, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
        JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
    };

    /// Owns a job handle. Dropping it closes the handle; with kill-on-close set,
    /// that terminates every process still assigned to the job.
    pub struct JobObject(HANDLE);

    // The handle is only touched from the owning thread's `assign` / `Drop`.
    unsafe impl Send for JobObject {}
    unsafe impl Sync for JobObject {}

    impl JobObject {
        /// Create a job configured to kill its members when the handle closes.
        ///
        /// # Errors
        /// Propagates the Win32 failure from `CreateJobObjectW` /
        /// `SetInformationJobObject`.
        pub fn new() -> io::Result<Self> {
            unsafe {
                let handle = CreateJobObjectW(None, None).map_err(|e| win_err(&e))?;
                let mut info = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
                info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
                SetInformationJobObject(
                    handle,
                    JobObjectExtendedLimitInformation,
                    std::ptr::addr_of!(info).cast(),
                    u32::try_from(std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>())
                        .unwrap_or(u32::MAX),
                )
                .map_err(|e| win_err(&e))?;
                Ok(Self(handle))
            }
        }

        /// Assign an already-spawned child (by its raw process handle) to the job.
        ///
        /// # Errors
        /// Propagates the Win32 failure from `AssignProcessToJobObject`.
        pub fn assign(&self, raw_process_handle: isize) -> io::Result<()> {
            unsafe {
                AssignProcessToJobObject(self.0, HANDLE(raw_process_handle as *mut _))
                    .map_err(|e| win_err(&e))
            }
        }
    }

    impl Drop for JobObject {
        fn drop(&mut self) {
            unsafe {
                let _ = CloseHandle(self.0);
            }
        }
    }

    fn win_err(e: &windows::core::Error) -> io::Error {
        io::Error::other(e.to_string())
    }
}

#[cfg(not(windows))]
mod stub_impl {
    use std::io;

    /// No-op on non-Windows. Child cleanup relies on `Child::kill` alone there.
    pub struct JobObject;

    impl JobObject {
        pub fn new() -> io::Result<Self> {
            Ok(Self)
        }

        #[allow(clippy::unused_self)]
        pub fn assign(&self, _raw_process_handle: isize) -> io::Result<()> {
            Ok(())
        }
    }
}
