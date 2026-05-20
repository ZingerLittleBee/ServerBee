import AVFoundation
import SwiftUI
import UIKit

struct QRScannerView: UIViewControllerRepresentable {
    let onScanned: (String, String) -> Void
    @Environment(\.dismiss) private var dismiss

    func makeUIViewController(context: Context) -> QRScannerViewController {
        let controller = QRScannerViewController()
        controller.onScanned = { serverUrl, code in
            onScanned(serverUrl, code)
        }
        controller.onDismiss = {
            dismiss()
        }
        return controller
    }

    func updateUIViewController(_ uiViewController: QRScannerViewController, context: Context) {}
}

// MARK: - QR Scanner View Controller

final class QRScannerViewController: UIViewController, @preconcurrency AVCaptureMetadataOutputObjectsDelegate {
    var onScanned: ((String, String) -> Void)?
    var onDismiss: (() -> Void)?

    /// Serial queue that owns ALL `AVCaptureSession` mutations. Per Apple's
    /// AVFoundation docs, `beginConfiguration()`, `addInput`, `addOutput`,
    /// `startRunning()`, and `stopRunning()` must not be interleaved across
    /// threads. Routing everything through a single serial queue gives us
    /// that ordering guarantee without `nonisolated(unsafe)`.
    private let captureQueue = DispatchQueue(label: "com.serverbee.capture", qos: .userInitiated)
    private let captureSession = AVCaptureSession()
    private var previewLayer: AVCaptureVideoPreviewLayer?
    private var hasScanned = false
    private var isConfigured = false

    /// Permission state reflected by the UI. Updated on the main actor so the
    /// view controller can swap to a "denied" panel.
    enum PermissionState: Equatable {
        case unknown
        case authorized
        case denied
    }

    private(set) var permissionState: PermissionState = .unknown

    override func viewDidLoad() {
        super.viewDidLoad()
        view.backgroundColor = .black
        setupDismissButton()
        requestCameraAndStart()
    }

    override func viewDidLayoutSubviews() {
        super.viewDidLayoutSubviews()
        previewLayer?.frame = view.bounds
    }

    override func viewWillDisappear(_ animated: Bool) {
        super.viewWillDisappear(animated)
        stopSession()
    }

    // MARK: - Permission + start

    private func requestCameraAndStart() {
        switch AVCaptureDevice.authorizationStatus(for: .video) {
        case .authorized:
            permissionState = .authorized
            configureAndStartSession()
        case .notDetermined:
            AVCaptureDevice.requestAccess(for: .video) { [weak self] granted in
                DispatchQueue.main.async {
                    guard let self else { return }
                    if granted {
                        self.permissionState = .authorized
                        self.configureAndStartSession()
                    } else {
                        self.permissionState = .denied
                        self.showPermissionDeniedUI()
                    }
                }
            }
        case .denied, .restricted:
            permissionState = .denied
            showPermissionDeniedUI()
        @unknown default:
            permissionState = .denied
            showPermissionDeniedUI()
        }
    }

    // MARK: - Session lifecycle (serial queue only)

    private func configureAndStartSession() {
        captureQueue.async { [weak self] in
            guard let self else { return }
            self.configureSessionLocked()
            if !self.captureSession.isRunning {
                self.captureSession.startRunning()
            }
        }
    }

    /// Must run on `captureQueue`. Configures inputs/outputs exactly once.
    private func configureSessionLocked() {
        guard !isConfigured else { return }

        captureSession.beginConfiguration()
        defer { captureSession.commitConfiguration() }

        guard let device = AVCaptureDevice.default(for: .video) else {
            DispatchQueue.main.async { [weak self] in
                self?.showError(String(localized: "qr_camera_unavailable"))
            }
            return
        }
        guard let input = try? AVCaptureDeviceInput(device: device),
              captureSession.canAddInput(input)
        else {
            DispatchQueue.main.async { [weak self] in
                self?.showError(String(localized: "qr_camera_input_failed"))
            }
            return
        }
        captureSession.addInput(input)

        let metadataOutput = AVCaptureMetadataOutput()
        guard captureSession.canAddOutput(metadataOutput) else {
            DispatchQueue.main.async { [weak self] in
                self?.showError(String(localized: "qr_camera_output_failed"))
            }
            return
        }
        captureSession.addOutput(metadataOutput)
        metadataOutput.setMetadataObjectsDelegate(self, queue: DispatchQueue.main)
        metadataOutput.metadataObjectTypes = [.qr]

        DispatchQueue.main.async { [weak self] in
            guard let self else { return }
            let preview = AVCaptureVideoPreviewLayer(session: self.captureSession)
            preview.frame = self.view.bounds
            preview.videoGravity = .resizeAspectFill
            self.view.layer.insertSublayer(preview, at: 0)
            self.previewLayer = preview
        }

        isConfigured = true
    }

