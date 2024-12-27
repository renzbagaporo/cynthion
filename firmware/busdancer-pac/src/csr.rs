pub mod interrupt {
    //! CSR access methods.

    use crate::Interrupt;

    /// Unmask the given [`Interrupt`] in the CPU's Machines IRQ Mask register.
    ///
    /// # Safety
    ///
    /// Passing incorrect value can cause undefined behaviour. See CPU reference manual.
    pub unsafe fn enable(interrupt: Interrupt) {
        //TODO  Implement for I.MX RT
    }

    /// Mask the given [`Interrupt`] in the CPU's Machines IRQ Mask register.
    ///
    /// # Safety
    ///
    /// Passing incorrect value can cause undefined behaviour. See CPU reference manual.
    pub unsafe fn disable(interrupt: Interrupt) {
        //TODO  Implement for I.MX RT
    }

    /// Return the current value of the CPU's Machines IRQ Mask register.
    #[must_use]
    pub fn reg_mask() -> usize {
        //TODO  Implement for I.MX RT
        0
    }

    /// Return the current bit value of the CPU's Machines IRQ Pending register.
    #[must_use]
    pub fn bits_pending() -> usize {
        //TODO  Implement for I.MX RT
        0
    }

    /// Check if the given `Interrupt` is pending in the CPU's Machines IRQ Pending register.
    #[must_use]
    pub fn is_pending(interrupt: Interrupt) -> bool {
        //TODO  Implement for I.MX RT
        true
    }

    /// Returns the current `Interrupt` pending in the CPU's Machines IRQ Pending register.
    ///
    /// If there is no interrupt pending or an unknown interrupt
    /// pending it returns an `Err` containing the current bit value
    /// of the register.
    pub fn pending() -> Result<Interrupt, usize> {
        //TODO  Implement for I.MX RT
        Err(0)
    }
}
