//! The radio image's updates (spec 032 R8): `UpdateAdapter` on ESP-IDF's OTA, and the probation
//! tick. Every call that touches the flash runs on ONE thread, `ota`, whose 6 KB stack is in
//! internal RAM: the API workers' stacks are in PSRAM and must never touch the flash. A worker
//! sends each call through a channel of one request at a time and waits for the answer; the same
//! thread ticks `Updates::watch` every second (health: the engine's loop is running and the API
//! server is listening). A chunk crosses in one 4096-byte buffer in PSRAM, allocated once by that
//! thread. The logic (header, signature, digest, probation) is the core's, tested on the Mac.
//!
//! The update's handle (`esp_ota_begin` ... `esp_ota_end`) is kept on this thread through ESP-IDF's
//! own functions: `EspOta::initiate_update` returns a value that borrows `EspOta` for as long as
//! the update lasts, which would forbid the slot's state, the confirmation and the rollback (which
//! `EspOta` also gives) during an installation. `EspOta` is used for those three.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender, SyncSender, channel, sync_channel};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use esp_idf_svc::hal::task::thread::ThreadSpawnConfiguration;
use esp_idf_svc::ota::EspOta;
use esp_idf_svc::sys::{
    MALLOC_CAP_8BIT, MALLOC_CAP_SPIRAM, esp, esp_ota_abort, esp_ota_begin, esp_ota_end,
    esp_ota_get_next_update_partition, esp_ota_get_running_partition, esp_ota_get_state_partition,
    esp_ota_handle_t, esp_ota_img_states_t, esp_ota_img_states_t_ESP_OTA_IMG_PENDING_VERIFY,
    esp_ota_set_boot_partition, esp_ota_write, esp_partition_t, esp_restart, heap_caps_malloc,
};
use solite_core::devices::lock;
use solite_core::update::{CHUNK_BYTES, UpdateAdapter, Updates};

use crate::radio_engine::{STOP, STOPPED};
use crate::supervisor;

/// The OTA thread's stack, in internal RAM, for the whole run (spec 032 R8).
pub const STACK_BYTES: usize = 6 * 1024;
/// The probation is ticked this often.
const TICK: Duration = Duration::from_secs(1);
/// The ESP32 restarts this long after the engine was asked to stop, stopped or not.
const STOP_WAIT: Duration = Duration::from_secs(10);
/// The answer of an installation leaves the socket this long before the restart.
const ANSWER_FLUSH: Duration = Duration::from_millis(500);

/// Set while the engine's loop runs (`engine::run` entered, not yet returned).
pub static ENGINE_RUNNING: AtomicBool = AtomicBool::new(false);
/// Set once the API server has started.
pub static API_LISTENING: AtomicBool = AtomicBool::new(false);

static UPDATES: OnceLock<Updates> = OnceLock::new();
static LINK: OnceLock<Link> = OnceLock::new();

/// The updates of this build, once `start` has run.
pub fn updates() -> Option<&'static Updates> {
    UPDATES.get()
}

/// An adapter for a worker: every call goes to the OTA thread. `None` when it did not start.
pub fn adapter() -> Option<RemoteAdapter> {
    LINK.get().map(|_| RemoteAdapter)
}

/// What a worker asks of the OTA thread.
enum Call {
    Capacity,
    Begin(u32),
    /// The first bytes of the shared chunk buffer.
    Write(usize),
    Abort,
    Finish,
    Pending,
    Confirm,
    RollBack,
}

/// The answer: a number (capacity, a flag as 0 or 1) or a message.
type Answer = Result<u64, String>;

/// The worker's side: the requests (one at a time, under the mutex) and the shared buffer.
struct Link {
    calls: Mutex<(SyncSender<Call>, Receiver<Answer>)>,
    chunk: &'static Mutex<&'static mut [u8]>,
}

/// The adapter a worker uses: it holds nothing, the OTA thread does.
pub struct RemoteAdapter;

fn gone() -> String {
    "the update thread is not running".to_string()
}

impl RemoteAdapter {
    fn call(&self, call: Call) -> Answer {
        let link = LINK.get().ok_or_else(gone)?;
        let pair = lock(&link.calls);
        pair.0.send(call).map_err(|_| gone())?;
        pair.1.recv().map_err(|_| gone())?
    }
}

