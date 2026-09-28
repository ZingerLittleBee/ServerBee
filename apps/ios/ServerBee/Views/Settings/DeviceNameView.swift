import SwiftUI

/// Settings → Device Name: edits the name this installation registers under
/// (listed in Devices). The server has no rename endpoint, so the name is sent
/// with the next sign-in; the footer says so instead of implying a live rename.
struct DeviceNameView: View {
    /// Only the custom name. The generated default stays the placeholder, so
    /// opening and leaving the screen never pins it as a custom name.
    @AppStorage(DeviceNameProvider.storageKey) private var customName = ""
    @State private var draft = ""
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
        .onAppear {
            draft = customName
            focused = true
        }
        .onDisappear(perform: commit)
    }

    private func commit() {
        DeviceNameProvider.set(draft)
        // Mirror what was stored: trimmed, and empty shows the default placeholder.
        draft = draft.trimmingCharacters(in: .whitespacesAndNewlines)
    }
}
