//! Interrupt controller — unified IRQ registration across ARM (GIC) and x86 (APIC).

/// Interrupt vector / IRQ number.
pub type Irq = u32;

/// Interrupt handler function pointer.
pub type Handler = unsafe extern "C" fn();

/// Trigger mode for an interrupt line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TriggerMode {
    Edge,
    Level,
}

/// Polarity of an interrupt line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Polarity {
    ActiveHigh,
    ActiveLow,
}

/// Configuration for an IRQ registration.
#[derive(Debug, Clone, Copy)]
pub struct IrqConfig {
    pub trigger: TriggerMode,
    pub polarity: Polarity,
    pub target_cpu: Option<u32>, // None = any CPU
}

impl Default for IrqConfig {
    fn default() -> Self {
        Self { trigger: TriggerMode::Edge, polarity: Polarity::ActiveHigh, target_cpu: None }
    }
}

/// Register an interrupt handler for the given IRQ.
///
/// # Safety
/// The handler must be a valid function pointer that does not block.
pub unsafe fn register_irq(irq: Irq, config: IrqConfig, handler: Handler) {
    crate::arch::register_irq(irq, config, handler);
}

/// Unregister a previously registered IRQ handler.
pub unsafe fn unregister_irq(irq: Irq) {
    crate::arch::unregister_irq(irq);
}

/// Send an inter-processor interrupt to the target CPU.
///
/// # Safety
/// IPIs are used for TLB shootdowns, scheduling, and rescheduling.
pub unsafe fn send_ipi(cpu: u32, vector: Irq) {
    crate::arch::send_ipi(cpu, vector);
}

/// Acknowledge the current interrupt at the interrupt controller.
/// Called by the interrupt dispatcher before running the handler.
pub unsafe fn ack_irq(irq: Irq) {
    crate::arch::ack_irq(irq);
}

/// End-of-interrupt: tell the controller the handler is done.
pub unsafe fn eoi(irq: Irq) {
    crate::arch::eoi(irq);
}