impl UpdateAdapter for RemoteAdapter {
    fn capacity(&self) -> u64 {
        self.call(Call::Capacity).unwrap_or(0)
    }

    fn begin(&mut self, length: u32) -> Result<(), String> {
        self.call(Call::Begin(length)).map(|_| ())
    }

    fn write(&mut self, chunk: &[u8]) -> Result<(), String> {
        let link = LINK.get().ok_or_else(gone)?;
        for piece in chunk.chunks(CHUNK_BYTES) {
            lock(link.chunk)[..piece.len()].copy_from_slice(piece);
            self.call(Call::Write(piece.len()))?;
        }
        Ok(())
    }

    fn abort(&mut self) {
        let _ = self.call(Call::Abort);
    }

    fn finish(&mut self) -> Result<(), String> {
        self.call(Call::Finish).map(|_| ())
    }

    fn pending(&self) -> bool {
        self.call(Call::Pending).is_ok_and(|flag| flag == 1)
    }

    fn confirm(&mut self) -> Result<(), String> {
        self.call(Call::Confirm).map(|_| ())
    }

    fn roll_back(&mut self) -> Result<(), String> {
        self.call(Call::RollBack).map(|_| ())
    }
}

/// The update being written: ESP-IDF's handle.
struct Writing(esp_ota_handle_t);

/// The thread's side: the flash calls themselves.
struct Flash {
    ota: EspOta,
    writing: Option<Writing>,
}

fn message(code: esp_idf_svc::sys::EspError) -> String {
    code.to_string()
}

impl Flash {
    #[allow(unsafe_code)]
    fn capacity(&self) -> u64 {
        // SAFETY: asks for the next update partition (null: after the running one); the result is
        // a static table entry or null.
        unsafe { esp_ota_get_next_update_partition(std::ptr::null()).as_ref() }
            .map_or(0, |partition| u64::from(partition.size))
    }

    #[allow(unsafe_code)]
    fn begin(&mut self, length: u32) -> Result<(), String> {
        self.abort();
        // SAFETY: as `capacity`.
        let partition = unsafe { esp_ota_get_next_update_partition(std::ptr::null()) };
        if partition.is_null() {
            return Err("no update partition".to_string());
        }
        let mut handle: esp_ota_handle_t = 0;
        // SAFETY: `partition` is a live table entry; ESP-IDF writes the handle; it erases `length`
        // bytes (rounded up to a sector) of that partition.
        esp!(unsafe { esp_ota_begin(partition, length as usize, &mut handle) }).map_err(message)?;
        self.writing = Some(Writing(handle));
        Ok(())
    }

    #[allow(unsafe_code)]
    fn write(&mut self, bytes: &[u8]) -> Result<(), String> {
        let Writing(handle) = self.writing.as_ref().ok_or("no update is open")?;
        // SAFETY: the handle is open; `bytes` is valid for its length during the call.
        esp!(unsafe { esp_ota_write(*handle, bytes.as_ptr().cast(), bytes.len()) }).map_err(message)
    }

    #[allow(unsafe_code)]
    fn abort(&mut self) {
        if let Some(Writing(handle)) = self.writing.take() {
            // SAFETY: the handle is open and taken out of `self`: aborted once.
            unsafe { esp_ota_abort(handle) };
        }
    }

    /// Closes the slot (ESP-IDF checks the image) and makes it the next boot.
    #[allow(unsafe_code)]
    fn finish(&mut self) -> Result<(), String> {
        let Writing(handle) = self.writing.take().ok_or("no update is open")?;
        // SAFETY: the handle is open; `esp_ota_end` releases it whatever it answers.
        esp!(unsafe { esp_ota_end(handle) }).map_err(message)?;
        // SAFETY: as `capacity`; the partition is the one just written.
        let partition = unsafe { esp_ota_get_next_update_partition(std::ptr::null()) };
        if partition.is_null() {
            return Err("no update partition".to_string());
        }
        // SAFETY: a live table entry whose image `esp_ota_end` just verified.
        esp!(unsafe { esp_ota_set_boot_partition(partition) }).map_err(message)
    }

