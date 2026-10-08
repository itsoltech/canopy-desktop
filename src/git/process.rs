//! Bounded subprocess execution for user hooks and signing programs.
use super::Result;
use std::{
    collections::HashMap,
    ffi::OsString,
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

#[derive(Clone)]
pub(super) struct Invocation {
    pub program: PathBuf,
    pub arguments: Vec<OsString>,
}

pub(super) struct Spec {
    pub invocation: Invocation,
    pub fallback: Option<Invocation>,
    pub cwd: PathBuf,
    pub environment: HashMap<String, String>,
    pub environment_os: Vec<(OsString, OsString)>,
    pub input: Vec<u8>,
    pub timeout: Duration,
    pub output_limit: usize,
    pub fail_on_output_limit: bool,
    #[cfg(all(test, windows))]
    pub synchronize_before_read: bool,
}

pub(super) struct Output {
    pub code: Option<i64>,
    pub status: String,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
    pub truncated: bool,
    pub input_written: bool,
    pub failure: Option<String>,
}

impl Output {
    pub fn success(&self) -> bool {
        self.failure.is_none() && self.code == Some(0) && self.input_written
    }
}

pub(super) fn run(spec: Spec, cancel: &AtomicBool) -> Result<Output> {
    if cancel.load(Ordering::Acquire) {
        return Err("Cancelled before execution.".into());
    }
    platform::run(spec, cancel)
}

#[cfg(unix)]
fn collector<R: std::io::Read + Send + 'static>(
    mut stream: R,
    limit: usize,
    overflow: Arc<AtomicBool>,
) -> std::thread::JoinHandle<Vec<u8>> {
    std::thread::spawn(move || {
        let mut output = Vec::new();
        let mut buffer = [0; 4096];
        loop {
            match stream.read(&mut buffer) {
                Ok(0) => break,
                Ok(length) => {
                    let keep = length.min(limit.saturating_sub(output.len()));
                    output.extend_from_slice(&buffer[..keep]);
                    if keep < length {
                        overflow.store(true, Ordering::Release);
                    }
                }
                Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(_) => break,
            }
        }
        output
    })
}

#[cfg(unix)]
mod platform {
    use super::*;
    use std::{
        io::Write,
        os::unix::process::CommandExt,
        process::{Command, Stdio},
    };

    fn command(invocation: &Invocation, spec: &Spec) -> Command {
        let mut command = Command::new(&invocation.program);
        command
            .args(&invocation.arguments)
            .current_dir(&spec.cwd)
            .env_clear()
            .envs(&spec.environment)
            .envs(spec.environment_os.iter().map(|(key, value)| (key, value)))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .process_group(0);
        command
    }

    pub fn run(spec: Spec, cancel: &AtomicBool) -> Result<Output> {
        let mut child = match command(&spec.invocation, &spec).spawn() {
            Ok(child) => child,
            Err(error) if error.raw_os_error() == Some(libc::ENOEXEC) => {
                let fallback = spec
                    .fallback
                    .as_ref()
                    .ok_or_else(|| format!("Could not start: {error}"))?;
                command(fallback, &spec)
                    .spawn()
                    .map_err(|error| format!("Could not start: {error}"))?
            }
            Err(error) => return Err(format!("Could not start: {error}")),
        };
        let input = child.stdin.take().ok_or("Missing stdin")?;
        let writer = if spec.input.is_empty() {
            drop(input);
            None
        } else {
            let body = spec.input;
            Some(std::thread::spawn(move || {
                let mut input = input;
                input.write_all(&body).is_ok()
            }))
        };
        let overflow = Arc::new(AtomicBool::new(false));
        let stdout = collector(
            child.stdout.take().ok_or("Missing stdout")?,
            spec.output_limit,
            overflow.clone(),
        );
        let stderr = collector(
            child.stderr.take().ok_or("Missing stderr")?,
            spec.output_limit,
            overflow.clone(),
        );
        let deadline = Instant::now() + spec.timeout;
        let (status, mut failure) = loop {
            match child.try_wait() {
                Ok(Some(status)) => break (Some(status), None),
                Ok(None) => {}
                Err(_) => break (None, Some("Could not wait for the subprocess.".to_owned())),
            }
            if cancel.load(Ordering::Acquire) {
                break (None, Some("Cancelled.".to_owned()));
            }
            if Instant::now() >= deadline {
                break (None, Some("Timed out.".to_owned()));
            }
            if spec.fail_on_output_limit && overflow.load(Ordering::Acquire) {
                break (None, Some("Output limit exceeded.".to_owned()));
            }
            std::thread::sleep(Duration::from_millis(20));
        };
        // The process is the leader of a new group. Stop descendants before joining readers;
        // otherwise a background child can retain stdout/stderr indefinitely.
        unsafe {
            libc::kill(-(child.id() as i32), libc::SIGKILL);
        }
        if status.is_none() {
            let _ = child.kill();
        }
        let _ = child.wait();
        let input_written = writer
            .map(|writer| writer.join().unwrap_or(false))
            .unwrap_or(true);
        let stdout = stdout.join().unwrap_or_default();
        let stderr = stderr.join().unwrap_or_default();
        if spec.fail_on_output_limit && overflow.load(Ordering::Acquire) {
            failure.get_or_insert_with(|| "Output limit exceeded.".to_owned());
        }
        let code = status.and_then(|status| status.code()).map(i64::from);
        Ok(Output {
            code,
            status: status
                .map(|status| status.to_string())
                .unwrap_or_else(|| "terminated".into()),
            stdout,
            stderr,
            truncated: overflow.load(Ordering::Acquire),
            input_written,
            failure,
        })
    }
}

