use super::Event;
use serde::Deserialize;
use std::{
    collections::HashMap,
    io::Read,
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
};

const MAX_ENVELOPE_BYTES: usize = 1_048_576;
const EVENT_QUEUE_CAPACITY: usize = 128;

#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Endpoint {
    UnixSocket(PathBuf),
    WindowsPipe(String),
}

impl Endpoint {
    /// Stable identifier passed to the hook helper alongside the address.
    pub fn kind(&self) -> &'static str {
        match self {
            Self::UnixSocket(_) => "unix",
            Self::WindowsPipe(_) => "windows-pipe",
        }
    }

    /// Platform address without assuming that every endpoint is a filesystem path.
    pub fn address(&self) -> String {
        match self {
            Self::UnixSocket(path) => path.to_string_lossy().into_owned(),
            Self::WindowsPipe(name) => name.clone(),
        }
    }

    fn from_env() -> Result<Self, String> {
        let kind = std::env::var("CANOPY_AGENT_ENDPOINT_KIND").ok();
        let address = std::env::var_os("CANOPY_AGENT_ENDPOINT")
            .or_else(|| std::env::var_os("CANOPY_AGENT_SOCKET"))
            .ok_or_else(|| "missing agent relay endpoint".to_owned())?;
        match kind.as_deref() {
            Some("windows-pipe") => Ok(Self::WindowsPipe(address.to_string_lossy().into_owned())),
            Some("unix") | None => Ok(Self::UnixSocket(address.into())),
            Some(other) => Err(format!("unsupported agent relay endpoint: {other}")),
        }
    }
}

#[derive(Clone, Debug)]
#[non_exhaustive]
pub struct Registration {
    pub run: String,
    pub token: String,
    pub endpoint: Endpoint,
}

#[derive(Clone)]
pub struct Relay {
    endpoint: Endpoint,
    signal: transport::StopSignal,
    tokens: Arc<Mutex<HashMap<String, String>>>,
    stop: Arc<AtomicBool>,
    join: Arc<Mutex<Option<std::thread::JoinHandle<()>>>>,
    pub events: async_channel::Receiver<Event>,
}

impl Relay {
    pub fn start() -> Result<Self, String> {
        let tokens = Arc::new(Mutex::new(HashMap::<String, String>::new()));
        let stop = Arc::new(AtomicBool::new(false));
        let (tx, events) = async_channel::bounded(EVENT_QUEUE_CAPACITY);
        let (endpoint, signal, join) = transport::start(tokens.clone(), stop.clone(), tx)?;
        Ok(Self {
            endpoint,
            signal,
            tokens,
            stop,
            join: Arc::new(Mutex::new(Some(join))),
            events,
        })
    }

    #[cfg(all(test, windows))]
    fn start_with_recreate_gate(gate: Arc<transport::RecreateGate>) -> Result<Self, String> {
        let tokens = Arc::new(Mutex::new(HashMap::<String, String>::new()));
        let stop = Arc::new(AtomicBool::new(false));
        let (tx, events) = async_channel::bounded(EVENT_QUEUE_CAPACITY);
        let (endpoint, signal, join) =
            transport::start_with_recreate_gate(tokens.clone(), stop.clone(), tx, gate)?;
        Ok(Self {
            endpoint,
            signal,
            tokens,
            stop,
            join: Arc::new(Mutex::new(Some(join))),
            events,
        })
    }

    pub fn register(&self) -> Registration {
        let run = uuid::Uuid::new_v4().to_string();
        let token = format!(
            "{}{}",
            uuid::Uuid::new_v4().simple(),
            uuid::Uuid::new_v4().simple()
        );
        self.tokens
            .lock()
            .unwrap()
            .insert(run.clone(), token.clone());
        Registration {
            run,
            token,
            endpoint: self.endpoint.clone(),
        }
    }

    pub fn remove(&self, run: &str) {
        self.tokens.lock().unwrap().remove(run);
    }

    pub fn shutdown(&self) {
        self.stop.store(true, Ordering::Release);
        self.signal.wake(&self.endpoint);
        if let Some(join) = self.join.lock().unwrap().take() {
            let _ = join.join();
        }
    }
}

/// Invoked by providers as a stable hook command. Never writes stdout/context.
pub fn forward() {
    let _ = (|| -> Result<(), String> {
        let mut bytes = Vec::new();
        std::io::stdin()
            .take((MAX_ENVELOPE_BYTES + 1) as u64)
            .read_to_end(&mut bytes)
            .map_err(|error| error.to_string())?;
        if bytes.len() > MAX_ENVELOPE_BYTES {
            return Ok(());
        }
        let event: serde_json::Value =
            serde_json::from_slice(&bytes).map_err(|error| error.to_string())?;
        let registration = Registration {
            run: std::env::var("CANOPY_AGENT_RUN").map_err(|error| error.to_string())?,
            token: std::env::var("CANOPY_AGENT_TOKEN").map_err(|error| error.to_string())?,
            endpoint: Endpoint::from_env()?,
        };
        forward_event(&registration, &event)
    })();
}

