use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Mutex;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serverbee_common::constants::UPGRADE_PROBE_ARG;
#[cfg(test)]
use serverbee_common::constants::is_upgrade_probe;

pub const PARENT_WATCHDOG_ENV: &str = "SERVERBEE_UPGRADE_PARENT_WATCHDOG";
const STARTUP_HEALTH_TIMEOUT: Duration = Duration::from_secs(90);
const STARTUP_STABILITY_WINDOW: Duration = Duration::from_secs(5);
const CANDIDATE_PROBE_TIMEOUT: Duration = Duration::from_secs(10);
static UPGRADE_STATE_LOCK: Mutex<()> = Mutex::new(());

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum TransactionPhase {
    Pending,
    Booting,
    Healthy,
    RolledBack,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct UpgradeTransaction {
    target_version: String,
    #[serde(default)]
    job_id: Option<String>,
    phase: TransactionPhase,
    #[serde(default)]
    error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecoveryReport {
    pub target_version: String,
    pub job_id: Option<String>,
    pub error: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StartupDisposition {
    Normal,
    Trial,
    Recovered,
    RestartAfterRollback(PathBuf),
}

pub async fn verify_candidate_version(candidate: &Path, expected: &str) -> anyhow::Result<()> {
    let mut command = tokio::process::Command::new(candidate);
    command
        .arg(UPGRADE_PROBE_ARG)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);

    let output = tokio::time::timeout(CANDIDATE_PROBE_TIMEOUT, command.output())
        .await
        .map_err(|_| anyhow::anyhow!("candidate probe timed out after 10s"))??;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!(
            "candidate probe exited with {}: {}",
            output.status,
            stderr.trim()
        );
    }

    let reported = String::from_utf8_lossy(&output.stdout);
    if normalize_version(reported.trim()) != normalize_version(expected) {
        anyhow::bail!(
            "candidate reports version {}, expected {}",
            reported.trim(),
            expected
        );
    }
    Ok(())
}

pub fn install_candidate(
    current_exe: &Path,
    candidate: &Path,
    target_version: &str,
    job_id: Option<String>,
) -> anyhow::Result<PathBuf> {
    let _guard = state_lock()?;
    install_candidate_at(current_exe, candidate, target_version, job_id)
}

pub fn prepare_startup() -> anyhow::Result<StartupDisposition> {
    let current_exe = std::env::current_exe()?;
    let _guard = state_lock()?;
    prepare_startup_at(&current_exe, serverbee_common::constants::VERSION)
}

pub fn commit_startup_trial() -> anyhow::Result<bool> {
    let current_exe = std::env::current_exe()?;
    let marked_healthy = {
        let _guard = state_lock()?;
        commit_startup_trial_at(&current_exe, serverbee_common::constants::VERSION)?
    };
    if marked_healthy {
        std::thread::spawn(move || {
            std::thread::sleep(STARTUP_STABILITY_WINDOW);
            let result = (|| -> anyhow::Result<()> {
                let _guard = state_lock()?;
                finalize_startup_trial_at(&current_exe, serverbee_common::constants::VERSION)
            })();
            if let Err(error) = result {
                eprintln!("Failed to finalize Agent upgrade stability window: {error}");
            }
        });
    }
    Ok(marked_healthy)
}

pub fn read_recovery_report() -> anyhow::Result<Option<RecoveryReport>> {
    let current_exe = std::env::current_exe()?;
    let _guard = state_lock()?;
    read_recovery_report_at(&current_exe, serverbee_common::constants::VERSION)
}

pub fn acknowledge_recovery_report() -> anyhow::Result<()> {
    let current_exe = std::env::current_exe()?;
    let _guard = state_lock()?;
    let state_path = state_path(&current_exe);
    if state_path.exists() {
        std::fs::remove_file(&state_path)?;
        sync_parent(&state_path)?;
    }
    Ok(())
}

pub fn rollback_failed_candidate(
    current_exe: &Path,
    target_version: &str,
    job_id: Option<String>,
    error: String,
) -> anyhow::Result<()> {
    let _guard = state_lock()?;
    rollback_failed_candidate_at(current_exe, target_version, job_id, error)
}

fn rollback_failed_candidate_at(
    current_exe: &Path,
    target_version: &str,
    job_id: Option<String>,
    error: String,
) -> anyhow::Result<()> {
    let mut state = read_state(current_exe)?.unwrap_or_else(|| UpgradeTransaction {
        target_version: normalize_version(target_version).to_string(),
        job_id,
        phase: TransactionPhase::Booting,
        error: None,
    });
    rollback_files(current_exe)?;
    state.phase = TransactionPhase::RolledBack;
    state.error = Some(error);
    write_state(current_exe, &state)
}

pub fn trial_is_active(current_exe: &Path) -> anyhow::Result<bool> {
    let _guard = state_lock()?;
    trial_is_active_at(current_exe)
}

fn trial_is_active_at(current_exe: &Path) -> anyhow::Result<bool> {
    Ok(read_state(current_exe)?.is_some_and(|state| {
        matches!(
            state.phase,
            TransactionPhase::Pending | TransactionPhase::Booting | TransactionPhase::Healthy
        )
    }))
}

pub fn has_parent_watchdog() -> bool {
    std::env::var_os(PARENT_WATCHDOG_ENV).is_some_and(|value| !value.is_empty())
}

pub fn startup_health_timeout() -> Duration {
    STARTUP_HEALTH_TIMEOUT
}

pub fn is_supervised() -> bool {
    ["SERVERBEE_SUPERVISED", "INVOCATION_ID", "RC_SVCNAME"]
        .into_iter()
        .any(|name| std::env::var_os(name).is_some_and(|value| !value.is_empty()))
}

pub fn restart_restored_binary(restored_exe: &Path) -> anyhow::Result<()> {
    if is_supervised() {
        std::process::exit(1);
    }

    let args: Vec<String> = std::env::args().skip(1).collect();
    std::process::Command::new(restored_exe)
        .args(args)
        .spawn()?;
    std::process::exit(1);
}

pub fn start_trial_watchdog() {
    std::thread::spawn(|| {
        std::thread::sleep(STARTUP_HEALTH_TIMEOUT);
        let result = (|| -> anyhow::Result<Option<PathBuf>> {
            let current_exe = std::env::current_exe()?;
            let _guard = state_lock()?;
            let rolled_back =
                rollback_unhealthy_trial_at(&current_exe, serverbee_common::constants::VERSION)?;
            Ok(rolled_back.then_some(current_exe))
        })();

        match result {
            Ok(None) => {}
            Ok(Some(restored_exe)) => {
                eprintln!(
                    "Agent upgrade did not become healthy within 90s; restored the previous binary"
                );
                if let Err(error) = restart_restored_binary(&restored_exe) {
                    eprintln!("Failed to restart restored Agent binary: {error}");
                    std::process::exit(1);
                }
            }
            Err(error) => {
                eprintln!("Failed to roll back unhealthy Agent upgrade: {error}");
                std::process::exit(1);
            }
        }
    });
}

fn state_lock() -> anyhow::Result<std::sync::MutexGuard<'static, ()>> {
    UPGRADE_STATE_LOCK
        .lock()
        .map_err(|_| anyhow::anyhow!("upgrade state lock poisoned"))
}