#[cfg(windows)]
mod platform {
    use super::*;
    use crate::terminal::windows_job::ProcessJob;
    use std::{ffi::c_void, os::windows::ffi::OsStrExt};
    use windows_sys::Win32::{
        Foundation::{
            CloseHandle, ERROR_BROKEN_PIPE, ERROR_IO_PENDING, ERROR_NO_DATA,
            ERROR_OPERATION_ABORTED, ERROR_PIPE_CONNECTED, GENERIC_READ, GENERIC_WRITE,
            GetLastError, HANDLE, INVALID_HANDLE_VALUE, LocalFree, WAIT_OBJECT_0, WAIT_TIMEOUT,
        },
        Security::{
            Authorization::{
                ConvertStringSecurityDescriptorToSecurityDescriptorW, SDDL_REVISION_1,
            },
            SECURITY_ATTRIBUTES,
        },
        Storage::FileSystem::{
            CreateFileW, FILE_FLAG_FIRST_PIPE_INSTANCE, FILE_FLAG_OVERLAPPED, OPEN_EXISTING,
            PIPE_ACCESS_INBOUND, PIPE_ACCESS_OUTBOUND, ReadFile, WriteFile,
        },
        System::{
            IO::{CancelIoEx, GetOverlappedResult, OVERLAPPED},
            Pipes::{
                ConnectNamedPipe, CreateNamedPipeW, PIPE_READMODE_BYTE, PIPE_REJECT_REMOTE_CLIENTS,
                PIPE_TYPE_BYTE, PIPE_WAIT,
            },
            Threading::{
                CREATE_NO_WINDOW, CREATE_UNICODE_ENVIRONMENT, CreateEventW, CreateProcessW,
                DeleteProcThreadAttributeList, EXTENDED_STARTUPINFO_PRESENT, GetExitCodeProcess,
                InitializeProcThreadAttributeList, PROC_THREAD_ATTRIBUTE_HANDLE_LIST,
                PROC_THREAD_ATTRIBUTE_JOB_LIST, PROCESS_INFORMATION, ResetEvent,
                STARTF_USESTDHANDLES, STARTUPINFOEXW, SetEvent, UpdateProcThreadAttribute,
                WaitForMultipleObjects, WaitForSingleObject,
            },
        },
    };

    struct OwnedHandle(HANDLE);
    unsafe impl Send for OwnedHandle {}
    impl Drop for OwnedHandle {
        fn drop(&mut self) {
            if !self.0.is_null() {
                // SAFETY: this wrapper owns one valid kernel handle.
                unsafe { CloseHandle(self.0) };
            }
        }
    }

    struct SecurityDescriptor(*mut c_void);
    impl SecurityDescriptor {
        fn current_user_only() -> std::io::Result<Self> {
            let sddl: Vec<_> = "D:P(A;;GA;;;SY)(A;;GA;;;OW)"
                .encode_utf16()
                .chain(Some(0))
                .collect();
            let mut descriptor = std::ptr::null_mut();
            // SAFETY: SDDL is terminated and output storage is valid.
            if unsafe {
                ConvertStringSecurityDescriptorToSecurityDescriptorW(
                    sddl.as_ptr(),
                    SDDL_REVISION_1,
                    &mut descriptor,
                    std::ptr::null_mut(),
                )
            } == 0
            {
                Err(std::io::Error::last_os_error())
            } else {
                Ok(Self(descriptor))
            }
        }

        fn attributes(&self) -> SECURITY_ATTRIBUTES {
            SECURITY_ATTRIBUTES {
                nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
                lpSecurityDescriptor: self.0,
                bInheritHandle: 0,
            }
        }
    }
    impl Drop for SecurityDescriptor {
        fn drop(&mut self) {
            // SAFETY: the conversion API allocated this descriptor with LocalAlloc.
            unsafe { LocalFree(self.0) };
        }
    }

    fn event(manual_reset: bool, initial: bool) -> std::io::Result<OwnedHandle> {
        // SAFETY: unnamed event with default security and valid boolean flags.
        let handle = unsafe {
            CreateEventW(
                std::ptr::null(),
                manual_reset.into(),
                initial.into(),
                std::ptr::null(),
            )
        };
        if handle.is_null() {
            Err(std::io::Error::last_os_error())
        } else {
            Ok(OwnedHandle(handle))
        }
    }

