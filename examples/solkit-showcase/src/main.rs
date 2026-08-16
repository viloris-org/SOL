//! CLI entry point for the deterministic SolKit app showcase.

#[cfg(feature = "native")]
fn main() -> Result<(), String> {
    solkit_showcase::run_native_showcase()
}

#[cfg(not(feature = "native"))]
fn main() -> Result<(), String> {
    let report = solkit_showcase::run_headless_showcase()?;
    println!(
        "SolKit showcase: command={}, tab={}, search={}, activated={}, animation={}ms",
        report.command_data,
        report.selected_tab,
        report.search_value,
        report.activated_control,
        report.animation_duration_ms,
    );
    Ok(())
}
