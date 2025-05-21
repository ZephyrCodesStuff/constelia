use anyhow::Result;
use clap::Parser;
// No fields because we'll ask for the info interactively
#[derive(Debug, Parser)]
pub struct Attack {}

pub async fn attack(scheduler_addr: &str) -> Result<()> {
    // TODO: ask for the required info with `inquire`
    // 1. Get the list of targets from remote
    // 2. Ask if they want to use that, or use a custom target (we can't add them yet)
    // 3. Ask for the exploit(s) to use on the target(s)
    // 4. Queue all of the necessary jobs for the given target(s)

    Ok(())
}