    enum ParentDirection {
        Read,
        Write,
    }

    fn pipe(direction: ParentDirection) -> std::io::Result<(OwnedHandle, OwnedHandle)> {
        let name: Vec<_> = format!(r"\\.\pipe\canopy-git-{}", uuid::Uuid::new_v4())
            .encode_utf16()
            .chain(Some(0))
            .collect();
        let (server_access, client_access) = match direction {
            ParentDirection::Read => (PIPE_ACCESS_INBOUND, GENERIC_WRITE),
            ParentDirection::Write => (PIPE_ACCESS_OUTBOUND, GENERIC_READ),
        };
        let descriptor = SecurityDescriptor::current_user_only()?;
        let server_security = descriptor.attributes();
        // SAFETY: name and protected security descriptor remain valid for this call.
        let server = unsafe {
            CreateNamedPipeW(
                name.as_ptr(),
                server_access | FILE_FLAG_OVERLAPPED | FILE_FLAG_FIRST_PIPE_INSTANCE,
                PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT | PIPE_REJECT_REMOTE_CLIENTS,
                1,
                4096,
                4096,
                0,
                &server_security,
            )
        };
        if server == INVALID_HANDLE_VALUE {
            return Err(std::io::Error::last_os_error());
        }
        let server = OwnedHandle(server);
        let connected = event(false, false)?;
        let mut overlap: Box<OVERLAPPED> = Box::new(unsafe { std::mem::zeroed() });
        overlap.hEvent = connected.0;
        // SAFETY: server and OVERLAPPED remain valid until connection completion below.
        let connect = unsafe { ConnectNamedPipe(server.0, overlap.as_mut()) };
        let pending = if connect != 0 {
            false
        } else {
            match unsafe { GetLastError() } {
                ERROR_IO_PENDING => true,
                ERROR_PIPE_CONNECTED => false,
                _ => return Err(std::io::Error::last_os_error()),
            }
        };
        let attributes = SECURITY_ATTRIBUTES {
            nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor: std::ptr::null_mut(),
            bInheritHandle: 1,
        };
        // SAFETY: name and inheritable security attributes remain valid for the open.
        let client = unsafe {
            CreateFileW(
                name.as_ptr(),
                client_access,
                0,
                &attributes,
                OPEN_EXISTING,
                0,
                std::ptr::null_mut(),
            )
        };
        if client == INVALID_HANDLE_VALUE {
            let error = std::io::Error::last_os_error();
            if pending {
                // SAFETY: cancel the outstanding connect before overlap leaves scope.
                unsafe {
                    CancelIoEx(server.0, overlap.as_ref());
                }
                if unsafe { WaitForSingleObject(connected.0, 1_000) } != WAIT_OBJECT_0 {
                    std::mem::forget(server);
                    std::mem::forget(connected);
                    Box::leak(overlap);
                    return Err(error);
                }
                let mut transferred = 0;
                unsafe {
                    GetOverlappedResult(server.0, overlap.as_ref(), &mut transferred, 0);
                }
            }
            return Err(error);
        }
        let client = OwnedHandle(client);
        if pending {
            // SAFETY: connected is a valid event for the outstanding connect operation.
            if unsafe { WaitForSingleObject(connected.0, 5_000) } != WAIT_OBJECT_0 {
                drop(client);
                unsafe { CancelIoEx(server.0, overlap.as_ref()) };
                if unsafe { WaitForSingleObject(connected.0, 1_000) } != WAIT_OBJECT_0 {
                    std::mem::forget(server);
                    std::mem::forget(connected);
                    Box::leak(overlap);
                }
                return Err(std::io::Error::new(
                    std::io::ErrorKind::TimedOut,
                    "named pipe connection timed out",
                ));
            }
            let mut transferred = 0;
            // SAFETY: the event signaled completion and all storage is still alive.
            if unsafe { GetOverlappedResult(server.0, overlap.as_ref(), &mut transferred, 0) } == 0
            {
                return Err(std::io::Error::last_os_error());
            }
        }
        Ok((server, client))
    }

