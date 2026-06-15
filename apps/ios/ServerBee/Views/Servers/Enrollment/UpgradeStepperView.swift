import SwiftUI

/// Renders the live state of an agent self-upgrade: a five-node stepper while
/// running, or a terminal banner (succeeded / failed / timeout) once finished.
/// Driven entirely by `UpgradeJobsStore`; no local polling.
struct UpgradeStepperView: View {
    let job: UpgradeJob

    @State private var pulse = false

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            switch job.status {
            case .running:
                runningContent
            case .succeeded:
                terminalRow(
                    icon: "checkmark.circle.fill",
                    tint: .serverOnline,
                    title: String(format: String(localized: "Upgraded to v%@"), job.targetVersion),
                    subtitle: nil
                )
            case .failed:
                terminalRow(
                    icon: "exclamationmark.triangle.fill",
                    tint: .serverOffline,
                    title: String(localized: "Upgrade failed"),
                    subtitle: job.error ?? backupHint
                )
            case .timeout:
                terminalRow(
                    icon: "clock.badge.exclamationmark",
                    tint: .warningAmber,
                    title: String(localized: "Upgrade timed out"),
                    subtitle: backupHint
                )
            }
        }
        .padding(10)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(Color(.systemGray6))
        .clipShape(RoundedRectangle(cornerRadius: 10))
        .onAppear {
            withAnimation(.easeInOut(duration: 0.9).repeatForever(autoreverses: true)) { pulse = true }
        }
    }
}

private extension UpgradeStepperView {
    @ViewBuilder
    var runningContent: some View {
        HStack(spacing: 8) {
            ProgressView().controlSize(.small)
            Text(String(localized: "Upgrading…")).font(.subheadline.weight(.semibold))
            Spacer()
            Text("v\(job.targetVersion)").font(.caption.monospaced()).foregroundStyle(.secondary)
        }
        stepper
        Text(job.stage.label)
            .font(.caption)
            .foregroundStyle(Color.brandAccent)
    }

    var stepper: some View {
        HStack(spacing: 0) {
            ForEach(Array(UpgradeStage.allCases.enumerated()), id: \.element) { index, stage in
                node(for: stage, index: index)
                if index < UpgradeStage.allCases.count - 1 {
                    Rectangle()
                        .fill(index < job.stage.order ? Color.brandAccent : Color(.systemGray4))
                        .frame(height: 2)
                        .frame(maxWidth: .infinity)
                }
            }
        }
    }

    @ViewBuilder
    func node(for stage: UpgradeStage, index: Int) -> some View {
        let active = index <= job.stage.order
        let current = index == job.stage.order
        Image(systemName: stage.systemImage)
            .font(.system(size: 13, weight: .semibold))
            .foregroundStyle(active ? Color.white : Color.secondary)
            .frame(width: 30, height: 30)
            .background(Circle().fill(active ? AnyShapeStyle(Color.brandAccent) : AnyShapeStyle(Color(.systemGray5))))
            .overlay {
                if current {
                    Circle()
                        .stroke(Color.brandAccent.opacity(0.4), lineWidth: 3)
                        .scaleEffect(pulse ? 1.35 : 1.0)
                        .opacity(pulse ? 0.0 : 1.0)
                }
            }
            .accessibilityLabel(stage.label)
    }

    func terminalRow(icon: String, tint: Color, title: String, subtitle: String?) -> some View {
        HStack(alignment: .top, spacing: 10) {
            Image(systemName: icon).foregroundStyle(tint)
            VStack(alignment: .leading, spacing: 2) {
                Text(title).font(.subheadline.weight(.semibold))
                if let subtitle {
                    Text(subtitle).font(.caption).foregroundStyle(.secondary)
                }
            }
            Spacer(minLength: 0)
        }
    }

    var backupHint: String? {
        guard let path = job.backupPath else { return nil }
        return String(format: String(localized: "Backup at %@"), path)
    }
}