/// Sends one provider event through the registered local transport and waits for acceptance.
pub fn forward_event(registration: &Registration, event: &serde_json::Value) -> Result<(), String> {
    let bytes = serde_json::to_vec(&serde_json::json!({
        "run": registration.run,
        "token": registration.token,
        "event": event,
    }))
    .map_err(|error| error.to_string())?;
    if bytes.len() > MAX_ENVELOPE_BYTES {
        return Err("agent relay envelope exceeds 1 MiB".into());
    }
    transport::send(&registration.endpoint, &bytes)
}

#[derive(Deserialize)]
struct Envelope {
    run: String,
    token: String,
    event: serde_json::Value,
}

fn accept_envelope(
    bytes: &[u8],
    tokens: &Mutex<HashMap<String, String>>,
    events: &async_channel::Sender<Event>,
) -> bool {
    if bytes.len() > MAX_ENVELOPE_BYTES {
        return false;
    }
    let Ok(envelope) = serde_json::from_slice::<Envelope>(bytes) else {
        return false;
    };
    if tokens
        .lock()
        .unwrap()
        .get(&envelope.run)
        .is_none_or(|expected| !same_token(expected, &envelope.token))
    {
        return false;
    }
    events
        .try_send(Event::from_json(envelope.run, &envelope.event))
        .is_ok()
}

fn same_token(expected: &str, actual: &str) -> bool {
    if expected.len() != actual.len() {
        return false;
    }
    expected
        .bytes()
        .zip(actual.bytes())
        .fold(0, |diff, (a, b)| diff | (a ^ b))
        == 0
}

impl Drop for Relay {
    fn drop(&mut self) {
        if Arc::strong_count(&self.join) == 1 {
            self.stop.store(true, Ordering::Release);
            self.signal.wake(&self.endpoint);
            // Normal shutdown joins off the UI thread. A dropped startup still wakes
            // the worker so its private endpoint is eventually released.
        }
    }
}

#[cfg(unix)]
mod transport {
    use super::*;
    use std::{
        fs::Permissions,
        io::{Read, Write},
        net::Shutdown,
        os::unix::{
            fs::PermissionsExt,
            net::{UnixListener, UnixStream},
        },
        time::{Duration, Instant},
    };

    const IO_TIMEOUT: Duration = Duration::from_millis(250);
    const MESSAGE_DEADLINE: Duration = Duration::from_secs(2);

    #[derive(Clone, Copy)]
    pub struct StopSignal;

    impl StopSignal {
        pub fn wake(&self, endpoint: &Endpoint) {
            if let Endpoint::UnixSocket(socket) = endpoint
                && let Ok(stream) = UnixStream::connect(socket)
            {
                let _ = stream.shutdown(Shutdown::Both);
            }
        }
    }

    pub fn start(
        tokens: Arc<Mutex<HashMap<String, String>>>,
        stop: Arc<AtomicBool>,
        events: async_channel::Sender<Event>,
    ) -> Result<(Endpoint, StopSignal, std::thread::JoinHandle<()>), String> {
        // /tmp avoids macOS's small sockaddr_un limit for a long user temp path.
        let dir = tempfile::Builder::new()
            .prefix("canopy-hooks-")
            .tempdir_in("/tmp")
            .map_err(|error| error.to_string())?;
        std::fs::set_permissions(dir.path(), Permissions::from_mode(0o700))
            .map_err(|error| error.to_string())?;
        let socket = dir.path().join("events.sock");
        let listener = UnixListener::bind(&socket).map_err(|error| error.to_string())?;
        let endpoint = Endpoint::UnixSocket(socket);
        let join = std::thread::Builder::new()
            .name("canopy-agent-hooks".into())
            .spawn(move || {
                let _directory = dir;
                for connection in listener.incoming() {
                    if stop.load(Ordering::Acquire) {
                        break;
                    }
                    let Ok(mut stream) = connection else {
                        break;
                    };
                    if let Some(bytes) = read_frame(&mut stream, &stop)
                        && accept_envelope(&bytes, &tokens, &events)
                    {
                        let _ = stream.set_write_timeout(Some(Duration::from_millis(100)));
                        let _ = stream.write_all(b"1");
                    }
                }
            })
            .map_err(|error| error.to_string())?;
        Ok((endpoint, StopSignal, join))
    }

