//! `synbot pairing` — manage channel pairings (supplement to allowlist).

use anyhow::{bail, Result};
use clap::Subcommand;

use crate::config::{
    is_pairing_channel_provider, load_config, save_config, Config, PairingEntry,
};

#[derive(Subcommand)]
pub enum PairingAction {
    /// Add a pairing for channel provider + code (from the bot's pairing hint).
    Approve {
        /// Channel provider: feishu, discord, telegram, …
        channel: String,
        /// 12-character hex pairing code from the hint.
        code: String,
    },
    /// List all pairings.
    List,
    /// Remove a pairing entry.
    Remove {
        channel: String,
        code: String,
    },
}

fn normalize_pairing_code(code: &str) -> Result<String> {
    let t = code.trim();
    if t.len() != 12 {
        bail!("pairing code must be exactly 12 hexadecimal characters");
    }
    if !t.chars().all(|c| c.is_ascii_hexdigit()) {
        bail!("pairing code must be hexadecimal (0-9, a-f)");
    }
    Ok(t.to_ascii_lowercase())
}

fn normalize_channel(channel: &str) -> Result<String> {
    let c = channel.trim().to_ascii_lowercase();
    if c.is_empty() {
        bail!("channel name is required");
    }
    if !is_pairing_channel_provider(&c) {
        bail!(
            "unknown channel provider {:?}; expected one of: {}",
            c,
            crate::config::PAIRING_CHANNEL_PROVIDERS.join(", ")
        );
    }
    Ok(c)
}

pub async fn cmd_pairing(action: PairingAction) -> Result<()> {
    let path = crate::config::config_path();
    let mut cfg = load_config(Some(path.as_path()))?;

    match action {
        PairingAction::Approve { channel, code } => {
            let ch = normalize_channel(&channel)?;
            let code = normalize_pairing_code(&code)?;
            approve_pairing(&mut cfg, &ch, &code)?;
            save_config(&cfg, Some(path.as_path()))?;
            println!("Paired: channel={} pairingCode={}", ch, code);
        }
        PairingAction::List => {
            if cfg.pairings.is_empty() {
                println!("No pairings configured.");
            } else {
                println!(
                    "{:<12} {}",
                    "CHANNEL", "PAIRING_CODE"
                );
                for p in &cfg.pairings {
                    println!("{:<12} {}", p.channel, p.pairing_code);
                }
            }
        }
        PairingAction::Remove { channel, code } => {
            let ch = normalize_channel(&channel)?;
            let code = normalize_pairing_code(&code)?;
            let before = cfg.pairings.len();
            cfg.pairings.retain(|p| {
                !(p.channel.eq_ignore_ascii_case(&ch) && p.pairing_code.eq_ignore_ascii_case(&code))
            });
            if cfg.pairings.len() == before {
                bail!("no matching pairing for channel={} code={}", ch, code);
            }
            save_config(&cfg, Some(path.as_path()))?;
            println!("Removed pairing: channel={} pairingCode={}", ch, code);
        }
    }
    Ok(())
}

fn approve_pairing(cfg: &mut Config, channel: &str, code: &str) -> Result<()> {
    if cfg.pairings.iter().any(|p| {
        p.channel.eq_ignore_ascii_case(channel) && p.pairing_code.eq_ignore_ascii_case(code)
    }) {
        println!("Pairing already exists (no changes).");
        return Ok(());
    }
    cfg.pairings.push(PairingEntry {
        channel: channel.to_string(),
        pairing_code: code.to_string(),
    });
    Ok(())
}
