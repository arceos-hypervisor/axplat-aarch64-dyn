use core::arch::naked_asm;

use pie_boot::BootInfo;
const BOOT_STACK_SIZE: usize = 0x40000; // 256KB

#[unsafe(link_section = ".bss.stack")]
static mut BOOT_STACK: [u8; BOOT_STACK_SIZE] = [0; BOOT_STACK_SIZE];

#[pie_boot::entry]
fn main(args: &BootInfo) -> ! {
    unsafe {
        switch_sp(args);
    }
}
#[unsafe(naked)]
unsafe extern "C" fn switch_sp(_args: &BootInfo) -> ! {
    naked_asm!(
        "
        adrp x8, {sp}
        add  x8, x8, :lo12:{sp}
        add  x8, x8, {size}
        mov  sp, x8
        bl   {next}
        ",
        sp = sym BOOT_STACK,
        size = const BOOT_STACK_SIZE,
        next = sym sp_reset,
    )
}

fn sp_reset(args: &BootInfo) -> ! {
    axplat::call_main(0, args.fdt.map(|p| p.as_ptr() as usize).unwrap_or_default());
}
