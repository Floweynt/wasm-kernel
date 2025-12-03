use super::{CharSink, ansi::Color};
use core::ptr;
use flanterm::{
    flanterm_context, flanterm_fb_init, flanterm_flush, flanterm_set_autoflush, flanterm_write,
};
use limine::framebuffer::Framebuffer;

pub struct FlanTermTTY {
    context: *mut flanterm_context,
}

impl FlanTermTTY {
    pub fn from_framebuffer(fb: &Framebuffer) -> FlanTermTTY {
        let context: *mut flanterm_context;
        let mut ansi_colors = [
            Color::BLACK.rgb(),
            Color::RED.rgb(),
            Color::GREEN.rgb(),
            Color::YELLOW.rgb(),
            Color::BLUE.rgb(),
            Color::PURPLE.rgb(),
            Color::CYAN.rgb(),
            Color::WHITE.rgb(),
        ];

        let mut ansi_colors_bright = [
            Color::BRIGHT_BLACK.rgb(),
            Color::BRIGHT_RED.rgb(),
            Color::BRIGHT_GREEN.rgb(),
            Color::BRIGHT_YELLOW.rgb(),
            Color::BRIGHT_BLUE.rgb(),
            Color::BRIGHT_PURPLE.rgb(),
            Color::BRIGHT_CYAN.rgb(),
            Color::BRIGHT_WHITE.rgb(),
        ];

        let mut default_bg = Color::BACKGROUND.rgb();
        let mut default_fg = Color::FOREGROUND.rgb();
        
        unsafe {
            context = flanterm_fb_init(
                None,
                None,
                fb.addr() as *mut u32,
                usize::try_from(fb.width()).unwrap(),
                usize::try_from(fb.height()).unwrap(),
                usize::try_from(fb.pitch()).unwrap(),
                fb.red_mask_size(),
                fb.red_mask_shift(),
                fb.green_mask_size(),
                fb.green_mask_shift(),
                fb.blue_mask_size(),
                fb.blue_mask_shift(),
                ptr::null_mut(),
                ansi_colors.as_mut_ptr(),
                ansi_colors_bright.as_mut_ptr(),
                &raw mut default_bg,
                &raw mut default_fg,
                ptr::null_mut(),
                ptr::null_mut(),
                ptr::null_mut(),
                0usize,
                0usize,
                1usize,
                0usize,
                0usize,
                0usize,
            );

            flanterm_set_autoflush(context, false);
        }

        FlanTermTTY { context }
    }
}

impl CharSink for FlanTermTTY {
    unsafe fn putc(&self, ch: u8) {
        unsafe {
            flanterm_write(self.context, ptr::from_ref(&(ch as i8)), 1);

            if ch == b'\n' {
                flanterm_flush(self.context);
            }
        }
    }

    unsafe fn flush(&self) {
        unsafe { flanterm_flush(self.context) };
    }
}

unsafe impl Send for FlanTermTTY {}
unsafe impl Sync for FlanTermTTY {}
