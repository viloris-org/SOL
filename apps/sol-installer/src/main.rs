//! Live-image entry point for the SOL installation welcome page.

use sol_installer::{InstallerWelcome, LiveSessionInfo};

fn release_name() -> String {
    std::env::var("SOL_RELEASE_NAME").unwrap_or_else(|_| "SOL Preview 0.1".to_owned())
}

#[cfg(feature = "native")]
fn main() -> Result<(), String> {
    use sol_design::accessibility::TokenMode;
    use sol_installer::InstallGuideOutcome;
    use sol_ui::{GuidedPageAction, NativeGuidedPageRenderer};

    let mut welcome = InstallerWelcome::new(LiveSessionInfo::new(release_name(), true))?;
    welcome.app.start().map_err(|error| error.to_string())?;
    let renderer = NativeGuidedPageRenderer::new()?;
    renderer.render(&welcome.frame_for(TokenMode::light()));
    let outcome = match renderer.run_until_action()? {
        GuidedPageAction::Primary => InstallGuideOutcome::BeginInstallation,
        GuidedPageAction::Secondary | GuidedPageAction::Dismissed => {
            InstallGuideOutcome::KeepExploring
        }
    };
    println!("sol-installer: {outcome:?}");
    Ok(())
}

#[cfg(not(feature = "native"))]
fn main() -> Result<(), String> {
    use sol_design::accessibility::TokenMode;

    let mut welcome = InstallerWelcome::new(LiveSessionInfo::new(release_name(), true))?;
    welcome.app.start().map_err(|error| error.to_string())?;
    let frame = welcome.frame_for(TokenMode::light());
    println!(
        "{} — {} [{} / {}]",
        frame.eyebrow, frame.title, frame.primary.label, frame.secondary.label
    );
    Ok(())
}
