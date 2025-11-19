use crate::tty::TTYHandler;
use uart_16550::SerialPort;

pub struct SerialTTY {
    serial: SerialPort,
}

impl SerialTTY {
    pub fn open(port: u16) -> SerialTTY {
        let mut serial = unsafe { SerialPort::new(port) };
        serial.init();
        SerialTTY { serial }
    }
}

impl TTYHandler for SerialTTY {
    fn putc(&mut self, ch: u8) {
        self.serial.send(ch);
    }
}
