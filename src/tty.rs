use core::ptr;
use flanterm::{flanterm_context, flanterm_fb_init, flanterm_write, flanterm_set_autoflush};
use limine::framebuffer::Framebuffer;

pub trait TTYHandler {
    fn putc(&self, ch: char);
}

// static data: *dyn TTYHandler = ptr::null();

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

unsafe extern "C" {
    pub fn flanterm_flush(ctx: *mut flanterm_context);
}

impl TTYHandler for FlanTermTTY {
    fn putc(&self, ch: char) {
        unsafe {
            flanterm_write(self.context, ptr::from_ref(&(ch as i8)), 1);

            if ch == '\n' {
                flanterm_flush(self.context);
            }
        }
    }
}