    fn read_frame(stream: &mut UnixStream, stop: &AtomicBool) -> Option<Vec<u8>> {
        stream.set_read_timeout(Some(IO_TIMEOUT)).ok()?;
        let deadline = Instant::now() + MESSAGE_DEADLINE;
        let mut length = [0; 4];
        read_exact_until(stream, &mut length, stop, deadline)?;
        let length = u32::from_be_bytes(length) as usize;
        if length > MAX_ENVELOPE_BYTES {
            return None;
        }
        let mut bytes = vec![0; length];
        read_exact_until(stream, &mut bytes, stop, deadline)?;
        Some(bytes)
    }

    fn read_exact_until(
        stream: &mut UnixStream,
        mut bytes: &mut [u8],
        stop: &AtomicBool,
        deadline: Instant,
    ) -> Option<()> {
        while !bytes.is_empty() && Instant::now() <= deadline && !stop.load(Ordering::Acquire) {
            match stream.read(bytes) {
                Ok(0) => return None,
                Ok(read) => bytes = &mut bytes[read..],
                Err(error)
                    if matches!(
                        error.kind(),
                        std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                    ) => {}
                Err(_) => return None,
            }
        }
        bytes.is_empty().then_some(())
    }

    pub fn send(endpoint: &Endpoint, bytes: &[u8]) -> Result<(), String> {
        let Endpoint::UnixSocket(socket) = endpoint else {
            return Err("agent relay endpoint is not a Unix socket".into());
        };
        let mut stream = UnixStream::connect(socket).map_err(|error| error.to_string())?;
        stream
            .set_write_timeout(Some(Duration::from_millis(200)))
            .map_err(|error| error.to_string())?;
        stream
            .write_all(&(bytes.len() as u32).to_be_bytes())
            .and_then(|_| stream.write_all(bytes))
            .map_err(|error| error.to_string())?;
        stream
            .set_read_timeout(Some(IO_TIMEOUT))
            .map_err(|error| error.to_string())?;
        let mut ack = [0];
        stream
            .read_exact(&mut ack)
            .map_err(|error| error.to_string())?;
        if ack == [b'1'] {
            Ok(())
        } else {
            Err("agent relay rejected the event".into())
        }
    }
}

#[cfg(windows)]
mod transport {
    use super::*;
    use std::{
        ffi::c_void,
        ptr,
        time::{Duration, Instant},
    };
    use windows_sys::Win32::{
        Foundation::{
            CloseHandle, ERROR_FILE_NOT_FOUND, ERROR_IO_PENDING, ERROR_MORE_DATA, ERROR_PIPE_BUSY,
            ERROR_PIPE_CONNECTED, GENERIC_READ, GENERIC_WRITE, HANDLE, INVALID_HANDLE_VALUE,
            LocalFree, WAIT_OBJECT_0, WAIT_TIMEOUT,
        },
        Security::{
            Authorization::ConvertStringSecurityDescriptorToSecurityDescriptorW,
            SECURITY_ATTRIBUTES,
        },
        Storage::FileSystem::{
            CreateFileW, FILE_FLAG_FIRST_PIPE_INSTANCE, FILE_FLAG_OVERLAPPED, OPEN_EXISTING,
            PIPE_ACCESS_DUPLEX, ReadFile, WriteFile,
        },
        System::IO::{CancelIoEx, GetOverlappedResult, OVERLAPPED},
        System::Pipes::{
            ConnectNamedPipe, CreateNamedPipeW, DisconnectNamedPipe, PIPE_READMODE_MESSAGE,
            PIPE_REJECT_REMOTE_CLIENTS, PIPE_TYPE_MESSAGE, SetNamedPipeHandleState, WaitNamedPipeW,
        },
        System::Threading::{
            CreateEventW, INFINITE, SetEvent, WaitForMultipleObjects, WaitForSingleObject,
        },
    };

    const SDDL_REVISION_1: u32 = 1;
    const PIPE_BUFFER_BYTES: u32 = (MAX_ENVELOPE_BYTES + 1) as u32;
    const CLIENT_DEADLINE: Duration = Duration::from_millis(1_500);
    const SERVER_DEADLINE: Duration = Duration::from_secs(2);
    const ACK: u8 = b'1';
    const ACK_RECEIVED: u8 = b'2';

    #[derive(Clone)]
    pub struct StopSignal(Arc<OwnedEvent>);

    impl StopSignal {
        fn new() -> Result<Self, String> {
            Ok(Self(Arc::new(OwnedEvent::new()?)))
        }

