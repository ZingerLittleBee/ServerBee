import SwiftUI

/// Shows a freshly-minted enrollment code and the agent install command, with
/// copy buttons and a one-time warning. Reused by create / recover / regenerate.
struct EnrollmentResultView: View {
    let issued: AgentLifecycleViewModel.IssuedEnrollment

    @State private var copiedCode = false
    @State private var copiedCommand = false

    var body: some View {
        VStack(spacing: 14) {
            Label(String(localized: "Copy this now — the code is shown only once."),
                  systemImage: "exclamationmark.triangle.fill")
                .font(.caption)
                .foregroundStyle(Color.warningAmber)
                .frame(maxWidth: .infinity, alignment: .leading)

            field(
                title: String(localized: "Enrollment code"),
                value: issued.code,
                copied: copiedCode
            ) {
                UIPasteboard.general.string = issued.code
                copiedCode = true
            }

            field(
                title: String(localized: "Install command"),
                value: issued.installCommand,
                monospaceSmall: true,
                copied: copiedCommand
            ) {
                UIPasteboard.general.string = issued.installCommand
                copiedCommand = true
            }

            Text(String(localized: "Run the install command on the target server. The code expires soon."))
                .font(.caption2)
                .foregroundStyle(.secondary)
                .frame(maxWidth: .infinity, alignment: .leading)
        }
    }

    @ViewBuilder
    private func field(title: String, value: String, monospaceSmall: Bool = false, copied: Bool, copy: @escaping () -> Void) -> some View {
        VStack(alignment: .leading, spacing: 6) {
            Text(title)
                .font(.caption.weight(.medium))
                .foregroundStyle(.secondary)
            Text(value)
                .font(monospaceSmall ? .caption.monospaced() : .callout.monospaced())
                .textSelection(.enabled)
                .frame(maxWidth: .infinity, alignment: .leading)
                .padding(10)
                .background(Color(.secondarySystemBackground))
                .clipShape(RoundedRectangle(cornerRadius: 8))
            Button(action: copy) {
                Label(copied ? String(localized: "Copied") : String(localized: "Copy"),
                      systemImage: copied ? "checkmark" : "doc.on.doc")
                    .font(.caption)
            }
        }
    }
}