fn normalize_version(version: &str) -> &str {
    version.trim_start_matches('v')
}

fn state_path(current_exe: &Path) -> PathBuf {
    current_exe.with_extension("upgrade-state.json")
}

fn state_temp_path(current_exe: &Path) -> PathBuf {
    current_exe.with_extension("upgrade-state.tmp")
}

fn backup_path(current_exe: &Path) -> PathBuf {
    current_exe.with_extension("bak")
}

fn failed_path(current_exe: &Path) -> PathBuf {
    current_exe.with_extension("failed")
}

fn install_candidate_at(
    current_exe: &Path,
    candidate: &Path,
    target_version: &str,
    job_id: Option<String>,
) -> anyhow::Result<PathBuf> {
    if !current_exe.is_file() {
        anyhow::bail!(
            "current Agent binary does not exist: {}",
            current_exe.display()
        );
    }
    if !candidate.is_file() {
        anyhow::bail!(
            "candidate Agent binary does not exist: {}",
            candidate.display()
        );
    }

    let state = UpgradeTransaction {
        target_version: normalize_version(target_version).to_string(),
        job_id,
        phase: TransactionPhase::Pending,
        error: None,
    };
    write_state(current_exe, &state)?;

    let backup = backup_path(current_exe);
    if let Err(error) = copy_and_replace(current_exe, &backup) {
        let _ = remove_state(current_exe);
        return Err(error);
    }

    if let Err(error) = std::fs::rename(candidate, current_exe) {
        let _ = remove_state(current_exe);
        return Err(error.into());
    }
    if let Err(sync_error) = sync_parent(current_exe) {
        let rollback_result = rollback_files(current_exe);
        let _ = remove_state(current_exe);
        if let Err(rollback_error) = rollback_result {
            anyhow::bail!(
                "installed candidate but failed to sync its directory ({sync_error}); rollback also failed: {rollback_error}"
            );
        }
        return Err(sync_error);
    }
    Ok(backup)
}

