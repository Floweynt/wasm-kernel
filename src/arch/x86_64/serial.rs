use core::cell::SyncUnsafeCell;

use uart_16550::SerialPort;

use crate::log::CharSink;

pub struct SerialCharSink {
    serial: SyncUnsafeCell<SerialPort>,
}

impl SerialCharSink {
    pub fn open(port: u16) -> SerialCharSink {
        let mut serial = unsafe { SerialPort::new(port) };
        serial.init();
        SerialCharSink {
            serial: SyncUnsafeCell::new(serial),
        }
    }
}

impl CharSink for SerialCharSink {
    unsafe fn putc(&self, ch: u8) {
        unsafe { &mut *self.serial.get() }.send(ch);
    }

    unsafe fn flush(&self) {
        // no-op
    }
}
