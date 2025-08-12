use aarch64_cpu::registers::*;
use alloc::{format, string::String};

use axplat::irq::{HandlerTable, IrqHandler, IrqIf};
use lazyinit::LazyInit;
use log::{debug, trace, warn};
use rdrive::{Device, driver::intc::*};

const SPI_START: usize = 32;
/// The maximum number of IRQs.
const MAX_IRQ_COUNT: usize = 1024;

fn is_irq_private(irq_num: usize) -> bool {
    irq_num < SPI_START
}

// per-CPU, no lock
static CPU_IF: LazyInit<local::Boxed> = LazyInit::new();
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
    fn handle(irq: usize) {
        let irq = if irq == 0 {
            let Some(irq) = CPU_IF.ack() else {
                return;
            };
            irq
        } else {
            IrqId::from(irq)
        };

        let irq_num: usize = irq.into();
        trace!("IRQ {}", irq_num);
        if !IRQ_HANDLER_TABLE.handle(irq_num as _) {
            warn!("Unhandled IRQ {irq_num}");
        }

        CPU_IF.eoi(irq);
        if CPU_IF.get_eoi_mode() {
            CPU_IF.dir(irq);
        }
    }

    fn irq_fetch() -> usize {
        let irq = CPU_IF.ack();
        if let Some(irq) = irq {
            trace!("IRQ fetched: {irq:?}");
            irq.into()
        } else {
            trace!("No IRQ fetched");
            0
        }
    }
}

pub(crate) fn init() {
    let intc = get_gicd();
    debug!("Initializing GICD...");
    intc.lock().unwrap().open().unwrap();
    debug!("GICD initialized");
}

pub(crate) fn init_current_cpu() {
    let intc = rdrive::get_one::<Intc>().expect("no interrupt controller found");
    let mut cpu_if = intc.lock().unwrap().cpu_local().unwrap();
    cpu_if.open().unwrap();
    cpu_if.set_eoi_mode(true);
    CPU_IF.call_once(move || cpu_if);
    debug!("GIC initialized for current CPU");
}

fn get_gicd() -> Device<Intc> {
    rdrive::get_one().expect("no interrupt controller found")
}

fn current_cpu() -> usize {
    MPIDR_EL1.get() as usize & 0xffffff
}

pub(crate) fn set_enable(irq_raw: usize, trigger: Option<Trigger>, enabled: bool) {
    debug!(
        "IRQ({:#x}) set enable: {}, {}",
        irq_raw,
        enabled,
        match trigger {
            Some(t) => format!("trigger: {t:?}"),
            None => String::new(),
        }
    );
    let irq: IrqId = irq_raw.into();
    if is_irq_private(irq_raw)
        && let local::Capability::ConfigLocalIrq(c) = CPU_IF.capability()
    {
        if enabled {
            c.irq_enable(irq).expect("failed to enable local IRQ");
        } else {
            c.irq_disable(irq).expect("failed to disable local IRQ");
        }
        if let Some(t) = trigger {
            c.set_trigger(irq, t)
                .expect("failed to set local IRQ trigger");
        }
    } else {
        let mut intc = get_gicd().lock().unwrap();
        if enabled {
            intc.irq_enable(irq).expect("failed to enable IRQ");
            if !is_irq_private(irq_raw) {
                // For private IRQs, we need to acknowledge the interrupt
                // controller.
                intc.set_target_cpu(irq, current_cpu().into()).unwrap();
            }

            if let Some(t) = trigger {
                intc.set_trigger(irq, t).expect("failed to set IRQ trigger");
            }
        } else {
            intc.irq_disable(irq).expect("failed to disable IRQ");
        }
    }
    debug!("IRQ({irq_raw:#x}) set enable done");
}
