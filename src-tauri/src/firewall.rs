//! Windows Firewall detection and repair (FR-28, FR-29).
//!
//! The installer creates a program-scoped inbound Allow rule before first
//! launch so the user never sees the Windows Firewall prompt. This module
//! detects whether that rule exists, finds stale Block rules (Windows writes
//! one when the user dismisses the prompt), and supports in-app repair.
//!
//! Detection uses PowerShell's `Get-NetFirewallRule`, which returns structured
//! objects and is consistent across Windows locales — unlike netsh text
//! parsing. The repair path (deleting/creating rules) requires elevation and
//! requests UAC from the app with the user's explicit consent.
//!
//! This module is Windows-only: Linux and macOS have no program-scoped
//! firewall rule to manage, so the module compiles to nothing there and
//! `commands.rs` supplies a graceful no-op status.
#![cfg(target_os = "windows")]

use std::process::Command;

/// The canonical DisplayName of the program-scoped inbound Allow rule
/// created by `installer/firewall.ps1`.
pub const ALLOW_RULE_NAME: &str = "AirLynk (inbound LAN)";

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum FirewallError {
    #[error("PowerShell command failed: {0}")]
    PsCommand(String),
    #[error("elevation required to modify firewall rules")]
    ElevationRequired,
    #[error("repair failed: {0}")]
    RepairFailed(String),
}

/// The combined firewall state, for the UI to pick the right copy (FR-29).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FirewallStatus {
    /// Allow rule present, no stale Block rules. Normal operation.
    Clean,
    /// Allow rule missing. The installer did not run, or a reinstall is due.
    MissingAllowRule,
    /// One or more stale Block rules target AirLynk. Block beats Allow, so
    /// nothing else matters until these are deleted (FR-29).
    StaleBlockRules { names: Vec<String> },
    /// Could not determine the state (e.g. PowerShell unavailable).
    Unknown,
}

// ---------------------------------------------------------------------------
// Detection (no elevation needed)
// ---------------------------------------------------------------------------

/// Check whether an Allow rule exists **and** targets this exact executable.
///
/// The rule must match by DisplayName *and* by its Program filter. A rule
/// pointing at a stale path (project moved, target dir rebuilt elsewhere,
/// app reinstalled) silently stops matching the running binary while still
/// passing a name-only check — the phone would hang forever while the UI
/// reported "clean". Comparison is case-insensitive (Windows paths are not
/// case-sensitive) after trimming PowerShell's trailing CRLF. An empty
/// Program filter means the rule covers any program, which also matches.
///
/// Uses `Get-NetFirewallRule`; a missing rule yields the sentinel. Does not
/// need elevation.
pub fn allow_rule_exists() -> bool {
    let exe = std::env::current_exe()
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_default();
    if exe.is_empty() {
        return false;
    }
    let result = run_ps_script(&format!(
        r#"$r = Get-NetFirewallRule -DisplayName "{ALLOW_RULE_NAME}" -ErrorAction SilentlyContinue; if ($r) {{ ($r | Get-NetFirewallApplicationFilter).Program }} else {{ '{NO_RULE_SENTINEL}' }}"#
    ));
    match result {
        Ok(out) => !out.contains(NO_RULE_SENTINEL) && program_matches(&out, &exe),
        Err(_) => false,
    }
}

/// Sentinel returned by the detection script when no rule with the canonical
/// DisplayName exists — distinguishes "no rule" from "rule with no Program
/// filter" (the latter means the rule allows any program and matches).
const NO_RULE_SENTINEL: &str = "__airlynk-no-rule__";

/// True when a firewall rule's Program filter covers `exe`. Pure decision
/// core, unit-tested: an empty filter means "any program"; anything else must
/// equal the exe path case-insensitively. An empty `exe` can never be
/// verified, so it never matches.
fn program_matches(rule_program: &str, exe: &str) -> bool {
    if exe.trim().is_empty() {
        return false;
    }
    let rule = rule_program.trim().to_ascii_lowercase();
    rule.is_empty() || rule == exe.trim().to_ascii_lowercase()
}