        fn handle(&self) -> HANDLE {
            self.0.0
        }

        pub fn wake(&self, _: &Endpoint) {
            // SAFETY: the manual-reset event remains alive in this shared owner.
            unsafe {
                SetEvent(self.handle());
            }
        }
    }

    #[cfg(test)]
    pub(super) struct RecreateGate {
        reached: std::sync::Barrier,
        release: std::sync::Barrier,
    }

    #[cfg(test)]
    impl RecreateGate {
        fn new() -> Arc<Self> {
            Arc::new(Self {
                reached: std::sync::Barrier::new(2),
                release: std::sync::Barrier::new(2),
            })
        }

        fn worker_wait(&self) {
            self.reached.wait();
            self.release.wait();
        }

        fn wait_until_recreate(&self) {
            self.reached.wait();
        }

        fn release(&self) {
            self.release.wait();
        }
    }

    #[cfg(not(test))]
    struct RecreateGate;

    pub fn start(
        tokens: Arc<Mutex<HashMap<String, String>>>,
        stop: Arc<AtomicBool>,
        events: async_channel::Sender<Event>,
    ) -> Result<(Endpoint, StopSignal, std::thread::JoinHandle<()>), String> {
        start_inner(tokens, stop, events, None)
    }

    #[cfg(test)]
    pub(super) fn start_with_recreate_gate(
        tokens: Arc<Mutex<HashMap<String, String>>>,
        stop: Arc<AtomicBool>,
        events: async_channel::Sender<Event>,
        gate: Arc<RecreateGate>,
    ) -> Result<(Endpoint, StopSignal, std::thread::JoinHandle<()>), String> {
        start_inner(tokens, stop, events, Some(gate))
    }

    fn start_inner(
        tokens: Arc<Mutex<HashMap<String, String>>>,
        stop: Arc<AtomicBool>,
        events: async_channel::Sender<Event>,
        #[cfg_attr(not(test), allow(unused_variables))] gate: Option<Arc<RecreateGate>>,
    ) -> Result<(Endpoint, StopSignal, std::thread::JoinHandle<()>), String> {
        let name = format!(
            r"\\.\pipe\canopy-hooks-{}-{}",
            std::process::id(),
            uuid::Uuid::new_v4().simple()
        );
        let endpoint = Endpoint::WindowsPipe(name.clone());
        let signal = StopSignal::new()?;
        let worker_signal = signal.clone();
        let security = SecurityDescriptor::current_user_only()?;
        let attributes = security.attributes();
        let handle = create_pipe(&name, &attributes)?;
        let join = std::thread::Builder::new()
            .name("canopy-agent-hooks".into())
            .spawn(move || {
                let security = security;
                let mut current = Some(handle);
                while let Some(pipe) = current.take() {
                    if wait_for_client(pipe.0, worker_signal.handle())
                        && !stop.load(Ordering::Acquire)
                    {
                        let deadline = Instant::now() + SERVER_DEADLINE;
                        if let Some(bytes) = read_message(pipe.0, worker_signal.handle(), deadline)
                            && !stop.load(Ordering::Acquire)
                            && accept_envelope(&bytes, &tokens, &events)
                            && write_message(pipe.0, &[ACK], Some(worker_signal.handle()), deadline)
                        {
                            let _ = read_receipt(pipe.0, worker_signal.handle(), deadline);
                        }
                        // SAFETY: pipe is owned by this worker and no I/O remains outstanding.
                        unsafe {
                            DisconnectNamedPipe(pipe.0);
                        }
                    }
                    drop(pipe);
                    if stop.load(Ordering::Acquire) {
                        break;
                    }
                    #[cfg(test)]
                    if let Some(gate) = &gate {
                        gate.worker_wait();
                    }
                    let attributes = security.attributes();
                    current = create_pipe(&name, &attributes).ok();
                }
            })
            .map_err(|error| error.to_string())?;
        Ok((endpoint, signal, join))
    }

    fn create_pipe(name: &str, security: &SECURITY_ATTRIBUTES) -> Result<OwnedPipe, String> {
        let name = wide(name);
        // SAFETY: all pointers remain valid for the synchronous call. The returned handle has
        // one owner and is closed by the relay worker.
        let handle = unsafe {
            CreateNamedPipeW(
                name.as_ptr(),
                PIPE_ACCESS_DUPLEX | FILE_FLAG_FIRST_PIPE_INSTANCE | FILE_FLAG_OVERLAPPED,
                PIPE_TYPE_MESSAGE | PIPE_READMODE_MESSAGE | PIPE_REJECT_REMOTE_CLIENTS,
                1,
                PIPE_BUFFER_BYTES,
                PIPE_BUFFER_BYTES,
                0,
                security,
            )
        };
        if handle == INVALID_HANDLE_VALUE {
            Err(std::io::Error::last_os_error().to_string())
        } else {
            Ok(OwnedPipe(handle))
        }
    }

