import Foundation

/// The bit worth testing: a counter that refuses to go below zero.
public struct Counter: Equatable {
    public private(set) var value: Int

    public init(value: Int = 0) {
        self.value = max(0, value)
    }

    public mutating func increment(by amount: Int = 1) {
        value += max(0, amount)
    }

    /// Decrementing stops at zero rather than going negative.
    public mutating func decrement(by amount: Int = 1) {
        value = max(0, value - max(0, amount))
    }

    public mutating func reset() {
        value = 0
    }

    public var description: String {
        value == 1 ? "1 item" : "\(value) items"
    }
}
