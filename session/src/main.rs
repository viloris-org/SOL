use std::process::ExitCode;

fn main() -> ExitCode {
    let cli = match sol_session::parse_cli(std::env::args().skip(1)) {
        Ok(cli) => cli,
        Err(message) => {
            println!("{message}");
            return if message == sol_session::usage() {
                ExitCode::SUCCESS
            } else {
                ExitCode::from(2)
            };
        }
    };
    let environment = match sol_session::environment(cli.socket_override) {
        Ok(environment) => environment,
        Err(error) => {
            eprintln!("sol-session: {error}");
            return ExitCode::from(2);
        }
    };
    let plan =
        sol_session::LaunchPlan::new(&environment, &sol_session::ProgramPaths::from_environment());
    if cli.dry_run {
        print!("{}", plan.dry_run_output());
        return ExitCode::SUCCESS;
    }
    let result = if cli.attach {
        sol_session::run_attached(&plan)
    } else {
        sol_session::run(&plan)
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("sol-session: {error}");
            ExitCode::from(1)
        }
    }
}