    fn wait_for_client(pipe: HANDLE, stop: HANDLE) -> bool {
        let Ok(mut operation) = PendingIo::new() else {
            return false;
        };
        // SAFETY: pipe and the OVERLAPPED event stay alive until immediate completion or until
        // wait_pending cancels and drains the operation.
        if unsafe { ConnectNamedPipe(pipe, &mut operation.overlapped) } != 0 {
            return true;
        }
        match std::io::Error::last_os_error().raw_os_error() {
            Some(code) if code == ERROR_PIPE_CONNECTED as i32 => true,
            Some(code) if code == ERROR_IO_PENDING as i32 => matches!(
                wait_pending(pipe, &operation, Some(stop), None),
                IoCompletion::Complete(_)
            ),
            _ => false,
        }
    }

    fn read_message(pipe: HANDLE, stop: HANDLE, deadline: Instant) -> Option<Vec<u8>> {
        let mut bytes = Vec::new();
        let mut chunk = [0; 8192];
        loop {
            let completion = read_once(pipe, &mut chunk, Some(stop), deadline);
            let (read, complete) = match completion {
                IoCompletion::Complete(read) => (read, true),
                IoCompletion::MoreData(read) => (read, false),
                _ => return None,
            };
            if bytes.len() + read as usize > MAX_ENVELOPE_BYTES {
                return None;
            }
            bytes.extend_from_slice(&chunk[..read as usize]);
            if complete {
                return Some(bytes);
            }
        }
    }

    fn read_receipt(pipe: HANDLE, stop: HANDLE, deadline: Instant) -> bool {
        let mut receipt = [0u8; 1];
        matches!(
            read_once(pipe, &mut receipt, Some(stop), deadline),
            IoCompletion::Complete(1)
        ) && receipt == [ACK_RECEIVED]
    }

    fn read_once(
        pipe: HANDLE,
        bytes: &mut [u8],
        stop: Option<HANDLE>,
        deadline: Instant,
    ) -> IoCompletion {
        let Ok(mut operation) = PendingIo::new() else {
            return IoCompletion::Failed;
        };
        let mut read = 0;
        // SAFETY: the buffer, handle and OVERLAPPED storage remain alive until completion or
        // until wait_pending has canceled and drained the operation.
        let ok = unsafe {
            ReadFile(
                pipe,
                bytes.as_mut_ptr(),
                bytes.len() as u32,
                &mut read,
                &mut operation.overlapped,
            )
        };
        if ok != 0 {
            return IoCompletion::Complete(read);
        }
        match std::io::Error::last_os_error().raw_os_error() {
            Some(code) if code == ERROR_MORE_DATA as i32 => IoCompletion::MoreData(read),
            Some(code) if code == ERROR_IO_PENDING as i32 => {
                wait_pending(pipe, &operation, stop, Some(deadline))
            }
            _ => IoCompletion::Failed,
        }
    }

    fn write_message(pipe: HANDLE, bytes: &[u8], stop: Option<HANDLE>, deadline: Instant) -> bool {
        let Ok(mut operation) = PendingIo::new() else {
            return false;
        };
        let mut written = 0;
        // SAFETY: the buffer, handle and OVERLAPPED storage remain alive until completion or
        // until wait_pending has canceled and drained the operation.
        let ok = unsafe {
            WriteFile(
                pipe,
                bytes.as_ptr(),
                bytes.len() as u32,
                &mut written,
                &mut operation.overlapped,
            )
        };
        let completion = if ok != 0 {
            IoCompletion::Complete(written)
        } else if std::io::Error::last_os_error().raw_os_error() == Some(ERROR_IO_PENDING as i32) {
            wait_pending(pipe, &operation, stop, Some(deadline))
        } else {
            IoCompletion::Failed
        };
        matches!(completion, IoCompletion::Complete(count) if count == bytes.len() as u32)
    }