fn prepare_startup_at(
    current_exe: &Path,
    running_version: &str,
) -> anyhow::Result<StartupDisposition> {
    let Some(mut state) = read_state(current_exe)? else {
        return Ok(StartupDisposition::Normal);
    };

    let is_target = normalize_version(running_version) == normalize_version(&state.target_version);
    if !is_target {
        if state.phase != TransactionPhase::RolledBack {
            state.phase = TransactionPhase::RolledBack;
            state.error = Some(
                "upgrade was interrupted before the candidate became healthy; previous binary is active"
                    .to_string(),
            );
            write_state(current_exe, &state)?;
        }
        return Ok(StartupDisposition::Recovered);
    }

    match state.phase {
        TransactionPhase::Pending => {
            state.phase = TransactionPhase::Booting;
            write_state(current_exe, &state)?;
            Ok(StartupDisposition::Trial)
        }
        TransactionPhase::Booting | TransactionPhase::Healthy => {
            rollback_files(current_exe)?;
            state.phase = TransactionPhase::RolledBack;
            state.error = Some(
                "candidate Agent exited before completing startup health confirmation; previous binary restored"
                    .to_string(),
            );
            write_state(current_exe, &state)?;
            Ok(StartupDisposition::RestartAfterRollback(
                current_exe.to_path_buf(),
            ))
        }
        TransactionPhase::RolledBack => anyhow::bail!(
            "rollback state still points at candidate version {}",
            state.target_version
        ),
    }
}

fn commit_startup_trial_at(current_exe: &Path, running_version: &str) -> anyhow::Result<bool> {
    let Some(mut state) = read_state(current_exe)? else {
        return Ok(false);
    };
    if state.phase != TransactionPhase::Booting
        || normalize_version(running_version) != normalize_version(&state.target_version)
    {
        return Ok(false);
    }
    state.phase = TransactionPhase::Healthy;
    write_state(current_exe, &state)?;
    Ok(true)
}

fn finalize_startup_trial_at(current_exe: &Path, running_version: &str) -> anyhow::Result<()> {
    let Some(state) = read_state(current_exe)? else {
        return Ok(());
    };
    if state.phase == TransactionPhase::Healthy
        && normalize_version(running_version) == normalize_version(&state.target_version)
    {
        remove_state(current_exe)?;
    }
    Ok(())
}

fn rollback_unhealthy_trial_at(current_exe: &Path, running_version: &str) -> anyhow::Result<bool> {
    let Some(mut state) = read_state(current_exe)? else {
        return Ok(false);
    };
    if !matches!(
        state.phase,
        TransactionPhase::Booting | TransactionPhase::Healthy
    ) || normalize_version(running_version) != normalize_version(&state.target_version)
    {
        return Ok(false);
    }

    rollback_files(current_exe)?;
    state.phase = TransactionPhase::RolledBack;
    state.error = Some(
        "candidate Agent did not complete startup health confirmation within 90s; previous binary restored"
            .to_string(),
    );
    write_state(current_exe, &state)?;
    Ok(true)
}

fn read_recovery_report_at(
    current_exe: &Path,
    running_version: &str,
) -> anyhow::Result<Option<RecoveryReport>> {
    let Some(state) = read_state(current_exe)? else {
        return Ok(None);
    };
    if state.phase != TransactionPhase::RolledBack
        || normalize_version(running_version) == normalize_version(&state.target_version)
    {
        return Ok(None);
    }
    Ok(Some(RecoveryReport {
        target_version: state.target_version,
        job_id: state.job_id,
        error: state
            .error
            .unwrap_or_else(|| "candidate Agent rolled back before becoming healthy".to_string()),
    }))
}

fn rollback_files(current_exe: &Path) -> anyhow::Result<()> {
    let backup = backup_path(current_exe);
    if !backup.is_file() {
        anyhow::bail!("upgrade backup is missing: {}", backup.display());
    }

    let failed = failed_path(current_exe);
    if failed.exists() {
        std::fs::remove_file(&failed)?;
    }
    std::fs::rename(current_exe, &failed)?;
    if let Err(restore_error) = std::fs::rename(&backup, current_exe) {
        if let Err(revert_error) = std::fs::rename(&failed, current_exe) {
            anyhow::bail!(
                "failed to restore backup ({restore_error}); also failed to restore candidate ({revert_error})"
            );
        }
        return Err(restore_error.into());
    }
    sync_parent(current_exe)
}

