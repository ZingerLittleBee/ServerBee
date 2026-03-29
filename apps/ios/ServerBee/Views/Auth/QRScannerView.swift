import AVFoundation
import SwiftUI

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

final class QRScannerViewController: UIViewController, AVCaptureMetadataOutputObjectsDelegate {
    var onScanned: ((String, String) -> Void)?
    var onDismiss: (() -> Void)?

    private let captureSession = AVCaptureSession()
    private var previewLayer: AVCaptureVideoPreviewLayer?
    private var hasScanned = false

    override func viewDidLoad() {
        super.viewDidLoad()
        view.backgroundColor = .black
        setupCamera()
        setupDismissButton()
    }

    override func viewDidLayoutSubviews() {
        super.viewDidLayoutSubviews()
        previewLayer?.frame = view.bounds
    }

    override func viewWillAppear(_ animated: Bool) {
        super.viewWillAppear(animated)
        if !captureSession.isRunning {
            DispatchQueue.global(qos: .userInitiated).async { [weak self] in
                self?.captureSession.startRunning()
            }
        }
    }

    override func viewWillDisappear(_ animated: Bool) {
        super.viewWillDisappear(animated)
        if captureSession.isRunning {
            captureSession.stopRunning()
        }
    }

    // MARK: - Setup

    private func setupCamera() {
        guard let videoCaptureDevice = AVCaptureDevice.default(for: .video) else {
            showError("Camera not available")
            return
        }

        guard let videoInput = try? AVCaptureDeviceInput(device: videoCaptureDevice) else {
            showError("Cannot access camera")
            return
        }

        guard captureSession.canAddInput(videoInput) else {
            showError("Cannot configure camera")
            return
        }
        captureSession.addInput(videoInput)

        let metadataOutput = AVCaptureMetadataOutput()
        guard captureSession.canAddOutput(metadataOutput) else {
            showError("Cannot configure scanner")
            return
        }
        captureSession.addOutput(metadataOutput)

        metadataOutput.setMetadataObjectsDelegate(self, queue: DispatchQueue.main)
        metadataOutput.metadataObjectTypes = [.qr]

        let preview = AVCaptureVideoPreviewLayer(session: captureSession)
        preview.frame = view.bounds
        preview.videoGravity = .resizeAspectFill
        view.layer.addSublayer(preview)
        previewLayer = preview

        DispatchQueue.global(qos: .userInitiated).async { [weak self] in
            self?.captureSession.startRunning()
        }
    }

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
        captureSession.stopRunning()

        AudioServicesPlaySystemSound(SystemSoundID(kSystemSoundID_Vibrate))
        onScanned?(serverUrl, code)
    }

    // MARK: - Error Handling

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
