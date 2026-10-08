//! Foundation-stage tool; the desktop controls are connected in the next stage.
use canopy_desktop::settings::{Access, Change, PreferenceKey, SettingsClient};
use std::path::PathBuf;

fn main() {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    let result = futures_lite::future::block_on(async {
        let command = args.first().and_then(|s| s.to_str()).unwrap_or("");
        match (command, args.len()) {
            ("init", 2) => {
                let client = SettingsClient::create(PathBuf::from(&args[1])).await?;
                println!(
                    "{}",
                    serde_json::to_string_pretty(&client.load().await?)
                        .expect("serializable snapshot")
                );
                client.shutdown().await?;
            }
            ("import", 3) => {
                let report = SettingsClient::import_electron(
                    PathBuf::from(&args[1]),
                    PathBuf::from(&args[2]),
                )
                .await?;
                println!(
                    "{}",
                    serde_json::to_string_pretty(&report).expect("serializable report")
                );
            }
            ("show", 2) => {
                let client =
                    SettingsClient::open(PathBuf::from(&args[1]), Access::ReadOnly).await?;
                println!(
                    "{}",
                    serde_json::to_string_pretty(&client.load().await?)
                        .expect("serializable snapshot")
                );
                client.shutdown().await?;
            }
            ("set", 4) => {
                let key = args[2].to_str().unwrap_or("").parse::<PreferenceKey>()?;
                let change = Change::parse(key, args[3].to_str().unwrap_or(""))?;
                let client =
                    SettingsClient::open(PathBuf::from(&args[1]), Access::ReadWrite).await?;
                println!(
                    "{}",
                    serde_json::to_string_pretty(&client.apply(vec![change]).await?)
                        .expect("serializable snapshot")
                );
                client.shutdown().await?;
            }
            _ => {
                eprintln!(
                    "Usage: canopy-settings init DB | import ELECTRON_DB NEW_DB | show DB | set DB KEY VALUE"
                );
                std::process::exit(2);
            }
        }
        Ok::<_, canopy_desktop::settings::Error>(())
    });
    if let Err(error) = result {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
