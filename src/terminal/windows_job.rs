use std::{
    io, ptr,
    time::{Duration, Instant},
};
use windows_sys::Win32::{
    Foundation::{CloseHandle, HANDLE},
    System::JobObjects::{
        CreateJobObjectW, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
        JOBOBJECT_BASIC_ACCOUNTING_INFORMATION, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
        JobObjectBasicAccountingInformation, JobObjectExtendedLimitInformation,
        QueryInformationJobObject, SetInformationJobObject, TerminateJobObject,
    },
};

const CLEANUP_TIMEOUT: Duration = Duration::from_secs(5);
#[cfg(test)]
static FORCE_NEXT_CLEANUP_FAILURE: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

#[cfg(test)]
pub(crate) fn force_next_cleanup_failure() {
    FORCE_NEXT_CLEANUP_FAILURE.store(true, std::sync::atomic::Ordering::SeqCst);
}

/// Owns one terminal process tree. The vendored ConPTY launcher assigns this job
/// atomically in `CreateProcessW` before the child can create descendants.
pub(crate) struct ProcessJob(HANDLE);

impl ProcessJob {
    pub(crate) fn new() -> io::Result<Self> {
        // SAFETY: both pointers are null for an unnamed, non-inheritable job.
        let handle = unsafe { CreateJobObjectW(ptr::null(), ptr::null()) };
        if handle.is_null() {
            return Err(io::Error::last_os_error());
        }
        let job = Self(handle);
        // SAFETY: the all-zero structure is valid before selecting the one limit flag below.
        let mut limits: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = unsafe { std::mem::zeroed() };
        limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        // SAFETY: the job and structure remain valid for the synchronous configuration call.
        let configured = unsafe {
            SetInformationJobObject(
                job.0,
                JobObjectExtendedLimitInformation,
                (&limits as *const JOBOBJECT_EXTENDED_LIMIT_INFORMATION).cast(),
                std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
            )
        };
        if configured == 0 {
            Err(io::Error::last_os_error())
        } else {
            Ok(job)
        }
    }

    pub(crate) fn raw(&self) -> usize {
        self.0 as usize
    }

    pub(crate) fn request_termination(&self) -> io::Result<()> {
        // SAFETY: the owned job handle remains valid. Repeated termination is harmless.
        if unsafe { TerminateJobObject(self.0, 1) } == 0 {
            Err(io::Error::last_os_error())
        } else {
            Ok(())
        }
    }

    pub(crate) fn terminate_and_wait(&self) -> io::Result<()> {
        #[cfg(test)]
        if FORCE_NEXT_CLEANUP_FAILURE.swap(false, std::sync::atomic::Ordering::SeqCst) {
            return Err(io::Error::other("forced terminal cleanup failure"));
        }
        self.request_termination()?;
        let deadline = Instant::now() + CLEANUP_TIMEOUT;
        loop {
            // Job-object signaling reports job time limits, not process exhaustion. Query the
            // accounting state directly until every assigned process has left the job.
            // SAFETY: the job and output structure remain valid for this synchronous query.
            let mut accounting: JOBOBJECT_BASIC_ACCOUNTING_INFORMATION =
                unsafe { std::mem::zeroed() };
            let queried = unsafe {
                QueryInformationJobObject(
                    self.0,
                    JobObjectBasicAccountingInformation,
                    (&mut accounting as *mut JOBOBJECT_BASIC_ACCOUNTING_INFORMATION).cast(),
                    std::mem::size_of::<JOBOBJECT_BASIC_ACCOUNTING_INFORMATION>() as u32,
                    ptr::null_mut(),
                )
            };
            if queried == 0 {
                return Err(io::Error::last_os_error());
            }
            if accounting.ActiveProcesses == 0 {
                return Ok(());
            }
            if Instant::now() >= deadline {
                return Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    "process job did not become empty within 5 seconds",
                ));
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }
}

// The job is an owned kernel handle and may be moved to the supervisor thread.
unsafe impl Send for ProcessJob {}
unsafe impl Sync for ProcessJob {}

impl Drop for ProcessJob {
    fn drop(&mut self) {
        // SAFETY: this type has sole ownership of a valid job handle.
        unsafe {
            CloseHandle(self.0);
        }
    }
}