    private func stopSession() {
        captureQueue.async { [weak self] in
            guard let self else { return }
            if self.captureSession.isRunning {
                self.captureSession.stopRunning()
            }
        }
    }

    // MARK: - Dismiss

    private func setupDismissButton() {
        let dismissButton = UIButton(type: .system)
        let config = UIImage.SymbolConfiguration(pointSize: 22, weight: .medium)
        dismissButton.setImage(UIImage(systemName: "xmark.circle.fill", withConfiguration: config), for: .normal)
        dismissButton.tintColor = .white
        dismissButton.addTarget(self, action: #selector(dismissTapped), for: .touchUpInside)
        dismissButton.translatesAutoresizingMaskIntoConstraints = false
        view.addSubview(dismissButton)

        NSLayoutConstraint.activate([
            dismissButton.topAnchor.constraint(equalTo: view.safeAreaLayoutGuide.topAnchor, constant: 16),
            dismissButton.trailingAnchor.constraint(equalTo: view.trailingAnchor, constant: -16),
            dismissButton.widthAnchor.constraint(equalToConstant: 44),
            dismissButton.heightAnchor.constraint(equalToConstant: 44),
        ])
    }

    @objc private func dismissTapped() {
        onDismiss?()
    }

    // MARK: - AVCaptureMetadataOutputObjectsDelegate

    func metadataOutput(
        _ output: AVCaptureMetadataOutput,
        didOutput metadataObjects: [AVMetadataObject],
        from connection: AVCaptureConnection
    ) {
        guard !hasScanned,
              let metadataObject = metadataObjects.first as? AVMetadataMachineReadableCodeObject,
              metadataObject.type == .qr,
              let stringValue = metadataObject.stringValue
        else {
            return
        }

        guard let data = stringValue.data(using: .utf8),
              let json = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
              let type = json["type"] as? String,
              type == "serverbee_pair",
              let serverUrl = json["server_url"] as? String,
              let code = json["code"] as? String
        else {
            return
        }

        hasScanned = true
        stopSession()

        AudioServicesPlaySystemSound(SystemSoundID(kSystemSoundID_Vibrate))
        onScanned?(serverUrl, code)
    }

    // MARK: - Error + permission UI

    fileprivate func showPermissionDeniedUI() {
        // Clear any previous transient label.
        view.subviews.filter { $0.tag == 9001 }.forEach { $0.removeFromSuperview() }

        let container = UIStackView()
        container.tag = 9001
        container.axis = .vertical
        container.alignment = .center
        container.spacing = 12
        container.translatesAutoresizingMaskIntoConstraints = false

        let title = UILabel()
        title.text = String(localized: "qr_permission_denied_title")
        title.font = .systemFont(ofSize: 18, weight: .semibold)
        title.textColor = .white
        title.textAlignment = .center
        title.numberOfLines = 0

        let body = UILabel()
        body.text = String(localized: "qr_permission_denied_body")
        body.font = .systemFont(ofSize: 14)
        body.textColor = .white.withAlphaComponent(0.85)
        body.textAlignment = .center
        body.numberOfLines = 0

        let openSettings = UIButton(type: .system)
        openSettings.setTitle(String(localized: "qr_open_settings"), for: .normal)
        openSettings.titleLabel?.font = .systemFont(ofSize: 16, weight: .semibold)
        openSettings.addTarget(self, action: #selector(openSettingsTapped), for: .touchUpInside)

        container.addArrangedSubview(title)
        container.addArrangedSubview(body)
        container.addArrangedSubview(openSettings)
        view.addSubview(container)

        NSLayoutConstraint.activate([
            container.centerXAnchor.constraint(equalTo: view.centerXAnchor),
            container.centerYAnchor.constraint(equalTo: view.centerYAnchor),
            container.leadingAnchor.constraint(greaterThanOrEqualTo: view.leadingAnchor, constant: 24),
            container.trailingAnchor.constraint(lessThanOrEqualTo: view.trailingAnchor, constant: -24),
        ])
    }

    @objc private func openSettingsTapped() {
        guard let url = URL(string: UIApplication.openSettingsURLString) else { return }
        UIApplication.shared.open(url)
    }

    private func showError(_ message: String) {
        let label = UILabel()
        label.text = message
        label.textColor = .white
        label.textAlignment = .center
        label.translatesAutoresizingMaskIntoConstraints = false
        view.addSubview(label)

        NSLayoutConstraint.activate([
            label.centerXAnchor.constraint(equalTo: view.centerXAnchor),
            label.centerYAnchor.constraint(equalTo: view.centerYAnchor),
        ])
    }
}
