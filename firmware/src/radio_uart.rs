//! The radio's serial link (feature `radio`, spec 021): UART1 to the EFR32, and the two lines that
//! reset it. This is the only file that names GPIO12 and GPIO15 (strapping pins of the ESP32: the
//! flash voltage and the silence of the ROM's boot messages) and GPIO13 and GPIO17 (the UART): a
//! test of `crates/host` checks it. The core describes the pins by role and the reset sequence;
//! this file maps the roles to pins and runs the steps.

use std::ffi::CStr;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use esp_idf_svc::sys::{
    ESP_INTR_FLAG_IRAM, configTICK_RATE_HZ, esp_err_to_name, gpio_config, gpio_config_t,
    gpio_int_type_t_GPIO_INTR_DISABLE, gpio_mode_t_GPIO_MODE_OUTPUT,
    gpio_pulldown_t_GPIO_PULLDOWN_DISABLE, gpio_pullup_t_GPIO_PULLUP_DISABLE, gpio_set_level,
    soc_periph_uart_clk_src_legacy_t_UART_SCLK_DEFAULT, uart_config_t, uart_config_t__bindgen_ty_1,
    uart_driver_install, uart_flush_input, uart_get_buffered_data_len,
    uart_hw_flowcontrol_t_UART_HW_FLOWCTRL_DISABLE, uart_param_config,
    uart_parity_t_UART_PARITY_DISABLE, uart_port_t, uart_read_bytes, uart_set_baudrate,
    uart_set_pin, uart_stop_bits_t_UART_STOP_BITS_1, uart_word_length_t_UART_DATA_8_BITS,
    uart_write_bytes,
};
use solite_core::radio::serial::{PinStep, RADIO_BAUD, RadioPin, SerialStats, reset_sequence};
use solite_core::radio::{RadioTransport, TransportError};

/// The radio's bootloader line (active high): also the ESP32's flash-voltage strapping pin.
const PIN_BOOTLOADER: i32 = 12;
/// The radio's reset line (active low): also the strapping pin that silences the ROM's messages
/// when held low at reset.
const PIN_RESET: i32 = 15;
/// The ESP32 receives on this pin.
const PIN_RX: i32 = 13;
/// The ESP32 transmits on this pin.
const PIN_TX: i32 = 17;
/// The UART the radio is on.
const UART: uart_port_t = 1;
/// The receive ring, in internal RAM: more than the radio's ASH window (7 frames of at most about
/// 450 stuffed bytes) before it waits for an acknowledgement.
pub const RX_RING_BYTES: usize = 4096;
/// The transmit ring, in internal RAM.
pub const TX_RING_BYTES: usize = 1024;
/// Milliseconds of one FreeRTOS tick, from the tick rate ESP-IDF is configured with (at least 1).
const TICK_MS: u64 = {
    let ms = 1000 / configTICK_RATE_HZ as u64;
    if ms == 0 { 1 } else { ms }
};

/// The name of an ESP-IDF error code.
#[allow(unsafe_code)]
fn error_name(code: i32) -> String {
    // SAFETY: `esp_err_to_name` returns a pointer to a static, NUL-terminated string (or to
    // "UNKNOWN ERROR"), valid for the whole run.
    unsafe { CStr::from_ptr(esp_err_to_name(code)) }
        .to_string_lossy()
        .into_owned()
}

fn check(code: i32) -> Result<(), String> {
    if code == 0 {
        Ok(())
    } else {
        Err(error_name(code))
    }
}

fn pin_of(pin: RadioPin) -> i32 {
    match pin {
        RadioPin::Reset => PIN_RESET,
        RadioPin::Bootloader => PIN_BOOTLOADER,
    }
}

