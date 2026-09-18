import SwiftUI

/// A screen over `Counter`, so the sample has something to build as well as
/// something to test.
public struct CounterView: View {
    @State private var counter = Counter()

    public init() {}

    public var body: some View {
        VStack(spacing: 24) {
            Text(counter.description)
                .font(.largeTitle)
                .monospacedDigit()

            HStack(spacing: 16) {
                Button("Remove") { counter.decrement() }
                    .disabled(counter.value == 0)
                Button("Add") { counter.increment() }
            }
            .buttonStyle(.borderedProminent)

            Button("Reset") { counter.reset() }
                .disabled(counter.value == 0)
        }
        .padding()
    }
}

#Preview {
    CounterView()
}