    struct Attributes {
        storage: Box<[u8]>,
        job_handles: Box<[HANDLE; 1]>,
        inherited_handles: Box<[HANDLE; 3]>,
        pointer: windows_sys::Win32::System::Threading::LPPROC_THREAD_ATTRIBUTE_LIST,
    }
    impl Attributes {
        fn new(job: &ProcessJob, handles: &[HANDLE; 3]) -> std::io::Result<Self> {
            let mut size = 0;
            // SAFETY: the first call queries the required allocation size.
            unsafe {
                InitializeProcThreadAttributeList(std::ptr::null_mut(), 2, 0, &mut size);
            }
            let mut storage = vec![0u8; size].into_boxed_slice();
            let pointer = storage.as_mut_ptr().cast();
            // SAFETY: storage has the exact queried size and remains owned by Self.
            if unsafe { InitializeProcThreadAttributeList(pointer, 2, 0, &mut size) } == 0 {
                return Err(std::io::Error::last_os_error());
            }
            let job_handles = Box::new([job.raw() as HANDLE]);
            let inherited_handles = Box::new(*handles);
            // SAFETY: the list is initialized and both values live through CreateProcessW.
            let job_ok = unsafe {
                UpdateProcThreadAttribute(
                    pointer,
                    0,
                    PROC_THREAD_ATTRIBUTE_JOB_LIST as usize,
                    job_handles.as_ptr().cast(),
                    std::mem::size_of::<HANDLE>(),
                    std::ptr::null_mut(),
                    std::ptr::null(),
                )
            };
            let handles_ok = unsafe {
                UpdateProcThreadAttribute(
                    pointer,
                    0,
                    PROC_THREAD_ATTRIBUTE_HANDLE_LIST as usize,
                    inherited_handles.as_ptr().cast(),
                    std::mem::size_of_val(inherited_handles.as_ref()),
                    std::ptr::null_mut(),
                    std::ptr::null(),
                )
            };
            if job_ok == 0 || handles_ok == 0 {
                let error = std::io::Error::last_os_error();
                // SAFETY: pointer was initialized successfully above.
                unsafe { DeleteProcThreadAttributeList(pointer) };
                return Err(error);
            }
            Ok(Self {
                storage,
                job_handles,
                inherited_handles,
                pointer,
            })
        }
    }
    impl Drop for Attributes {
        fn drop(&mut self) {
            // Keep the backing allocation observably alive until after list deletion.
            let _ = (
                self.storage.len(),
                self.job_handles.len(),
                self.inherited_handles.len(),
            );
            // SAFETY: pointer was initialized once and is deleted once.
            unsafe { DeleteProcThreadAttributeList(self.pointer) };
        }
    }

    fn wide(value: &std::ffi::OsStr) -> Result<Vec<u16>> {
        let encoded: Vec<_> = value.encode_wide().collect();
        if encoded.contains(&0) {
            Err("Subprocess values cannot contain NUL characters.".into())
        } else {
            Ok(encoded)
        }
    }

    fn quote(value: &std::ffi::OsStr) -> Result<Vec<u16>> {
        let value = wide(value)?;
        let mut output = vec![b'"' as u16];
        let mut slashes = 0;
        for unit in value {
            if unit == b'\\' as u16 {
                slashes += 1;
            } else if unit == b'"' as u16 {
                output.extend(std::iter::repeat_n(b'\\' as u16, slashes * 2 + 1));
                output.push(unit);
                slashes = 0;
            } else {
                output.extend(std::iter::repeat_n(b'\\' as u16, slashes));
                slashes = 0;
                output.push(unit);
            }
        }
        output.extend(std::iter::repeat_n(b'\\' as u16, slashes * 2));
        output.push(b'"' as u16);
        Ok(output)
    }

    fn command_line(invocation: &Invocation) -> Result<Vec<u16>> {
        let mut values = Vec::with_capacity(invocation.arguments.len() + 1);
        values.push(invocation.program.as_os_str());
        values.extend(invocation.arguments.iter().map(OsString::as_os_str));
        let mut output = Vec::new();
        for value in values {
            if !output.is_empty() {
                output.push(b' ' as u16);
            }
            output.extend(quote(value)?);
        }
        output.push(0);
        Ok(output)
    }

    fn environment(
        vars: &HashMap<String, String>,
        os_vars: &[(OsString, OsString)],
    ) -> Result<Vec<u16>> {
        let mut vars: Vec<(OsString, OsString)> = vars
            .iter()
            .map(|(key, value)| (key.into(), value.into()))
            .chain(os_vars.iter().cloned())
            .collect();
        vars.sort_by_key(|(key, _)| key.to_string_lossy().to_lowercase());
        let mut block = Vec::new();
        for (key, value) in vars {
            block.extend(wide(&key)?);
            block.push(b'=' as u16);
            block.extend(wide(&value)?);
            block.push(0);
        }
        block.push(0);
        if block.len() == 1 {
            block.push(0);
        }
        Ok(block)
    }

    enum Pending {
        Bytes(u32),
        Closed,
        Stopped,
        Failed,
        Leaked,
    }

    fn pipe_closed(error: u32) -> bool {
        matches!(error, ERROR_BROKEN_PIPE | ERROR_NO_DATA)
    }

    fn signal_stop(stop: HANDLE) {
        // SAFETY: stop is the live manual-reset event owned by run().
        unsafe { SetEvent(stop) };
        #[cfg(test)]
        {
            let (lock, changed) = test_pause_state();
            let mut state = lock.lock().unwrap();
            state.stop_set = true;
            changed.notify_all();
        }
    }

