import SwiftUI

struct SetupView: View {
    @Bindable var config: Config
    let done: () -> Void

    var body: some View {
        NavigationStack {
            Form {
                Section {
                    TextField("192.168.1.50", text: $config.address)
                        .textContentType(.URL)
                        .autocorrectionDisabled()
                } header: {
                    Text("Display address")
                } footer: {
                    Text("The address you open the web app at.")
                }
                Section {
                    SecureField("Password", text: $config.password)
                } header: {
                    Text("Web app password")
                } footer: {
                    Text("Leave it empty if the web app has none.")
                }
                Button("Connect", action: done)
                    .disabled(!config.isSet)
            }
            .navigationTitle("Coverussi")
        }
    }
}
