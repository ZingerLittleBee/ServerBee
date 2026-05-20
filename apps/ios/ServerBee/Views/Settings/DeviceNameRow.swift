import SwiftUI

struct DeviceNameRow: View {
    @State private var draft: String = DeviceNameProvider.current()
    @FocusState private var focused: Bool

    var body: some View {
        HStack {
            Text(String(localized: "Device Name"))
                .font(.body)
            Spacer()
            TextField(
                DeviceNameProvider.defaultName(),
                text: $draft
            )
            .multilineTextAlignment(.trailing)
            .textInputAutocapitalization(.words)
            .autocorrectionDisabled()
            .focused($focused)
            .submitLabel(.done)
            .onSubmit { commit() }
            .onChange(of: focused) { _, isFocused in
                if !isFocused { commit() }
            }
        }
    }

    private func commit() {
        DeviceNameProvider.set(draft)
        // Refresh draft so an empty submission shows the auto default.
        draft = DeviceNameProvider.current()
    }
}