    #[cfg(test)]
    #[derive(Default)]
    struct TestPause {
        paused: bool,
        released: bool,
        stop_set: bool,
    }

    #[cfg(test)]
    fn test_pause_state() -> &'static (std::sync::Mutex<TestPause>, std::sync::Condvar) {
        static STATE: std::sync::OnceLock<(std::sync::Mutex<TestPause>, std::sync::Condvar)> =
            std::sync::OnceLock::new();
        STATE.get_or_init(|| {
            (
                std::sync::Mutex::new(TestPause::default()),
                std::sync::Condvar::new(),
            )
        })
    }

    #[cfg(test)]
    static PAUSE_READ_AT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    #[cfg(test)]
    static READ_NUMBER: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

    #[cfg(test)]
    pub(super) fn arm_reader_pause(read_number: usize) {
        let (lock, _) = test_pause_state();
        *lock.lock().unwrap() = TestPause::default();
        READ_NUMBER.store(0, Ordering::SeqCst);
        PAUSE_READ_AT.store(read_number, Ordering::SeqCst);
    }

    #[cfg(test)]
    pub(super) fn release_reader_after_stop() -> bool {
        let (lock, changed) = test_pause_state();
        let mut state = lock.lock().unwrap();
        let deadline = Instant::now() + Duration::from_secs(10);
        while !state.paused || !state.stop_set {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                state.released = true;
                changed.notify_all();
                return false;
            }
            let (next, timeout) = changed.wait_timeout(state, remaining).unwrap();
            state = next;
            if timeout.timed_out() && (!state.paused || !state.stop_set) {
                state.released = true;
                changed.notify_all();
                return false;
            }
        }
        state.released = true;
        changed.notify_all();
        true
    }

    #[cfg(test)]
    fn pause_before_read() {
        let number = READ_NUMBER.fetch_add(1, Ordering::SeqCst) + 1;
        if PAUSE_READ_AT.load(Ordering::SeqCst) != number {
            return;
        }
        let (lock, changed) = test_pause_state();
        let mut state = lock.lock().unwrap();
        state.paused = true;
        changed.notify_all();
        while !state.released {
            state = changed.wait(state).unwrap();
        }
        PAUSE_READ_AT.store(0, Ordering::SeqCst);
    }

    fn pending_result(
        handle: HANDLE,
        event: &OwnedHandle,
        stop: HANDLE,
        overlap: &OVERLAPPED,
    ) -> Pending {
        let handles = [event.0, stop];
        // SAFETY: both handles and OVERLAPPED remain valid while this function waits.
        let wait = unsafe { WaitForMultipleObjects(2, handles.as_ptr(), 0, u32::MAX) };
        if wait == WAIT_OBJECT_0 + 1 {
            unsafe { CancelIoEx(handle, overlap) };
            // Cancellation completion must be observed before OVERLAPPED can be released.
            if unsafe { WaitForSingleObject(event.0, 1_000) } != WAIT_OBJECT_0 {
                return Pending::Leaked;
            }
        } else if wait != WAIT_OBJECT_0 {
            return Pending::Failed;
        }
        let mut transferred = 0;
        // SAFETY: the operation event signaled, so the result can be collected without waiting.
        if unsafe { GetOverlappedResult(handle, overlap, &mut transferred, 0) } != 0 {
            Pending::Bytes(transferred)
        } else {
            let error = unsafe { GetLastError() };
            if pipe_closed(error) {
                Pending::Closed
            } else if error == ERROR_OPERATION_ABORTED {
                Pending::Stopped
            } else {
                Pending::Failed
            }
        }
    }

    fn reader(
        pipe: OwnedHandle,
        stop: usize,
        limit: usize,
        overflow: Arc<AtomicBool>,
        io_failed: Arc<AtomicBool>,
        test_synchronization: bool,
    ) -> std::thread::JoinHandle<Vec<u8>> {
        std::thread::spawn(move || {
            let stop = stop as HANDLE;
            let event = match event(false, false) {
                Ok(event) => event,
                Err(_) => {
                    io_failed.store(true, Ordering::Release);
                    return vec![];
                }
            };
            let mut output = Vec::new();
            loop {
                if unsafe { WaitForSingleObject(stop, 0) } == WAIT_OBJECT_0 {
                    break;
                }
                #[cfg(test)]
                if test_synchronization {
                    pause_before_read();
                }
                #[cfg(not(test))]
                let _ = test_synchronization;
                if unsafe { WaitForSingleObject(stop, 0) } == WAIT_OBJECT_0 {
                    break;
                }
                unsafe { ResetEvent(event.0) };
                let mut overlap: Box<OVERLAPPED> = Box::new(unsafe { std::mem::zeroed() });
                overlap.hEvent = event.0;
                let mut buffer = Box::new([0u8; 4096]);
                let mut transferred = 0;
                // SAFETY: pipe, buffer and boxed OVERLAPPED remain stable until completion.
                let started = unsafe {
                    ReadFile(
                        pipe.0,
                        buffer.as_mut_ptr(),
                        buffer.len() as u32,
                        &mut transferred,
                        overlap.as_mut(),
                    )
                };
                let result = if started != 0 {
                    Pending::Bytes(transferred)
                } else {
                    let error = unsafe { GetLastError() };
                    if error == ERROR_IO_PENDING {
                        pending_result(pipe.0, &event, stop, &overlap)
                    } else if pipe_closed(error) {
                        Pending::Closed
                    } else {
                        Pending::Failed
                    }
                };
                let length = match result {
                    Pending::Bytes(0) | Pending::Closed | Pending::Stopped => break,
                    Pending::Bytes(length) => length as usize,
                    Pending::Failed => {
                        io_failed.store(true, Ordering::Release);
                        break;
                    }
                    Pending::Leaked => {
                        io_failed.store(true, Ordering::Release);
                        std::mem::forget(pipe);
                        std::mem::forget(event);
                        Box::leak(overlap);
                        Box::leak(buffer);
                        return output;
                    }
                };
                let keep = length.min(limit.saturating_sub(output.len()));
                output.extend_from_slice(&buffer[..keep]);
                if keep < length {
                    overflow.store(true, Ordering::Release);
                }
            }
            output
        })
    }

    fn writer(
        pipe: OwnedHandle,
        stop: usize,
        input: Vec<u8>,
        io_failed: Arc<AtomicBool>,
    ) -> std::thread::JoinHandle<bool> {
        std::thread::spawn(move || {
            let stop = stop as HANDLE;
            if input.is_empty() {
                return true;
            }
            let event = match event(false, false) {
                Ok(event) => event,
                Err(_) => {
                    io_failed.store(true, Ordering::Release);
                    return false;
                }
            };
            let mut offset = 0;
            while offset < input.len() {
                if unsafe { WaitForSingleObject(stop, 0) } == WAIT_OBJECT_0 {
                    return false;
                }
                unsafe { ResetEvent(event.0) };
                let mut overlap: Box<OVERLAPPED> = Box::new(unsafe { std::mem::zeroed() });
                overlap.hEvent = event.0;
                let length = (input.len() - offset).min(4096);
                let mut transferred = 0;
                // SAFETY: pipe, input and boxed OVERLAPPED remain stable until completion.
                let started = unsafe {
                    WriteFile(
                        pipe.0,
                        input.as_ptr().add(offset),
                        length as u32,
                        &mut transferred,
                        overlap.as_mut(),
                    )
                };
                let result = if started != 0 {
                    Pending::Bytes(transferred)
                } else {
                    let error = unsafe { GetLastError() };
                    if error == ERROR_IO_PENDING {
                        pending_result(pipe.0, &event, stop, &overlap)
                    } else if pipe_closed(error) {
                        Pending::Closed
                    } else {
                        Pending::Failed
                    }
                };
                match result {
                    Pending::Bytes(0) | Pending::Closed | Pending::Stopped => return false,
                    Pending::Bytes(length) => offset += length as usize,
                    Pending::Failed => {
                        io_failed.store(true, Ordering::Release);
                        return false;
                    }
                    Pending::Leaked => {
                        io_failed.store(true, Ordering::Release);
                        std::mem::forget(pipe);
                        std::mem::forget(event);
                        Box::leak(overlap);
                        std::mem::forget(input);
                        return false;
                    }
                }
            }
            true
        })
    }

    pub fn run(spec: Spec, cancel: &AtomicBool) -> Result<Output> {
        let job =
            ProcessJob::new().map_err(|error| format!("Could not create process job: {error}"))?;
        let (stdin_write, stdin_read) =
            pipe(ParentDirection::Write).map_err(|error| error.to_string())?;
        let (stdout_read, stdout_write) =
            pipe(ParentDirection::Read).map_err(|error| error.to_string())?;
        let (stderr_read, stderr_write) =
            pipe(ParentDirection::Read).map_err(|error| error.to_string())?;
        let stop = event(true, false).map_err(|error| error.to_string())?;
        let inherited = [stdin_read.0, stdout_write.0, stderr_write.0];
        let attributes = Attributes::new(&job, &inherited).map_err(|error| error.to_string())?;
        let mut startup: STARTUPINFOEXW = unsafe { std::mem::zeroed() };
        startup.StartupInfo.cb = std::mem::size_of::<STARTUPINFOEXW>() as u32;
        startup.StartupInfo.dwFlags = STARTF_USESTDHANDLES;
        startup.StartupInfo.hStdInput = stdin_read.0;
        startup.StartupInfo.hStdOutput = stdout_write.0;
        startup.StartupInfo.hStdError = stderr_write.0;
        startup.lpAttributeList = attributes.pointer;
        let application = {
            let mut value = wide(spec.invocation.program.as_os_str())?;
            value.push(0);
            value
        };
        let mut command_line = command_line(&spec.invocation)?;
        let environment = environment(&spec.environment, &spec.environment_os)?;
        let cwd = {
            let mut value = wide(spec.cwd.as_os_str())?;
            value.push(0);
            value
        };
        let mut process: PROCESS_INFORMATION = unsafe { std::mem::zeroed() };
        // SAFETY: all buffers and startup attributes remain alive for the synchronous spawn.
        let created = unsafe {
            CreateProcessW(
                application.as_ptr(),
                command_line.as_mut_ptr(),
                std::ptr::null(),
                std::ptr::null(),
                1,
                CREATE_NO_WINDOW | CREATE_UNICODE_ENVIRONMENT | EXTENDED_STARTUPINFO_PRESENT,
                environment.as_ptr().cast(),
                cwd.as_ptr(),
                &startup.StartupInfo as *const _,
                &mut process,
            )
        };
        drop(attributes);
        if created == 0 {
            return Err(format!(
                "Could not start: {}",
                std::io::Error::last_os_error()
            ));
        }
        let process_handle = OwnedHandle(process.hProcess);
        let _thread_handle = OwnedHandle(process.hThread);
        drop(stdin_read);
        drop(stdout_write);
        drop(stderr_write);

        let overflow = Arc::new(AtomicBool::new(false));
        let io_failed = Arc::new(AtomicBool::new(false));
        let writer = writer(stdin_write, stop.0 as usize, spec.input, io_failed.clone());
        #[cfg(all(test, windows))]
        let synchronize_before_read = spec.synchronize_before_read;
        #[cfg(not(all(test, windows)))]
        let synchronize_before_read = false;
        let stdout = reader(
            stdout_read,
            stop.0 as usize,
            spec.output_limit,
            overflow.clone(),
            io_failed.clone(),
            synchronize_before_read,
        );
        let stderr = reader(
            stderr_read,
            stop.0 as usize,
            spec.output_limit,
            overflow.clone(),
            io_failed.clone(),
            false,
        );
        let deadline = Instant::now() + spec.timeout;
        let (code, mut failure) = loop {
            // SAFETY: process_handle remains valid while polling.
            match unsafe { WaitForSingleObject(process_handle.0, 20) } {
                WAIT_OBJECT_0 => {
                    let mut code = 0;
                    // SAFETY: the signaled process handle has a stable exit code.
                    if unsafe { GetExitCodeProcess(process_handle.0, &mut code) } == 0 {
                        break (
                            None,
                            Some("Could not read subprocess exit status.".to_owned()),
                        );
                    }
                    break (Some(i64::from(code)), None);
                }
                WAIT_TIMEOUT => {}
                _ => break (None, Some("Could not wait for the subprocess.".to_owned())),
            }
            if cancel.load(Ordering::Acquire) {
                break (None, Some("Cancelled.".to_owned()));
            }
            if Instant::now() >= deadline {
                break (None, Some("Timed out.".to_owned()));
            }
            if spec.fail_on_output_limit && overflow.load(Ordering::Acquire) {
                break (None, Some("Output limit exceeded.".to_owned()));
            }
            if io_failed.load(Ordering::Acquire) {
                break (None, Some("Subprocess pipe I/O failed.".to_owned()));
            }
        };
        if failure.is_some() {
            signal_stop(stop.0);
        }
        let cleanup = job.terminate_and_wait();
        if cleanup.is_err() {
            failure.get_or_insert_with(|| "Could not stop subprocess descendants.".to_owned());
            signal_stop(stop.0);
        }
        // Closing a job configured with KILL_ON_JOB_CLOSE is the final fallback when explicit
        // termination or accounting verification failed. Do this before joining pipe threads.
        drop(job);
        drop(process_handle);
        let drain_deadline = Instant::now() + Duration::from_secs(1);
        while !(writer.is_finished() && stdout.is_finished() && stderr.is_finished())
            && Instant::now() < drain_deadline
        {
            std::thread::sleep(Duration::from_millis(5));
        }
        if !(writer.is_finished() && stdout.is_finished() && stderr.is_finished()) {
            failure.get_or_insert_with(|| {
                "Subprocess pipe output did not drain before its deadline.".to_owned()
            });
            // Persistent manual-reset stop closes the check-before-I/O race in every worker.
            signal_stop(stop.0);
        }
        let input_written = writer.join().unwrap_or(false);
        let stdout = stdout.join().unwrap_or_default();
        let stderr = stderr.join().unwrap_or_default();
        if io_failed.load(Ordering::Acquire) {
            failure.get_or_insert_with(|| "Subprocess pipe I/O failed.".to_owned());
        }
        if spec.fail_on_output_limit && overflow.load(Ordering::Acquire) {
            failure.get_or_insert_with(|| "Output limit exceeded.".to_owned());
        }
        Ok(Output {
            code,
            status: code
                .map(|code| format!("exit code {code}"))
                .unwrap_or_else(|| "terminated".into()),
            stdout,
            stderr,
            truncated: overflow.load(Ordering::Acquire),
            input_written,
            failure,
        })
    }
}

