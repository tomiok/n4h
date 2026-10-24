#[cfg(feature = "fault-injection")]
pub fn failpoint(name: &str) {
    if std::env::var("N4H_TEST_FAILPOINT").as_deref() == Ok(name) {
        std::process::exit(86);
    }
}

#[cfg(not(feature = "fault-injection"))]
pub fn failpoint(_name: &str) {}

pub fn before_write(phase: &str, index: usize) -> crate::error::Result<()> {
    #[cfg(feature = "fault-injection")]
    if std::env::var("N4H_TEST_FAILPOINT").as_deref()
        == Ok(format!("{phase}_io_before_file_{}", index + 1).as_str())
    {
        return Err(crate::error::Error::new(
            "io_error",
            "Injected write failure before file replacement.",
            4,
        ));
    }
    let _ = (phase, index);
    Ok(())
}