    fn wait_pending(
        pipe: HANDLE,
        operation: &PendingIo,
        stop: Option<HANDLE>,
        deadline: Option<Instant>,
    ) -> IoCompletion {
        let (handles, count, operation_ix) = if let Some(stop) = stop {
            ([stop, operation.event.0], 2, 1)
        } else {
            ([operation.event.0, ptr::null_mut()], 1, 0)
        };
        let timeout = deadline.map_or(INFINITE, remaining_millis);
        // SAFETY: every handle remains valid for the wait and count matches the initialized
        // prefix of the array.
        let result = unsafe { WaitForMultipleObjects(count, handles.as_ptr(), 0, timeout) };
        if result == WAIT_OBJECT_0 + operation_ix {
            return overlapped_result(pipe, operation);
        }
        cancel_and_drain(pipe, operation);
        if result == WAIT_TIMEOUT {
            IoCompletion::TimedOut
        } else if count == 2 && result == WAIT_OBJECT_0 {
            IoCompletion::Stopped
        } else {
            IoCompletion::Failed
        }
    }

    fn overlapped_result(pipe: HANDLE, operation: &PendingIo) -> IoCompletion {
        let mut transferred = 0;
        // SAFETY: the operation event has signaled and the OVERLAPPED storage remains valid.
        let ok = unsafe { GetOverlappedResult(pipe, &operation.overlapped, &mut transferred, 0) };
        if ok != 0 {
            IoCompletion::Complete(transferred)
        } else if std::io::Error::last_os_error().raw_os_error() == Some(ERROR_MORE_DATA as i32) {
            IoCompletion::MoreData(transferred)
        } else {
            IoCompletion::Failed
        }
    }

    fn cancel_and_drain(pipe: HANDLE, operation: &PendingIo) {
        // SAFETY: cancellation targets this exact operation. Waiting for its event and querying
        // the terminal result keep the OVERLAPPED storage alive until the kernel is finished.
        unsafe {
            CancelIoEx(pipe, &operation.overlapped);
            WaitForSingleObject(operation.event.0, INFINITE);
            let mut transferred = 0;
            GetOverlappedResult(pipe, &operation.overlapped, &mut transferred, 0);
        }
    }