fn copy_and_replace(source: &Path, destination: &Path) -> anyhow::Result<()> {
    let temp = destination.with_extension("bak.tmp");
    match std::fs::remove_file(&temp) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }
    std::fs::copy(source, &temp)?;
    std::fs::OpenOptions::new()
        .read(true)
        .open(&temp)?
        .sync_all()?;
    #[cfg(windows)]
    if destination.exists() {
        std::fs::remove_file(destination)?;
    }
    std::fs::rename(&temp, destination)?;
    sync_parent(destination)
}

fn read_state(current_exe: &Path) -> anyhow::Result<Option<UpgradeTransaction>> {
    let path = state_path(current_exe);
    match std::fs::read(&path) {
        Ok(bytes) => Ok(Some(serde_json::from_slice(&bytes).map_err(|error| {
            anyhow::anyhow!("invalid upgrade transaction {}: {error}", path.display())
        })?)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.into()),
    }
}

fn write_state(current_exe: &Path, state: &UpgradeTransaction) -> anyhow::Result<()> {
    use std::io::Write;

    let path = state_path(current_exe);
    let temp = state_temp_path(current_exe);
    let bytes = serde_json::to_vec(state)?;
    match std::fs::remove_file(&temp) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }
    {
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp)?;
        file.write_all(&bytes)?;
        file.sync_all()?;
    }
    #[cfg(windows)]
    if path.exists() {
        std::fs::remove_file(&path)?;
    }
    std::fs::rename(&temp, &path)?;
    sync_parent(&path)
}

fn remove_state(current_exe: &Path) -> anyhow::Result<()> {
    let path = state_path(current_exe);
    match std::fs::remove_file(&path) {
        Ok(()) => sync_parent(&path),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
    }
}

