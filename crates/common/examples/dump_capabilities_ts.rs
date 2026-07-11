//! Prints `apps/web/src/lib/capability-bits.generated.ts` from the
//! [`ALL_CAPABILITIES`] metadata table, so the web bit definitions cannot
//! drift from Rust. Regenerate via `bun run generate:capabilities` in
//! `apps/web` (same pipeline shape as `dump_openapi` → api-types).

use serverbee_common::constants::{ALL_CAPABILITIES, CAP_DEFAULT};

fn main() {
    let derived_default: u32 = ALL_CAPABILITIES
        .iter()
        .filter(|m| m.default_enabled)
        .map(|m| m.bit)
        .fold(0, |acc, bit| acc | bit);
    assert_eq!(
        derived_default, CAP_DEFAULT,
        "CAP_DEFAULT must equal the OR of default_enabled bits"
    );

    println!("// AUTO-GENERATED from crates/common/src/constants.rs (ALL_CAPABILITIES).");
    println!("// Regenerate with `bun run generate:capabilities` — do not edit by hand.");
    println!();
    for meta in ALL_CAPABILITIES {
        println!(
            "export const CAP_{} = {}",
            meta.key.to_uppercase(),
            meta.bit
        );
    }
    println!();
    let default_keys = ALL_CAPABILITIES
        .iter()
        .filter(|m| m.default_enabled)
        .map(|m| m.key)
        .collect::<Vec<_>>()
        .join(" + ");
    println!("// The OR of every default_enabled bit: {default_keys}.");
    println!("export const CAP_DEFAULT = {CAP_DEFAULT}");
    println!();
    println!("export const CAPABILITIES = [");
    let entries = ALL_CAPABILITIES
        .iter()
        .map(|meta| {
            format!(
                "  {{ bit: CAP_{}, key: '{}', labelKey: 'cap_{}', risk: '{}' }}",
                meta.key.to_uppercase(),
                meta.key,
                meta.key,
                meta.risk_level
            )
        })
        .collect::<Vec<_>>()
        .join(",\n");
    println!("{entries}");
    println!("] as const");
    println!();
    println!("export type CapabilityRisk = (typeof CAPABILITIES)[number]['risk']");
}