    fn remaining_millis(deadline: Instant) -> u32 {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            0
        } else {
            remaining
                .as_millis()
                .saturating_add(1)
                .min(u32::MAX as u128) as u32
        }
    }

    fn open_client(name: &[u16], deadline: Instant) -> Result<OwnedPipe, String> {
        loop {
            // SAFETY: name is NUL-terminated. The returned handle has one RAII owner.
            let handle = unsafe {
                CreateFileW(
                    name.as_ptr(),
                    GENERIC_READ | GENERIC_WRITE,
                    0,
                    ptr::null(),
                    OPEN_EXISTING,
                    FILE_FLAG_OVERLAPPED,
                    ptr::null_mut(),
                )
            };
            if handle != INVALID_HANDLE_VALUE {
                let pipe = OwnedPipe(handle);
                let mode = PIPE_READMODE_MESSAGE;
                // SAFETY: the connected duplex handle and mode pointer are valid.
                if unsafe { SetNamedPipeHandleState(pipe.0, &mode, ptr::null(), ptr::null()) } == 0
                {
                    return Err(std::io::Error::last_os_error().to_string());
                }
                return Ok(pipe);
            }
            let error = std::io::Error::last_os_error();
            match error.raw_os_error() {
                Some(code) if code == ERROR_PIPE_BUSY as i32 => {}
                Some(code) if code == ERROR_FILE_NOT_FOUND as i32 => {
                    if Instant::now() >= deadline {
                        return Err("agent relay connection timed out".into());
                    }
                    std::thread::sleep(Duration::from_millis(1));
                    continue;
                }
                _ => return Err(error.to_string()),
            }
            let timeout = remaining_millis(deadline);
            if timeout == 0 {
                return Err("agent relay connection timed out".into());
            }
            // SAFETY: name remains valid for this bounded availability wait.
            if unsafe { WaitNamedPipeW(name.as_ptr(), timeout) } == 0 && Instant::now() >= deadline
            {
                return Err("agent relay connection timed out".into());
            }
        }
    }

    pub fn send(endpoint: &Endpoint, bytes: &[u8]) -> Result<(), String> {
        let Endpoint::WindowsPipe(name) = endpoint else {
            return Err("agent relay endpoint is not a Windows named pipe".into());
        };
        let deadline = Instant::now() + CLIENT_DEADLINE;
        let pipe = open_client(&wide(name), deadline)?;
        if !write_message(pipe.0, bytes, None, deadline) {
            return Err("agent relay write timed out or failed".into());
        }
        let mut ack = [0u8; 1];
        if !matches!(
            read_once(pipe.0, &mut ack, None, deadline),
            IoCompletion::Complete(1)
        ) || ack != [ACK]
        {
            return Err("agent relay acknowledgement timed out or failed".into());
        }
        // The event is already accepted once ACK is observed. This bounded receipt prevents the
        // server from disconnecting while the acknowledgement is still unread.
        let _ = write_message(pipe.0, &[ACK_RECEIVED], None, deadline);
        Ok(())
    }

    struct SecurityDescriptor(*mut c_void);

    struct OwnedPipe(HANDLE);

    struct OwnedEvent(HANDLE);

    struct PendingIo {
        event: OwnedEvent,
        overlapped: OVERLAPPED,
    }

    enum IoCompletion {
        Complete(u32),
        MoreData(u32),
        Stopped,
        TimedOut,
        Failed,
    }

    impl OwnedEvent {
        fn new() -> Result<Self, String> {
            // SAFETY: the event is unnamed, non-inheritable and manually reset. Its handle is
            // released by Drop after every associated OVERLAPPED operation has completed.
            let handle = unsafe { CreateEventW(ptr::null(), 1, 0, ptr::null()) };
            if handle.is_null() {
                Err(std::io::Error::last_os_error().to_string())
            } else {
                Ok(Self(handle))
            }
        }
    }

    impl PendingIo {
        fn new() -> Result<Self, String> {
            let event = OwnedEvent::new()?;
            // SAFETY: an all-zero OVERLAPPED is the documented initialization, after which the
            // owned event handle is assigned before the structure reaches a Win32 call.
            let mut overlapped: OVERLAPPED = unsafe { std::mem::zeroed() };
            overlapped.hEvent = event.0;
            Ok(Self { event, overlapped })
        }
    }

    impl Drop for OwnedEvent {
        fn drop(&mut self) {
            // SAFETY: this is the sole owner of a valid event handle.
            unsafe {
                CloseHandle(self.0);
            }
        }
    }

    // Win32 event handles are safe to signal and wait on from multiple threads.
    unsafe impl Send for OwnedEvent {}
    unsafe impl Sync for OwnedEvent {}

    // The handle has one owner and all I/O happens on the relay worker.
    unsafe impl Send for OwnedPipe {}

    #[cfg(test)]
    impl OwnedPipe {
        fn handle(&self) -> HANDLE {
            self.0
        }
    }

    impl Drop for OwnedPipe {
        fn drop(&mut self) {
            // SAFETY: this is the sole owner of a valid named-pipe handle.
            unsafe {
                CloseHandle(self.0);
            }
        }
    }

    // The descriptor is created before spawning the worker and then owned only by that worker.
    unsafe impl Send for SecurityDescriptor {}

    impl SecurityDescriptor {
        fn current_user_only() -> Result<Self, String> {
            // Protected DACL: LocalSystem and the object owner receive full access. The owner is
            // the current user creating this local pipe; inherited access is disabled.
            let sddl = wide("D:P(A;;GA;;;SY)(A;;GA;;;OW)");
            let mut descriptor = ptr::null_mut();
            // SAFETY: sddl is NUL-terminated and descriptor points to writable storage.
            let ok = unsafe {
                ConvertStringSecurityDescriptorToSecurityDescriptorW(
                    sddl.as_ptr(),
                    SDDL_REVISION_1,
                    &mut descriptor,
                    ptr::null_mut(),
                )
            };
            if ok == 0 {
                Err(std::io::Error::last_os_error().to_string())
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
            // SAFETY: the descriptor was allocated by LocalAlloc inside the conversion API.
            unsafe {
                LocalFree(self.0);
            }
        }
    }

    fn wide(value: &str) -> Vec<u16> {
        value.encode_utf16().chain(Some(0)).collect()
    }

    #[cfg(test)]
    mod windows_tests {
        use super::*;
        use std::sync::mpsc;

        #[test]
        fn stop_event_survives_the_pipe_recreation_race() {
            let gate = RecreateGate::new();
            let relay = Relay::start_with_recreate_gate(gate.clone()).unwrap();
            let registration = relay.register();
            forward_event(
                &registration,
                &serde_json::json!({"hook_event_name": "SessionStart"}),
            )
            .unwrap();

            // The worker has checked stop and no pipe exists while it waits at this gate.
            gate.wait_until_recreate();
            relay.stop.store(true, Ordering::Release);
            relay.signal.wake(&relay.endpoint);
            gate.release();

            let started = Instant::now();
            relay.shutdown();
            assert!(started.elapsed() < Duration::from_secs(1));
        }

        #[test]
        fn server_keeps_the_ack_until_a_delayed_client_reads_it() {
            let relay = Relay::start().unwrap();
            let registration = relay.register();
            let Endpoint::WindowsPipe(name) = &registration.endpoint else {
                panic!("Windows relay did not return a named-pipe endpoint");
            };
            let bytes = serde_json::to_vec(&serde_json::json!({
                "run": registration.run,
                "token": registration.token,
                "event": {"hook_event_name": "SessionStart"}
            }))
            .unwrap();
            let deadline = Instant::now() + CLIENT_DEADLINE;
            let pipe = open_client(&wide(name), deadline).unwrap();
            assert!(write_message(pipe.0, &bytes, None, deadline));

            std::thread::sleep(Duration::from_millis(200));
            let mut ack = [0u8; 1];
            assert!(matches!(
                read_once(pipe.0, &mut ack, None, deadline),
                IoCompletion::Complete(1)
            ));
            assert_eq!(ack, [ACK]);
            assert!(write_message(pipe.0, &[ACK_RECEIVED], None, deadline));
            assert_eq!(relay.events.try_recv().unwrap().run, registration.run);
            relay.shutdown();
        }

        #[test]
        fn client_deadline_cancels_a_server_that_never_acknowledges() {
            let name = format!(
                r"\\.\pipe\canopy-hooks-stall-{}",
                uuid::Uuid::new_v4().simple()
            );
            let endpoint = Endpoint::WindowsPipe(name.clone());
            let security = SecurityDescriptor::current_user_only().unwrap();
            let attributes = security.attributes();
            let pipe = create_pipe(&name, &attributes).unwrap();
            let (accepted_tx, accepted_rx) = mpsc::channel();
            let (release_tx, release_rx) = mpsc::channel();
            let server = std::thread::spawn(move || {
                let stop = StopSignal::new().unwrap();
                assert!(wait_for_client(pipe.handle(), stop.handle()));
                assert!(
                    read_message(
                        pipe.handle(),
                        stop.handle(),
                        Instant::now() + SERVER_DEADLINE
                    )
                    .is_some()
                );
                accepted_tx.send(()).unwrap();
                release_rx.recv_timeout(Duration::from_secs(4)).unwrap();
                // SAFETY: this test server owns the connected pipe and has no pending I/O.
                unsafe {
                    DisconnectNamedPipe(pipe.handle());
                }
            });

            let (result_tx, result_rx) = mpsc::channel();
            let client = std::thread::spawn(move || {
                let started = Instant::now();
                let result = send(&endpoint, b"accepted-but-unacknowledged");
                result_tx.send((started.elapsed(), result)).unwrap();
            });
            accepted_rx.recv_timeout(Duration::from_secs(1)).unwrap();
            let (elapsed, result) = result_rx.recv_timeout(Duration::from_secs(4)).unwrap();
            assert!(result.is_err());
            assert!(elapsed >= Duration::from_secs(1));
            assert!(elapsed < Duration::from_secs(3));
            release_tx.send(()).unwrap();
            client.join().unwrap();
            server.join().unwrap();
        }
    }
}

