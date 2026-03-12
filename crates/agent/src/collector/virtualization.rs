/// Detect the virtualization type of the current system.
/// Returns a short identifier like "kvm", "vmware", "docker", etc.
pub fn detect() -> Option<String> {
    #[cfg(target_os = "linux")]
    {
        detect_linux()
    }

    #[cfg(not(target_os = "linux"))]
    {
        None
    }
}

#[cfg(target_os = "linux")]
fn detect_linux() -> Option<String> {
    // Try DMI product_name first
    if let Some(virt) = detect_from_dmi("/sys/class/dmi/id/product_name") {
        return Some(virt);
    }

    // Try DMI sys_vendor
    if let Some(virt) = detect_from_dmi("/sys/class/dmi/id/sys_vendor") {
        return Some(virt);
    }

    // Check container indicators
    if std::path::Path::new("/.dockerenv").exists() {
        return Some("docker".to_string());
    }

    if let Ok(cgroup) = std::fs::read_to_string("/proc/1/cgroup") {
        if cgroup.contains("docker") {
            return Some("docker".to_string());
        }
        if cgroup.contains("lxc") {
            return Some("lxc".to_string());
        }
    }

    // Fallback: try systemd-detect-virt
    if let Ok(output) = std::process::Command::new("systemd-detect-virt").output() {
        if output.status.success() {
            let result = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !result.is_empty() && result != "none" {
                return Some(result);
            }
        }
    }

    None
}

#[cfg(target_os = "linux")]
fn detect_from_dmi(path: &str) -> Option<String> {
    let content = std::fs::read_to_string(path).ok()?;
    let content = content.trim();
    map_vendor_to_virt(content)
}

#[cfg(target_os = "linux")]
fn map_vendor_to_virt(value: &str) -> Option<String> {
    let upper = value.to_uppercase();

    if upper.contains("QEMU") || upper.contains("KVM") {
        return Some("kvm".to_string());
    }
    if upper.contains("VMWARE") {
        return Some("vmware".to_string());
    }
    if upper.contains("VIRTUALBOX") || upper.contains("VBOX") {
        return Some("virtualbox".to_string());
    }
    if upper.contains("MICROSOFT") || upper.contains("HYPER-V") {
        return Some("hyperv".to_string());
    }
    if upper.contains("XEN") {
        return Some("xen".to_string());
    }
    if upper.contains("DOCKER") {
        return Some("docker".to_string());
    }
    if upper.contains("LXC") || upper.contains("LINUX CONTAINER") {
        return Some("lxc".to_string());
    }
    if upper.contains("PARALLELS") {
        return Some("parallels".to_string());
    }
    if upper.contains("BOCHS") {
        return Some("bochs".to_string());
    }
    if upper.contains("OPENSTACK") {
        return Some("openstack".to_string());
    }

    None
}