/// Switches a pin to a push-pull output at `high`: the level is written first, so the pin never
/// shows another level when it starts driving.
#[allow(unsafe_code)]
fn drive(pin: i32, high: bool) -> Result<(), String> {
    // SAFETY: `gpio_set_level` takes a pin number and a level; the pin is valid on the ESP32. It
    // only writes the output register.
    check(unsafe { gpio_set_level(pin, u32::from(high)) })?;
    let config = gpio_config_t {
        pin_bit_mask: 1u64 << pin,
        mode: gpio_mode_t_GPIO_MODE_OUTPUT,
        pull_up_en: gpio_pullup_t_GPIO_PULLUP_DISABLE,
        pull_down_en: gpio_pulldown_t_GPIO_PULLDOWN_DISABLE,
        intr_type: gpio_int_type_t_GPIO_INTR_DISABLE,
    };
    // SAFETY: `config` is valid for the call, which only reads it; it selects the GPIO function of
    // the pad and enables the output, with the level written above.
    check(unsafe { gpio_config(&config) })
}

/// Runs the core's hardware reset of the radio. Both lines are first driven inactive, each level
/// written before the pin becomes an output (the bootloader line low, then the reset line high:
/// the reset line never goes low before the bootloader line is low), then the steps run.
pub fn reset_radio() -> Result<(), String> {
    drive(
        pin_of(RadioPin::Bootloader),
        RadioPin::Bootloader.level(false),
    )?;
    drive(pin_of(RadioPin::Reset), RadioPin::Reset.level(false))?;
    for step in reset_sequence() {
        match step {
            PinStep::Set(pin, active) => drive(pin_of(pin), pin.level(active))?,
            PinStep::Wait(ms) => std::thread::sleep(Duration::from_millis(ms)),
        }
    }
    Ok(())
}

/// The UART to the radio. Clones share the one driver and its counters.
#[derive(Clone)]
pub struct UartTransport {
    stats: Arc<Mutex<SerialStats>>,
    baud: Arc<AtomicU32>,
}

fn io(reason: String) -> TransportError {
    TransportError::Io(reason)
}

impl UartTransport {
    /// Installs the driver on UART1 at `RADIO_BAUD`: 8 data bits, no parity, 1 stop bit, no flow
    /// control, the interrupt in internal RAM. `now_ms` is the time the first report counts from.
    #[allow(unsafe_code)]
    pub fn install(now_ms: u64) -> Result<Self, String> {
        let config = uart_config_t {
            baud_rate: RADIO_BAUD as i32,
            data_bits: uart_word_length_t_UART_DATA_8_BITS,
            parity: uart_parity_t_UART_PARITY_DISABLE,
            stop_bits: uart_stop_bits_t_UART_STOP_BITS_1,
            flow_ctrl: uart_hw_flowcontrol_t_UART_HW_FLOWCTRL_DISABLE,
            rx_flow_ctrl_thresh: 0,
            __bindgen_anon_1: uart_config_t__bindgen_ty_1 {
                source_clk: soc_periph_uart_clk_src_legacy_t_UART_SCLK_DEFAULT,
            },
            ..Default::default()
        };
        // SAFETY: `uart_driver_install` takes the port, the two ring sizes, no event queue (size 0
        // and a null handle) and the interrupt flags; nothing is borrowed after the call.
        check(unsafe {
            uart_driver_install(
                UART,
                RX_RING_BYTES as i32,
                TX_RING_BYTES as i32,
                0,
                std::ptr::null_mut(),
                ESP_INTR_FLAG_IRAM as i32,
            )
        })
        .map_err(|e| format!("radio: cannot install the UART driver: {e}"))?;
        // SAFETY: `config` is valid for the call, which only reads it.
        check(unsafe { uart_param_config(UART, &config) })
            .map_err(|e| format!("radio: cannot configure the UART: {e}"))?;
        // SAFETY: plain integers: the port, TX and RX pins, and no RTS or CTS (-1 leaves them).
        check(unsafe { uart_set_pin(UART, PIN_TX, PIN_RX, -1, -1) })
            .map_err(|e| format!("radio: cannot set the UART pins: {e}"))?;
        Ok(UartTransport {
            stats: Arc::new(Mutex::new(SerialStats::new(RX_RING_BYTES, now_ms))),
            baud: Arc::new(AtomicU32::new(RADIO_BAUD)),
        })
    }

