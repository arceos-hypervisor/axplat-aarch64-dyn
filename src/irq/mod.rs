use core::sync::atomic::AtomicI32;

use aarch64_cpu::registers::*;
use axplat::irq::{HandlerTable, IrqHandler, IrqIf};
use log::{debug, info, trace, warn};
use rdrive::{Device, driver::intc::*};
use spin::Mutex;

mod v2;
mod v3;

/// The maximum number of IRQs.
const MAX_IRQ_COUNT: usize = 1024;

static VERSION: AtomicI32 = AtomicI32::new(0);

static IRQ_HANDLER_TABLE: HandlerTable<MAX_IRQ_COUNT> = HandlerTable::new();

struct IrqIfImpl;

#[impl_plat_interface]
impl IrqIf for IrqIfImpl {
    /// Enables or disables the given IRQ.
    fn set_enable(irq_raw: usize, enabled: bool) {
        set_enable(irq_raw, None, enabled);
    }

    /// Registers an IRQ handler for the given IRQ.
    ///
    /// It also enables the IRQ if the registration succeeds. It returns `false`
    /// if the registration failed.
    fn register(irq_num: usize, handler: IrqHandler) -> bool {
        trace!("register handler IRQ {}", irq_num);
        if IRQ_HANDLER_TABLE.register_handler(irq_num, handler) {
            Self::set_enable(irq_num, true);
            return true;
        }
        warn!("register handler for IRQ {} failed", irq_num);
        false
    }

    /// Unregisters the IRQ handler for the given IRQ.
    ///
    /// It also disables the IRQ if the unregistration succeeds. It returns the
    /// existing handler if it is registered, `None` otherwise.
    fn unregister(irq_num: usize) -> Option<IrqHandler> {
        trace!("unregister handler IRQ {}", irq_num);
        Self::set_enable(irq_num, false);
        IRQ_HANDLER_TABLE.unregister_handler(irq_num)
    }

    /// Handles the IRQ.
    ///
    /// It is called by the common interrupt handler. It should look up in the
    /// IRQ handler table and calls the corresponding handler. If necessary, it
    /// also acknowledges the interrupt controller after handling.
    fn handle(irq_num: usize) {
        match gic_version() {
            2 => v2::handle(irq_num),
            3 => v3::handle(irq_num),
            _ => panic!("Unsupported GIC version"),
        }
    }
}

pub(crate) fn init() {
    let intc = get_gicd();
    debug!("Initializing GICD...");
    let mut gic = intc.lock().unwrap();
    gic.open().unwrap();
    debug!("GICD initialized");
}

fn gic_version() -> i32 {
    VERSION.load(core::sync::atomic::Ordering::SeqCst)
}

pub(crate) fn init_current_cpu() {
    {
        let mut intc = get_gicd().lock().unwrap();
        if let Some(v) = intc.typed_mut::<v2::Gic>() {
            let cpu = v.cpu_interface();
            v2::TRAP.call_once(|| cpu.trap_operations());
            v2::CPU_IF.with_current(|c| {
                c.call_once(|| Mutex::new(cpu));
            });

            VERSION.store(2, core::sync::atomic::Ordering::SeqCst);
        }

        if let Some(v) = intc.typed_mut::<v3::Gic>() {
            let cpu = v.cpu_interface();
            v3::TRAP.call_once(|| cpu.trap_operations());
            v3::CPU_IF.with_current(|c| {
                c.call_once(|| Mutex::new(cpu));
            });
            VERSION.store(3, core::sync::atomic::Ordering::SeqCst);
        }
    }
    match gic_version() {
        2 => v2::init_current_cpu(),
        3 => v3::init_current_cpu(),
        _ => panic!("Unsupported GIC version"),
    }
    debug!("GIC initialized for current CPU");
}

fn get_gicd() -> Device<Intc> {
    rdrive::get_one().expect("no interrupt controller found")
}

fn current_cpu() -> usize {
    MPIDR_EL1.get() as usize & 0xffffff
}

pub(crate) fn set_enable(irq_raw: usize, trigger: Option<Trigger>, enabled: bool) {
    trace!(
        "set_enable: irq_raw={:#x}, trigger={:?}, enabled={}",
        irq_raw, trigger, enabled
    );
    let t = trigger.map(|t| t.into());
    match gic_version() {
        2 => v2::set_enable(irq_raw, t, enabled),
        3 => v3::set_enable(irq_raw, t, enabled),
        _ => panic!("Unsupported GIC version"),
    }
}
