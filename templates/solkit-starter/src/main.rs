fn main() -> Result<(), String> {
    let report = solkit_starter::run()?;
    println!(
        "SolKit starter: command={}, activated={}, reduced-motion={}ms",
        report.command_result, report.activated_control, report.reduced_motion_duration_ms
    );
    Ok(())
}