    /// Changes the speed and drops what was received at the old one.
    #[allow(unsafe_code)]
    pub fn set_baud(&self, baud: u32) -> Result<(), TransportError> {
        // SAFETY: plain integers; the driver is installed (`install` made this value).
        check(unsafe { uart_set_baudrate(UART, baud) }).map_err(io)?;
        // SAFETY: the driver is installed; takes only the port.
        check(unsafe { uart_flush_input(UART) }).map_err(io)?;
        self.baud.store(baud, Ordering::SeqCst);
        Ok(())
    }

    /// The `serial:` line when a report is due.
    pub fn serial_report(&self, now_ms: u64) -> Option<String> {
        let baud = self.baud.load(Ordering::SeqCst);
        self.stats
            .lock()
            .ok()
            .and_then(|mut stats| stats.report(now_ms, baud))
    }

    #[allow(unsafe_code)]
    fn buffered(&self) -> usize {
        let mut size = 0usize;
        // SAFETY: `size` is valid for a write for the call; the driver is installed.
        let code = unsafe { uart_get_buffered_data_len(UART, &mut size) };
        if code == 0 { size } else { 0 }
    }
}

impl RadioTransport for UartTransport {
    #[allow(unsafe_code)]
    fn read(&mut self, buf: &mut [u8], timeout_ms: u64) -> Result<usize, TransportError> {
        if let Ok(mut stats) = self.stats.lock() {
            stats.observe(self.buffered());
        }
        if buf.is_empty() {
            return Ok(0);
        }
        let ticks = u32::try_from(timeout_ms.div_ceil(TICK_MS)).unwrap_or(u32::MAX);
        // Waits for the first byte only: `uart_read_bytes` returns once `length` bytes are in or
        // the time is over, and a read of the whole buffer would hold every ASH acknowledgement
        // back until the buffer fills or the line is silent for `timeout_ms`.
        // SAFETY: `buf` is valid for writes of at least 1 byte; the call writes at most 1 byte
        // and returns how many (negative on error).
        let first = unsafe { uart_read_bytes(UART, buf.as_mut_ptr().cast(), 1, ticks) };
        let first = usize::try_from(first).map_err(|_| io("uart_read_bytes failed".to_string()))?;
        if first == 0 {
            return Ok(0);
        }
        // Then what is already waiting, without waiting more.
        let more = self.buffered().min(buf.len() - 1);
        if more == 0 {
            return Ok(1);
        }
        let length = u32::try_from(more).unwrap_or(u32::MAX);
        // SAFETY: `buf[1..]` holds at least `more` bytes, `more` at most its length; the call
        // writes at most `length` bytes of it and returns how many (negative on error).
        let rest = unsafe { uart_read_bytes(UART, buf[1..].as_mut_ptr().cast(), length, 0) };
        let rest = usize::try_from(rest).map_err(|_| io("uart_read_bytes failed".to_string()))?;
        Ok(1 + rest)
    }

    #[allow(unsafe_code)]
    fn write(&mut self, bytes: &[u8]) -> Result<(), TransportError> {
        let mut rest = bytes;
        while !rest.is_empty() {
            // SAFETY: `rest` is valid for reads of its length for the call, which copies it into
            // the transmit ring (waiting for room) and returns how many bytes it took.
            let count = unsafe { uart_write_bytes(UART, rest.as_ptr().cast(), rest.len()) };
            let taken =
                usize::try_from(count).map_err(|_| io("uart_write_bytes failed".to_string()))?;
            rest = &rest[taken.min(rest.len())..];
        }
        Ok(())
    }
}
