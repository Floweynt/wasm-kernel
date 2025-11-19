use super::TTYHandler;
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
                ptr::null_mut(),
                ptr::null_mut(),
                ptr::null_mut(),
                ptr::null_mut(),
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

        FlanTermTTY { context: context }
    }
}

impl TTYHandler for FlanTermTTY {
    fn putc(&mut self, ch: u8) {
        unsafe {
            flanterm_write(self.context, ptr::from_ref(&(ch as i8)), 1);

            if ch == b'\n' {
                flanterm_flush(self.context);
            }
        }
    }
}

unsafe impl Send for FlanTermTTY {}
unsafe impl Sync for FlanTermTTY {}