#[cfg(not(any(unix, windows)))]
mod transport {
    use super::*;

    #[derive(Clone, Copy)]
    pub struct StopSignal;

    impl StopSignal {
        pub fn wake(&self, _: &Endpoint) {}
    }

    pub fn start(
        _: Arc<Mutex<HashMap<String, String>>>,
        _: Arc<AtomicBool>,
        _: async_channel::Sender<Event>,
    ) -> Result<(Endpoint, StopSignal, std::thread::JoinHandle<()>), String> {
        Err("agent relay transport is unavailable on this platform".into())
    }

    pub fn send(_: &Endpoint, _: &[u8]) -> Result<(), String> {
        Err("agent relay transport is unavailable on this platform".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn envelope_requires_an_exact_registered_token_and_queue_capacity() {
        let tokens = Mutex::new(HashMap::from([("run".into(), "secret".into())]));
        let (tx, rx) = async_channel::bounded(1);
        let envelope = |token: &str| {
            serde_json::to_vec(&serde_json::json!({
                "run": "run",
                "token": token,
                "event": {"hook_event_name": "SessionStart"}
            }))
            .unwrap()
        };
        assert!(!accept_envelope(&envelope("forged"), &tokens, &tx));
        assert!(accept_envelope(&envelope("secret"), &tokens, &tx));
        assert!(!accept_envelope(&envelope("secret"), &tokens, &tx));
        assert_eq!(rx.try_recv().unwrap().run, "run");
    }

    #[test]
    fn oversized_or_malformed_envelopes_are_rejected() {
        let tokens = Mutex::new(HashMap::new());
        let (tx, _) = async_channel::bounded(1);
        assert!(!accept_envelope(
            &vec![0; MAX_ENVELOPE_BYTES + 1],
            &tokens,
            &tx
        ));
        assert!(!accept_envelope(b"not-json", &tokens, &tx));
    }
}