#[cfg(not(any(unix, windows)))]
mod platform {
    use super::*;
    pub fn run(_: Spec, _: &AtomicBool) -> Result<Output> {
        Err("Subprocess execution is unavailable on this platform.".into())
    }
}

#[cfg(all(test, windows))]
mod windows_tests {
    use super::*;

    fn command_spec(arguments: Vec<OsString>, timeout: Duration) -> (tempfile::TempDir, Spec) {
        let source = PathBuf::from(std::env::var_os("ComSpec").expect("ComSpec"));
        let directory = tempfile::tempdir().unwrap();
        let program = directory
            .path()
            .join("Program Files")
            .join("Test Command.exe");
        std::fs::create_dir_all(program.parent().unwrap()).unwrap();
        std::fs::copy(source, &program).unwrap();
        let spec = Spec {
            invocation: Invocation { program, arguments },
            fallback: None,
            cwd: directory.path().to_owned(),
            environment: std::env::vars().collect(),
            environment_os: vec![],
            input: vec![],
            timeout,
            output_limit: 65_536,
            fail_on_output_limit: false,
            synchronize_before_read: false,
        };
        (directory, spec)
    }

    #[test]
    fn windows_runs_program_with_spaces_and_literal_arguments() {
        let (_directory, spec) = command_spec(
            ["/D", "/S", "/C", "echo literal value"]
                .into_iter()
                .map(OsString::from)
                .collect(),
            Duration::from_secs(5),
        );
        let output = run(spec, &AtomicBool::new(false)).unwrap();
        assert!(output.success());
        assert!(String::from_utf8_lossy(&output.stdout).contains("literal value"));
    }