/// Find stale **Block** rules that target AirLynk.
///
/// When a user dismisses the Windows Firewall prompt, Windows writes a Block
/// rule for that program. Block takes precedence over any Allow added later
/// (FR-29), so these must be deleted before an Allow rule can matter.
///
/// Returns the DisplayName of each Block rule found. Does not need elevation.
pub fn find_stale_blocks() -> Result<Vec<String>, FirewallError> {
    let script = r#"
$rules = Get-NetFirewallRule -Direction Inbound -Action Block -Enabled True -ErrorAction SilentlyContinue |
    Where-Object { $_.Program -and $_.Program -like '*airlynk*' }
$rules | ForEach-Object { $_.DisplayName }
"#;
    let out = run_ps_script(script)?;
    Ok(out
        .lines()
        .map(|l| l.trim().to_string())
        .filter(|l| !l.is_empty())
        .collect())
}

/// Pure decision core: assemble a `FirewallStatus` from raw observations.
/// Stale Block rules take precedence, because Block beats Allow regardless of
/// what the Allow rule says (FR-29).
pub fn classify(allow_exists: bool, stale_blocks: Vec<String>) -> FirewallStatus {
    if !stale_blocks.is_empty() {
        return FirewallStatus::StaleBlockRules { names: stale_blocks };
    }
    if allow_exists {
        FirewallStatus::Clean
    } else {
        FirewallStatus::MissingAllowRule
    }
}

/// One-shot detection for the UI. Returns `Unknown` when PowerShell is not
/// available or errors, so the app can degrade gracefully rather than lie.
pub fn status() -> FirewallStatus {
    let allow = allow_rule_exists();
    match find_stale_blocks() {
        Ok(blocks) => classify(allow, blocks),
        Err(_) => FirewallStatus::Unknown,
    }
}

// ---------------------------------------------------------------------------
// Repair (requires elevation)
// ---------------------------------------------------------------------------

/// Delete a firewall rule by DisplayName. Requires elevation.
pub fn delete_rule(name: &str) -> Result<(), FirewallError> {
    let safe = name.replace('\'', "''");
    let script = format!("Remove-NetFirewallRule -DisplayName '{safe}' -ErrorAction Stop");
    run_ps_script(&script)?;
    Ok(())
}

