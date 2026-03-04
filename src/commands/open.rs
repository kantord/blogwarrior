use anyhow::ensure;

use crate::feed::FeedItem;

pub(crate) fn cmd_print_url(item: &FeedItem) -> anyhow::Result<()> {
    ensure!(!item.link.is_empty(), "Post has no link");
    println!("{}", item.link);
    Ok(())
}

pub(crate) fn cmd_open_item(item: &FeedItem) -> anyhow::Result<()> {
    ensure!(!item.link.is_empty(), "Post has no link");
    match std::env::var("BROWSER") {
        Ok(browser) => {
            // Run directly so TUI browsers (w3m, elinks) inherit the terminal
            let status = std::process::Command::new(&browser)
                .arg(&item.link)
                .status()
                .map_err(|e| anyhow::anyhow!("Could not open URL: {}", e))?;
            if !status.success() {
                anyhow::bail!("{} exited with {}", browser, status);
            }
        }
        Err(_) => {
            open::that(&item.link).map_err(|e| anyhow::anyhow!("Could not open URL: {}", e))?;
        }
    }
    eprintln!("Opened in browser: {}", item.link);
    Ok(())
}