    /// True when the running image waits for its confirmation (state `PENDING_VERIFY`: only a
    /// bootloader with the app rollback sets it). The state `NEW` of an image booted by a
    /// bootloader without it is not pending: nothing would roll back.
    #[allow(unsafe_code)]
    fn pending(&self) -> bool {
        let mut state: esp_ota_img_states_t = 0;
        // SAFETY: the running partition is a static table entry; ESP-IDF writes the state.
        let code = unsafe {
            let running: *const esp_partition_t = esp_ota_get_running_partition();
            if running.is_null() {
                return false;
            }
            esp_ota_get_state_partition(running, &mut state)
        };
        code == 0 && state == esp_ota_img_states_t_ESP_OTA_IMG_PENDING_VERIFY
    }

    fn confirm(&mut self) -> Result<(), String> {
        self.ota.mark_running_slot_valid().map_err(message)
    }

    /// The running image is marked bad and the box restarts into the previous one: it returns only
    /// when that was not possible.
    fn roll_back(&mut self) -> Result<(), String> {
        // The box restarts into the previous image: the reason is written first (spec 036).
        supervisor::asked_restart();
        let error = message(self.ota.mark_running_slot_invalid_and_reboot());
        // No restart happened: `asked` must not hide a later crash from the boot-loop guard.
        supervisor::cancel_asked();
        Err(error)
    }

    fn run(&mut self, call: Call, chunk: &Mutex<&'static mut [u8]>) -> Answer {
        match call {
            Call::Capacity => Ok(self.capacity()),
            Call::Begin(length) => self.begin(length).map(|()| 0),
            Call::Write(count) => {
                let chunk = lock(chunk);
                // The worker never asks for more than the buffer holds.
                let bytes = chunk.get(..count).ok_or("chunk too long")?;
                self.write(bytes).map(|()| 0)
            }
            Call::Abort => {
                self.abort();
                Ok(0)
            }
            Call::Finish => self.finish().map(|()| 0),
            Call::Pending => Ok(u64::from(self.pending())),
            Call::Confirm => self.confirm().map(|()| 0),
            Call::RollBack => self.roll_back().map(|()| 0),
        }
    }
}

impl UpdateAdapter for Flash {
    fn capacity(&self) -> u64 {
        Flash::capacity(self)
    }
    fn begin(&mut self, length: u32) -> Result<(), String> {
        Flash::begin(self, length)
    }
    fn write(&mut self, chunk: &[u8]) -> Result<(), String> {
        Flash::write(self, chunk)
    }
    fn abort(&mut self) {
        Flash::abort(self);
    }
    fn finish(&mut self) -> Result<(), String> {
        Flash::finish(self)
    }
    fn pending(&self) -> bool {
        Flash::pending(self)
    }
    fn confirm(&mut self) -> Result<(), String> {
        Flash::confirm(self)
    }
    fn roll_back(&mut self) -> Result<(), String> {
        Flash::roll_back(self)
    }
}

/// A buffer of `CHUNK_BYTES` in PSRAM, kept for the run (as `body_buffer` of `http_server.rs`).
#[allow(unsafe_code)]
fn chunk_buffer() -> Result<&'static mut [u8], String> {
    // SAFETY: asks ESP-IDF for a block of PSRAM; checked for null below.
    let pointer = unsafe { heap_caps_malloc(CHUNK_BYTES, MALLOC_CAP_SPIRAM | MALLOC_CAP_8BIT) };
    if pointer.is_null() {
        return Err("update: no PSRAM for the chunk buffer".to_string());
    }
    // SAFETY: the block is `CHUNK_BYTES` long, never freed (kept for the run) and owned by this one
    // slice; it is zeroed before use.
    let buffer = unsafe { std::slice::from_raw_parts_mut(pointer.cast::<u8>(), CHUNK_BYTES) };
    buffer.fill(0);
    Ok(buffer)
}

fn healthy() -> bool {
    ENGINE_RUNNING.load(Ordering::SeqCst) && API_LISTENING.load(Ordering::SeqCst)
}

/// 0 for `ota_0`, 1 for `ota_1` (the host's `boot_slot`); 0 when ESP-IDF cannot say.
#[allow(unsafe_code)]
fn running_slot() -> u8 {
    // SAFETY: the running partition is a static table entry; only its subtype is read.
    unsafe {
        let running = esp_ota_get_running_partition();
        if running.is_null() {
            return 0;
        }
        let subtype = (*running).subtype;
        subtype.saturating_sub(
            esp_idf_svc::sys::esp_partition_subtype_t_ESP_PARTITION_SUBTYPE_APP_OTA_0,
        ) as u8
    }
}