/// Full repair: delete every stale Block rule, then ensure the Allow rule
/// exists. Requests UAC and waits for the user's decision.
///
/// The repair script is written to a temp file and run in an elevated
/// PowerShell spawned via `Start-Process -Verb RunAs`. The script writes a
/// marker file on success so the caller can distinguish "user cancelled UAC"
/// from "script ran but failed".
pub fn repair(program_path: &str) -> Result<(), FirewallError> {
    let blocks = find_stale_blocks().unwrap_or_default();
    let safe_program = program_path.replace('\'', "''");

    let pid = std::process::id();
    let tmp = std::env::temp_dir().join(format!("airlynk-firewall-repair-{pid}.ps1"));
    let marker = std::env::temp_dir().join(format!("airlynk-firewall-repair-{pid}.ok"));
    let marker_str = marker.display().to_string().replace('\'', "''");

    let _ = std::fs::remove_file(&marker);

    let mut script = String::new();
    for name in &blocks {
        let safe = name.replace('\'', "''");
        script.push_str(&format!(
            "Remove-NetFirewallRule -DisplayName '{safe}' -ErrorAction SilentlyContinue\n"
        ));
    }
    script.push_str(&format!(
        "Remove-NetFirewallRule -DisplayName '{ALLOW_RULE_NAME}' -ErrorAction SilentlyContinue\n\
         New-NetFirewallRule -DisplayName '{ALLOW_RULE_NAME}' \
             -Description 'Allows AirLynk to accept incoming file transfers from phones on the local network' \
             -Direction Inbound -Program '{safe_program}' -Protocol TCP -Action Allow \
             -Profile Private,Public -Enabled True\n\
         Set-Content -LiteralPath '{marker_str}' -Value 'ok'\n"
    ));

    std::fs::write(&tmp, &script).map_err(|e| FirewallError::RepairFailed(e.to_string()))?;

    let cmd = format!(
        "Start-Process -FilePath 'powershell' -Verb RunAs -Wait -ArgumentList @('-NoProfile','-ExecutionPolicy','Bypass','-File','{}')",
        tmp.display().to_string().replace('\'', "''")
    );

    let output = Command::new("powershell")
        .args(["-NoProfile", "-Command", &cmd])
        .output()
        .map_err(|e| FirewallError::PsCommand(e.to_string()))?;

    let _ = std::fs::remove_file(&tmp);
    let ok = marker.exists();
    let _ = std::fs::remove_file(&marker);

    if ok {
        Ok(())
    } else if output.status.success() {
        Err(FirewallError::RepairFailed(
            "the elevated repair ran but the rule was not created".into(),
        ))
    } else {
        Err(FirewallError::ElevationRequired)
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Run a PowerShell command and return its stdout (trimmed of trailing CRLF).
fn run_ps_script(script: &str) -> Result<String, FirewallError> {
    let output = Command::new("powershell")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-ExecutionPolicy",
            "Bypass",
            "-Command",
            script,
        ])
        .output()
        .map_err(|e| FirewallError::PsCommand(e.to_string()))?;

    if output.status.success() {
        let mut stdout = String::from_utf8_lossy(&output.stdout).to_string();
        while stdout.ends_with('\n') || stdout.ends_with('\r') {
            stdout.pop();
        }
        Ok(stdout)
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let msg = stderr.trim().to_string();
        if msg.contains("Access is denied") || msg.to_ascii_lowercase().contains("elevat") {
            Err(FirewallError::ElevationRequired)
        } else {
            Err(FirewallError::PsCommand(if msg.is_empty() {
                format!("exit code {}", output.status.code().unwrap_or(-1))
            } else {
                msg
            }))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rule_name_is_stable() {
        assert_eq!(ALLOW_RULE_NAME, "AirLynk (inbound LAN)");
    }

    #[test]
    fn classify_clean_when_allow_present_and_no_blocks() {
        assert_eq!(classify(true, vec![]), FirewallStatus::Clean);
    }

    #[test]
    fn classify_missing_allow_without_blocks() {
        assert_eq!(classify(false, vec![]), FirewallStatus::MissingAllowRule);
    }

    #[test]
    fn classify_stale_blocks_win_even_with_allow_present() {
        // Block beats Allow (FR-29): the presence of an Allow rule must not
        // hide a stale Block rule.
        let status = classify(true, vec!["AirLynk".into()]);
        assert_eq!(
            status,
            FirewallStatus::StaleBlockRules {
                names: vec!["AirLynk".into()]
            }
        );
    }

    #[test]
    fn classify_preserves_all_block_names() {
        let status = classify(false, vec!["A".into(), "B".into(), "C".into()]);
        match status {
            FirewallStatus::StaleBlockRules { names } => assert_eq!(names, vec!["A", "B", "C"]),
            other => panic!("expected StaleBlockRules, got {other:?}"),
        }
    }

    #[test]
    fn program_matches_any_program_rule() {
        assert!(program_matches("", "C:\\apps\\airlynk.exe"));
        assert!(program_matches("   ", "C:\\apps\\airlynk.exe"));
    }

    #[test]
    fn program_matches_is_case_insensitive_and_trimmed() {
        assert!(program_matches(" C:\\X\\A.EXE \r\n", "c:\\x\\a.exe"));
    }

    #[test]
    fn program_matches_rejects_stale_program_path() {
        // The project moved from C:\ to P:\ — the old rule's Program filter
        // no longer covers the running binary and detection must say so.
        assert!(!program_matches(
            "C:\\Users\\hp\\AirLynk\\target\\debug\\airlynk.exe",
            "P:\\AirLynk\\target\\debug\\airlynk.exe",
        ));
    }

    #[test]
    fn program_matches_rejects_unverifiable_exe() {
        assert!(!program_matches("", ""));
        assert!(!program_matches("C:\\apps\\airlynk.exe", "  "));
    }

    // Integration tests that touch PowerShell are environment-dependent.
    // Run explicitly on a Windows machine to check detection + repair.
    #[test]
    #[ignore = "requires a real Windows Firewall; run explicitly on Windows"]
    fn detection_end_to_end() {
        eprintln!("allow rule exists: {}", allow_rule_exists());
        eprintln!("stale blocks: {:?}", find_stale_blocks());
    }

    #[test]
    #[ignore = "triggers UAC; run explicitly on Windows to verify the repair flow"]
    fn repair_end_to_end() {
        let exe = std::env::current_exe().expect("current exe");
        repair(exe.to_str().unwrap()).expect("repair should succeed when elevated");
        assert!(allow_rule_exists());
    }
}
