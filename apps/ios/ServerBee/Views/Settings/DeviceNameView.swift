import SwiftUI

/// Settings → Device Name: edits the name this installation registers under
/// (listed in Devices). The server has no rename endpoint, so the name is sent
/// with the next sign-in; the footer says so instead of implying a live rename.
struct DeviceNameView: View {
    @State private var draft: String = DeviceNameProvider.current()
    @FocusState private var focused: Bool

    var body: some View {
        List {
            Section {
                TextField(DeviceNameProvider.defaultName(), text: $draft)
                    .textInputAutocapitalization(.words)
                    .autocorrectionDisabled()
                    .submitLabel(.done)
                    .focused($focused)
                    .onSubmit(commit)
            } footer: {
                Text(String(localized: "Shown in Devices on the server. A new name applies the next time you sign in on this device. Leave it empty to use the default name."))
            }
        }
        .navigationTitle(String(localized: "Device Name"))
        .navigationBarTitleDisplayMode(.inline)
        .onAppear { focused = true }
        .onDisappear(perform: commit)
    }

    private func commit() {
        DeviceNameProvider.set(draft)
        // Refresh so an empty submission shows the generated default.
        draft = DeviceNameProvider.current()
    }
}
