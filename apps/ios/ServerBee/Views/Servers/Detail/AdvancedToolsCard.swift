import SwiftUI

/// Surfaces the high-risk "advanced" capabilities a server supports — terminal,
/// command execution / scheduled tasks, and file management — with an honest
/// note that their full interactive experience lives in the web dashboard.
///
/// These features need a desktop-class UI (PTY emulation, file transfers,
/// command scheduling) to be used safely, so the mobile app deliberately
/// surfaces them as informational entries rather than half-built controls —
/// no dead links, with a permission/audit reminder.
struct AdvancedToolsCard: View {
    let capabilities: CapabilitySet

    private struct Tool: Identifiable {
        let id: String
        let capability: Capability
        let title: String
        let note: String
    }

    private var tools: [Tool] {
        [
            Tool(id: "terminal", capability: .terminal,
                 title: String(localized: "Terminal"),
                 note: String(localized: "Interactive SSH-style shell. Use the web dashboard for a full PTY.")),
            Tool(id: "exec", capability: .exec,
                 title: String(localized: "Commands & tasks"),
                 note: String(localized: "Run and schedule shell commands from the web dashboard.")),
            Tool(id: "file", capability: .file,
                 title: String(localized: "File manager"),
                 note: String(localized: "Browse and transfer files from the web dashboard."))
        ]
    }

    private var visibleTools: [Tool] {
        #if DEBUG
        // Visual-verification hook: the shared demo has no server configured for
        // terminal/exec/file, so allow forcing the card into view for screenshots.
        if UITestSupport.autoPresent == "advanced-tools" { return tools }
        #endif
        return tools.filter { capabilities.isConfigured($0.capability) }
    }

    private func isAvailable(_ tool: Tool) -> Bool {
        #if DEBUG
        if UITestSupport.autoPresent == "advanced-tools" {
            // Representative mix: terminal + exec live, file currently offline.
            return tool.capability != .file
        }
        #endif
        return capabilities.isEnabled(tool.capability)
    }

    var body: some View {
        if !visibleTools.isEmpty {
            SectionCard(String(localized: "Advanced"), systemImage: "wrench.and.screwdriver") {
                VStack(spacing: 0) {
                    ForEach(Array(visibleTools.enumerated()), id: \.element.id) { index, tool in
                        row(tool)
                        if index != visibleTools.count - 1 { Divider() }
                    }
                    Divider()
                    Label(String(localized: "These actions run on the server and are audited. Open the web dashboard for the full experience."),
                          systemImage: "info.circle")
                        .font(.caption2)
                        .foregroundStyle(.secondary)
                        .padding(.top, 8)
                        .frame(maxWidth: .infinity, alignment: .leading)
                }
            }
        }
    }

    private func row(_ tool: Tool) -> some View {
        let available = isAvailable(tool)
        return HStack(alignment: .top, spacing: 10) {
            Image(systemName: tool.capability.systemImage)
                .frame(width: 24)
                .foregroundStyle(available ? Color.brandAccent : Color.secondary)
            VStack(alignment: .leading, spacing: 2) {
                Text(tool.title).font(.subheadline.weight(.medium))
                Text(tool.note).font(.caption).foregroundStyle(.secondary)
            }
            Spacer(minLength: 8)
            Text(available ? String(localized: "Available") : String(localized: "Offline"))
                .font(.caption2.weight(.medium))
                .foregroundStyle(available ? Color.serverOnline : Color.secondary)
                .padding(.horizontal, 8)
                .padding(.vertical, 3)
                .background((available ? Color.serverOnline : Color.secondary).opacity(0.14))
                .clipShape(Capsule())
        }
        .padding(.vertical, 10)
    }
}
