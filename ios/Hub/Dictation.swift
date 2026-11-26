import AVFoundation
import Speech
import SwiftUI

/// Speech to text with the iPhone's recognizer: the words show as you speak. Swedish goes through
/// Apple's servers, which hear it far better than the phone's own model. Swedish or English,
/// after the phone's language unless chosen in Settings.
@MainActor
@Observable
final class Dictation {
    var text = ""
    var listening = false
    var level: Float = 0
    var error: String?
    var started = Date()

    private let engine = AVAudioEngine()
    private var request: SFSpeechAudioBufferRecognitionRequest?
    private var task: SFSpeechRecognitionTask?

    static var language: String {
        get { UserDefaults.standard.string(forKey: "dictation.language") ?? (Locale.current.language.languageCode?.identifier == "sv" ? "sv-SE" : "en-US") }
        set { UserDefaults.standard.set(newValue, forKey: "dictation.language") }
    }

    /// Words Hub talk is full of, so the recognizer does not guess at them.
    static let words = ["Hub", "hubd", "tråd", "trådar", "pull request", "PR", "merga", "commit", "pusha", "branch",
                        "Claude", "Codex", "Hermes", "tincan", "MacBook", "PC", "FOCUS.md", "Tailscale", "release",
                        "inkorgen", "snooza", "Forman", "Knegarn", "Desk", "Aftermath", "Svelte", "Rust", "SwiftUI"]

    func start() async {
        guard !listening else { return }
        error = nil
        text = ""
        let speech = await withCheckedContinuation { c in SFSpeechRecognizer.requestAuthorization { c.resume(returning: $0) } }
        print("hub dictation: speech \(speech.rawValue)")
        guard speech == .authorized else { error = "Hub may not use speech recognition. Allow it in Settings › Hub."; return }
        guard await AVAudioApplication.requestRecordPermission() else { error = "Hub may not use the microphone. Allow it in Settings › Hub."; return }
        guard let recognizer = SFSpeechRecognizer(locale: Locale(identifier: Self.language)), recognizer.isAvailable else {
            error = "Speech recognition is not available right now."
            return
        }
        do {
            let session = AVAudioSession.sharedInstance()
            try session.setCategory(.record, mode: .measurement, options: .duckOthers)
            try session.setActive(true, options: .notifyOthersOnDeactivation)
            let request = SFSpeechAudioBufferRecognitionRequest()
            request.shouldReportPartialResults = true
            request.addsPunctuation = true
            // Apple's servers understand Swedish far better and faster than the phone's own model,
            // so only English stays on the phone.
            request.requiresOnDeviceRecognition = recognizer.supportsOnDeviceRecognition && !Self.language.hasPrefix("sv")
            request.taskHint = .dictation
            request.contextualStrings = Self.words
            self.request = request
            let input = engine.inputNode
            input.removeTap(onBus: 0)
            // The audio tap and the recognizer call back on their own threads. Closures written
            // here would inherit the main actor and trap there, so they are made outside it.
            input.installTap(onBus: 0, bufferSize: 1024, format: input.outputFormat(forBus: 0),
                             block: Self.tap(request) { [weak self] level in self?.level = level })
            engine.prepare()
            try engine.start()
            task = recognizer.recognitionTask(with: request, resultHandler: Self.handler { [weak self] text, done in
                guard let self else { return }
                if let text { self.text = text }
                if done { self.teardown() }
            })
            started = Date()
            listening = true
            print("hub dictation: listening \(Self.language)")
            Haptics.tap()
        } catch {
            self.error = error.localizedDescription
            print("hub dictation: failed \(error)")
            teardown()
        }
    }

    /// Stops listening and gives back what was said.
    @discardableResult
    func stop() -> String {
        request?.endAudio()
        teardown()
        return text.trimmingCharacters(in: .whitespacesAndNewlines)
    }

    func cancel() {
        task?.cancel()
        teardown()
        text = ""
    }

    private func teardown() {
        if engine.isRunning { engine.stop() }
        engine.inputNode.removeTap(onBus: 0)
        request = nil
        task = nil
        listening = false
        level = 0
        try? AVAudioSession.sharedInstance().setActive(false, options: .notifyOthersOnDeactivation)
    }

    nonisolated private static func tap(_ request: SFSpeechAudioBufferRecognitionRequest,
                                        level: @escaping @MainActor @Sendable (Float) -> Void) -> AVAudioNodeTapBlock {
        { buffer, _ in
            request.append(buffer)
            let l = rms(buffer)
            Task { @MainActor in level(l) }
        }
    }

    nonisolated private static func handler(_ update: @escaping @MainActor @Sendable (String?, Bool) -> Void) -> (SFSpeechRecognitionResult?, Error?) -> Void {
        { result, err in
            let text = result?.bestTranscription.formattedString
            let done = err != nil || result?.isFinal == true
            if let err { print("hub dictation: recognizer \(err)") }
            if let text { print("hub dictation: heard \(text)") }
            Task { @MainActor in update(text, done) }
        }
    }

    nonisolated private static func rms(_ buffer: AVAudioPCMBuffer) -> Float {
        guard let data = buffer.floatChannelData?[0], buffer.frameLength > 0 else { return 0 }
        var sum: Float = 0
        for i in 0..<Int(buffer.frameLength) { sum += data[i] * data[i] }
        return min(1, sqrt(sum / Float(buffer.frameLength)) * 12)
    }
}

/// The mic in a reply box: tap to talk, tap again to stop; the words go into the box.
struct DictationButton: View {
    @Environment(Store.self) private var store
    @Binding var text: String
    @State private var dictation = Dictation()
    @State private var before = ""

    var body: some View {
        Button {
            if dictation.listening {
                let said = dictation.stop()
                text = join(before, said)
            } else {
                before = text
                Task { await dictation.start() }
            }
        } label: {
            Image(systemName: dictation.listening ? "stop.circle.fill" : "mic")
                .font(.system(size: dictation.listening ? 20 : 16))
                .foregroundStyle(dictation.listening ? Hub.red : Hub.muted)
                .scaleEffect(dictation.listening ? 1 + CGFloat(dictation.level) * 0.25 : 1)
                .animation(.easeOut(duration: 0.1), value: dictation.level)
        }
        .buttonStyle(.borderless)
        .onChange(of: dictation.text) { if dictation.listening { text = join(before, dictation.text) } }
        .onChange(of: dictation.error) { if let e = dictation.error { store.flash(e) } }
    }

    private func join(_ a: String, _ b: String) -> String {
        a.isEmpty ? b : (b.isEmpty ? a : a + (a.hasSuffix(" ") ? "" : " ") + b)
    }
}
