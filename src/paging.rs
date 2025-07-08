use aarch64_cpu_ext::cache::{CacheOp, dcache_all, icache_flush_all};
use memory_addr::{MemoryAddr, PhysAddr, VirtAddr};
use page_table_multiarch::aarch64::A64PageTable;
use page_table_multiarch::{MappingFlags, PagingHandler};
use pie_boot::{KLINER_OFFSET, boot_info};

pub(crate) static mut BOOT_PT: usize = 0;
static mut BOOT_PT_ITER: usize = 0;

struct PagingHandlerImpl;

impl PagingHandler for PagingHandlerImpl {
    fn alloc_frame() -> Option<PhysAddr> {
        unsafe {
            let addr = BOOT_PT_ITER;
            BOOT_PT_ITER += 0x1000; // Allocate 4KB frame
            Some(PhysAddr::from_usize(addr))
        }
    }

    fn dealloc_frame(_paddr: PhysAddr) {}

    fn phys_to_virt(paddr: PhysAddr) -> VirtAddr {
        (paddr.as_usize() + KLINER_OFFSET).into()
    }
}

pub fn init() {
    unsafe {
        BOOT_PT = (boot_info().free_memory_start as usize).align_up_4k();
        BOOT_PT_ITER = BOOT_PT;
    }

    let mut pt = A64PageTable::<PagingHandlerImpl>::try_new().unwrap();

    let va_offset = boot_info().kcode_offset();
    let vaddr = boot_info().kimage_start_vma as usize;
    let size = 512 * 1024 * 1024; // 512MB

    console_println!(
        "Mapping kernel image at {:#x} with offset {:#x}",
        vaddr,
        va_offset
    );

    pt.map_region(
        vaddr.into(),
        |ptr| (ptr.as_usize() - va_offset).into(),
        size,
        MappingFlags::READ | MappingFlags::WRITE | MappingFlags::EXECUTE,
        true,
        false,
    )
    .unwrap()
    .ignore();

    let size = 1024 * 1024 * 1024; // 1GB

    pt.map_region(
        VirtAddr::from_usize(boot_info().kimage_start_lma as usize),
        |ptr| (ptr.as_usize()).into(),
        size,
        MappingFlags::READ | MappingFlags::WRITE | MappingFlags::EXECUTE,
        true,
        false,
    )
    .unwrap()
    .ignore();

    for range in boot_info()
        .memory_regions
        .iter()
        .filter(|one| matches!(one.kind, pie_boot::MemoryRegionKind::Ram))
    {
        if range.end - range.start < 0x1000 {
            continue; // Skip small regions
        }
        let liner_vaddr = range.start + KLINER_OFFSET;
        console_println!(
            "Mapping RAM region: {:#x} - {:#x} to linear vaddr: {:#x}",
            range.start,
            range.end,
            liner_vaddr
        );
        pt.map_region(
            liner_vaddr.into(),
            |ptr| (ptr.as_usize() - KLINER_OFFSET).into(),
            range.end - range.start,
            MappingFlags::READ | MappingFlags::WRITE | MappingFlags::EXECUTE,
            true,
            false,
        )
        .unwrap()
        .ignore();
    }

    if let Some(com) = &boot_info().debug_console {
        let com_start = (com.base as usize).align_down_4k() + KLINER_OFFSET;
        let size = 0x1000;

        pt.map_region(
            com_start.into(),
            |ptr| (ptr.as_usize() - KLINER_OFFSET).into(),
            size,
            MappingFlags::READ | MappingFlags::WRITE | MappingFlags::DEVICE,
            true,
            false,
        )
        .unwrap()
        .ignore();
    }
    dcache_all(CacheOp::CleanAndInvalidate);
    icache_flush_all();
    console_println!("Boot page table initialized at {:#x}", pt.root_paddr());
    // unsafe{
    //     axcpu::asm::write_kernel_page_table(pt.root_paddr());
    // }
    // unsafe { axcpu::init::init_mmu(pt.root_paddr()) };
}

pub fn tb_range() -> (usize, usize) {
    unsafe {
        let start = BOOT_PT;
        let size = BOOT_PT_ITER - start;

        console_println!("TB range: start = {:#x}, size = {:#x}", start, size);

        (start, size)
    }
}