/// The thread's body: serves the calls and ticks the probation every second.
fn thread_body(
    updates: &'static Updates,
    calls: &Receiver<Call>,
    answers: &Sender<Answer>,
    ready: &Sender<Result<&'static Mutex<&'static mut [u8]>, String>>,
) {
    let setup = || -> Result<(Flash, &'static mut [u8]), String> {
        let ota = EspOta::new().map_err(|e| format!("update: {e}"))?;
        Ok((Flash { ota, writing: None }, chunk_buffer()?))
    };
    let (mut flash, buffer) = match setup() {
        Ok(parts) => parts,
        Err(e) => {
            let _ = ready.send(Err(e));
            return;
        }
    };
    let chunk: &'static Mutex<&'static mut [u8]> = Box::leak(Box::new(Mutex::new(buffer)));
    if ready.send(Ok(chunk)).is_err() {
        return;
    }
    if flash.pending() {
        log::info!("update: slot {} on probation", running_slot());
    }
    let start = Instant::now();
    let mut ticked = Instant::now();
    loop {
        match calls.recv_timeout(TICK) {
            Ok(call) => {
                let answer = flash.run(call, chunk);
                if answers.send(answer).is_err() {
                    return;
                }
            }
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => return,
        }
        if ticked.elapsed() >= TICK {
            ticked = Instant::now();
            let now = u64::try_from(start.elapsed().as_millis()).unwrap_or(u64::MAX);
            updates.watch(&mut flash, now, healthy());
            // The supervisor's clock, and the safe mode's retry (spec 036).
            supervisor::tick();
        }
    }
}

/// Builds the updates from the build's key (`SOLITE_UPDATE_PUBLIC_KEY`, spec 032 R2), logs the key
/// line and starts the OTA thread. Without the thread, updates answer 503.
pub fn start() -> Result<(), String> {
    let (updates, line) = Updates::from_key_text(option_env!("SOLITE_UPDATE_PUBLIC_KEY"));
    log::info!("{line}");
    let updates: &'static Updates = UPDATES.get_or_init(|| updates);
    let (call_sender, calls) = sync_channel::<Call>(1);
    let (answers, answer_receiver) = channel::<Answer>();
    let (ready, ready_receiver) = channel();
    // The default configuration: no core, a stack in internal RAM.
    ThreadSpawnConfiguration {
        name: Some(c"ota"),
        stack_size: STACK_BYTES,
        ..Default::default()
    }
    .set()
    .map_err(|e| format!("update: cannot configure its thread: {e}"))?;
    let spawned = std::thread::Builder::new()
        .name("ota".into())
        .stack_size(STACK_BYTES)
        .spawn(move || thread_body(updates, &calls, &answers, &ready));
    // Threads started later get the default configuration again.
    let _ = ThreadSpawnConfiguration::default().set();
    spawned.map_err(|e| format!("update: cannot start its thread: {e}"))?;
    let chunk = ready_receiver
        .recv_timeout(Duration::from_secs(5))
        .map_err(|_| "update: its thread did not start".to_string())??;
    LINK.set(Link {
        calls: Mutex::new((call_sender, answer_receiver)),
        chunk,
    })
    .map_err(|_| "update: already started".to_string())
}

/// After a successful installation or restore, once its answer is out: asks the engine to stop (its
/// files are saved), then restarts the ESP32 (into the new image, or onto the restored files).
#[allow(unsafe_code)]
pub fn restart_after_update() -> ! {
    std::thread::sleep(ANSWER_FLUSH);
    STOP.store(true, Ordering::SeqCst);
    let asked = Instant::now();
    while !STOPPED.load(Ordering::SeqCst) && asked.elapsed() < STOP_WAIT {
        std::thread::sleep(Duration::from_millis(50));
    }
    if !STOPPED.load(Ordering::SeqCst) {
        log::warn!("update: the engine did not stop within 10 s, restarting anyway");
    }
    log::info!("restarting");
    supervisor::asked_restart();
    // SAFETY: takes no argument, restarts the chip, never returns.
    unsafe { esp_restart() }
}