    #[test]
    fn windows_timeout_stops_job_before_reader_join() {
        let (_directory, spec) = command_spec(
            ["/D", "/S", "/C", "ping -n 30 127.0.0.1 >nul"]
                .into_iter()
                .map(OsString::from)
                .collect(),
            Duration::from_millis(100),
        );
        let started = Instant::now();
        let output = run(spec, &AtomicBool::new(false)).unwrap();
        assert_eq!(output.failure.as_deref(), Some("Timed out."));
        assert!(started.elapsed() < Duration::from_secs(5));
    }

    #[test]
    fn windows_parent_exit_stops_descendant_holding_pipes() {
        let (_directory, spec) = command_spec(
            [
                "/D",
                "/S",
                "/C",
                "start \"\" /B cmd.exe /D /S /C \"ping -n 30 127.0.0.1 ^>nul\" & echo parent-exited",
            ]
            .into_iter()
            .map(OsString::from)
            .collect(),
            Duration::from_secs(5),
        );
        let started = Instant::now();
        let output = run(spec, &AtomicBool::new(false)).unwrap();
        assert!(output.success());
        assert!(String::from_utf8_lossy(&output.stdout).contains("parent-exited"));
        assert!(started.elapsed() < Duration::from_secs(5));
    }

    #[test]
    fn windows_cleanup_failure_stops_reader_paused_before_next_io() {
        let (_directory, mut spec) = command_spec(
            [
                "/D",
                "/S",
                "/C",
                "echo first & ping -n 2 127.0.0.1 >nul & echo second & start \"\" /B cmd.exe /D /S /C \"ping -n 30 127.0.0.1 ^>nul\"",
            ]
            .into_iter()
            .map(OsString::from)
            .collect(),
            Duration::from_secs(5),
        );
        spec.synchronize_before_read = true;
        platform::arm_reader_pause(2);
        crate::terminal::windows_job::force_next_cleanup_failure();
        let release = std::thread::spawn(platform::release_reader_after_stop);
        let started = Instant::now();
        let output = run(spec, &AtomicBool::new(false)).unwrap();
        assert!(release.join().unwrap());
        assert_eq!(
            output.failure.as_deref(),
            Some("Could not stop subprocess descendants.")
        );
        assert!(started.elapsed() < Duration::from_secs(6));
    }

    #[test]
    fn windows_final_drain_timeout_cannot_report_success() {
        let (_directory, mut spec) = command_spec(
            [
                "/D",
                "/S",
                "/C",
                "echo first & ping -n 2 127.0.0.1 >nul & echo unread-second",
            ]
            .into_iter()
            .map(OsString::from)
            .collect(),
            Duration::from_secs(5),
        );
        spec.synchronize_before_read = true;
        platform::arm_reader_pause(2);
        let release = std::thread::spawn(platform::release_reader_after_stop);
        let output = run(spec, &AtomicBool::new(false)).unwrap();
        assert!(release.join().unwrap());
        assert_eq!(output.code, Some(0));
        assert_eq!(
            output.failure.as_deref(),
            Some("Subprocess pipe output did not drain before its deadline.")
        );
        assert!(!output.success());
        assert!(!String::from_utf8_lossy(&output.stdout).contains("unread-second"));
    }
}
