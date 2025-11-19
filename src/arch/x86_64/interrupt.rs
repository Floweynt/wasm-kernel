use core::arch::naked_asm;

use crate::tty::println;

#[repr(C)]
struct InterruptContext {
    regs: [u64; 14],

    err: u64,
    id: u64,

    rip: u64,
    cs: u64,
    rflags: u64,
    rsp: u64,
    ss: u64,
}

#[unsafe(naked)]
pub unsafe extern "C" fn irq_handler_entry<const I: u8>() -> ! {
    naked_asm!(
        // required for ABI reasons
        "cld",

        // normalize the stack frame: [int#, ec]
        "pushq ${}",
        "subq ${}, %rsp",

        "pushq %rax",
        "pushq %rcx",
        "pushq %rdx",
        "pushq %rbx",
        "pushq %rsi",
        "pushq %rdi",
        "pushq %r8",
        "pushq %r9",
        "pushq %r10",
        "pushq %r11",
        "pushq %r12",
        "pushq %r13",
        "pushq %r14",
        "pushq %r15",

        // point to top of stack
        "movq %rsp, %rdi",

        // simulate the call frame
        "pushq $0",
        "pushq %rbp",
        "movq %rsp, %rbp",

        // align stack
        "addq $8, %rsp",
        "andq $~15, %rsp",

        // invoke
        "call {}",

        "movq %rbp, %rsp",
        "popq %rbp",

        "addq $16, %rsp",

        "popq %r15",
        "popq %r14",
        "popq %r13",
        "popq %r12",
        "popq %r11",
        "popq %r10",
        "popq %r9",
        "popq %r8",
        "popq %rdi",
        "popq %rsi",
        "popq %rbx",
        "popq %rdx",
        "popq %rcx",
        "popq %rax",

        "addq $16, %rsp",
        "iretq",
        const I,
        const if I == 8 || (10..=14).contains(&I) || I == 17 || I == 21  { 0 } else { 8 },
        sym irq_handler_trampoline
    );
}

unsafe extern "C" fn irq_handler_trampoline(addr: *mut InterruptContext) {
    let mut context = unsafe { &*addr };
    println!("hi!");
}