fn sync_parent(path: &Path) -> anyhow::Result<()> {
    #[cfg(unix)]
    if let Some(parent) = path.parent()
        && let Err(error) = std::fs::File::open(parent)?.sync_all()
    {
        let unsupported = error
            .raw_os_error()
            .is_some_and(|code| code == libc::EINVAL || code == libc::ENOTSUP);
        if !unsupported {
            return Err(error.into());
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(path: &Path, contents: &str) {
        std::fs::write(path, contents).expect("write fixture");
    }

    #[test]
    fn probe_argument_is_detected_anywhere() {
        assert!(is_upgrade_probe(["agent", UPGRADE_PROBE_ARG]));
        assert!(is_upgrade_probe([UPGRADE_PROBE_ARG, "ignored"]));
        assert!(!is_upgrade_probe(["agent", "--version"]));
    }

    #[test]
    fn healthy_trial_commits_and_keeps_backup() {
        let temp = tempfile::TempDir::new().expect("temp dir");
        let current = temp.path().join("agent");
        let candidate = temp.path().join("agent.new");
        write(&current, "old");
        write(&candidate, "new");

        let backup = install_candidate_at(&current, &candidate, "2.0.0", Some("job-1".to_string()))
            .expect("install candidate");
        assert_eq!(std::fs::read_to_string(&current).expect("current"), "new");
        assert_eq!(std::fs::read_to_string(&backup).expect("backup"), "old");
        assert!(trial_is_active_at(&current).expect("pending trial"));
        assert_eq!(
            prepare_startup_at(&current, "2.0.0").expect("prepare trial"),
            StartupDisposition::Trial
        );
        assert!(commit_startup_trial_at(&current, "2.0.0").expect("commit trial"));
        assert!(trial_is_active_at(&current).expect("stability window"));
        finalize_startup_trial_at(&current, "2.0.0").expect("finalize trial");
        assert!(!state_path(&current).exists());
        assert!(!trial_is_active_at(&current).expect("committed trial"));
        assert_eq!(std::fs::read_to_string(&backup).expect("backup"), "old");
    }

    #[test]
    fn second_unhealthy_start_restores_previous_binary_and_reports_failure() {
        let temp = tempfile::TempDir::new().expect("temp dir");
        let current = temp.path().join("agent");
        let candidate = temp.path().join("agent.new");
        write(&current, "old");
        write(&candidate, "new");
        install_candidate_at(&current, &candidate, "2.0.0", Some("job-2".to_string()))
            .expect("install candidate");
        assert_eq!(
            prepare_startup_at(&current, "2.0.0").expect("first start"),
            StartupDisposition::Trial
        );
        assert_eq!(
            prepare_startup_at(&current, "2.0.0").expect("second start"),
            StartupDisposition::RestartAfterRollback(current.clone())
        );
        assert_eq!(std::fs::read_to_string(&current).expect("restored"), "old");
        assert_eq!(
            prepare_startup_at(&current, "1.0.0").expect("restored startup"),
            StartupDisposition::Recovered
        );
        let report = read_recovery_report_at(&current, "1.0.0")
            .expect("read report")
            .expect("rollback report");
        assert_eq!(report.job_id.as_deref(), Some("job-2"));
        assert_eq!(report.target_version, "2.0.0");
        assert!(report.error.contains("previous binary restored"));
        assert_eq!(
            std::fs::read_to_string(failed_path(&current)).expect("failed candidate"),
            "new"
        );
    }

    #[test]
    fn watchdog_rolls_back_only_an_uncommitted_matching_trial() {
        let temp = tempfile::TempDir::new().expect("temp dir");
        let current = temp.path().join("agent");
        let candidate = temp.path().join("agent.new");
        write(&current, "old");
        write(&candidate, "new");
        install_candidate_at(&current, &candidate, "2.0.0", None).expect("install candidate");
        prepare_startup_at(&current, "2.0.0").expect("prepare trial");

        assert!(rollback_unhealthy_trial_at(&current, "2.0.0").expect("watchdog rollback"));
        assert!(!rollback_unhealthy_trial_at(&current, "1.0.0").expect("second check"));
        assert_eq!(std::fs::read_to_string(&current).expect("restored"), "old");
    }

    #[test]
    fn interrupted_install_is_reported_by_the_previous_version() {
        let temp = tempfile::TempDir::new().expect("temp dir");
        let current = temp.path().join("agent");
        write(&current, "old");
        write_state(
            &current,
            &UpgradeTransaction {
                target_version: "2.0.0".to_string(),
                job_id: Some("job-3".to_string()),
                phase: TransactionPhase::Pending,
                error: None,
            },
        )
        .expect("write state");

        assert_eq!(
            prepare_startup_at(&current, "1.0.0").expect("recover"),
            StartupDisposition::Recovered
        );
        let report = read_recovery_report_at(&current, "1.0.0")
            .expect("read report")
            .expect("report");
        assert!(report.error.contains("interrupted"));
    }

    #[test]
    fn parent_watchdog_rollback_restores_and_persists_recovery_report() {
        let temp = tempfile::TempDir::new().expect("temp dir");
        let current = temp.path().join("agent");
        let candidate = temp.path().join("agent.new");
        write(&current, "old");
        write(&candidate, "new");
        install_candidate_at(
            &current,
            &candidate,
            "2.0.0",
            Some("job-parent".to_string()),
        )
        .expect("install candidate");

        rollback_failed_candidate_at(
            &current,
            "2.0.0",
            Some("job-parent".to_string()),
            "candidate exited; previous Agent binary restored".to_string(),
        )
        .expect("parent rollback");

        assert_eq!(std::fs::read_to_string(&current).expect("restored"), "old");
        let report = read_recovery_report_at(&current, "1.0.0")
            .expect("read report")
            .expect("recovery report");
        assert_eq!(report.job_id.as_deref(), Some("job-parent"));
        assert!(report.error.contains("candidate exited"));
    }

    #[cfg(unix)]
    #[test]
    fn backup_replacement_does_not_follow_an_existing_symlink() {
        let temp = tempfile::TempDir::new().expect("temp dir");
        let current = temp.path().join("agent");
        let candidate = temp.path().join("agent.new");
        let victim = temp.path().join("victim");
        write(&current, "old");
        write(&candidate, "new");
        write(&victim, "untouched");
        std::os::unix::fs::symlink(&victim, backup_path(&current)).expect("backup symlink");

        let backup =
            install_candidate_at(&current, &candidate, "2.0.0", None).expect("install candidate");

        assert_eq!(
            std::fs::read_to_string(&victim).expect("victim"),
            "untouched"
        );
        assert_eq!(std::fs::read_to_string(&backup).expect("backup"), "old");
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn candidate_probe_checks_reported_version() {
        use std::os::unix::fs::PermissionsExt;

        let temp = tempfile::TempDir::new().expect("temp dir");
        let candidate = temp.path().join("candidate");
        write(&candidate, "#!/bin/sh\nprintf '2.0.0\\n'\n");
        std::fs::set_permissions(&candidate, std::fs::Permissions::from_mode(0o755))
            .expect("candidate permissions");

        verify_candidate_version(&candidate, "2.0.0")
            .await
            .expect("matching version");
        let error = verify_candidate_version(&candidate, "2.0.1")
            .await
            .expect_err("mismatch should fail");
        assert!(error.to_string().contains("expected 2.0.1"));
    }
}
