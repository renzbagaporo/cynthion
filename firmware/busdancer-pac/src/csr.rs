pub mod interrupt {
    //! CSR access methods.

    use crate::Interrupt;

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
